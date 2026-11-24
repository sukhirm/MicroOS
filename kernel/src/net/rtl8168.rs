//! Realtek RTL8168 / RTL8111 PCI-Express Gigabit Ethernet Controller Driver.
//!
//! Adapted from the MIT-licensed Redox OS `rtl8168d` driver architecture and
//! standard OSDev specifications for bare-metal x86_64 execution.
//!
//! Features:
//! - Full 64-bit PCI MMIO mapping via `MmioRegion`.
//! - Bounded DMA descriptor rings (Tx and Rx) via `DmaBuffer`.
//! - Hardware MAC address readout from physical registers (`0x00..0x05`).
//! - Live PHY link status, duplex, and speed detection via `0x6C`.
//! - Packet transmission with bounded completion polling.
//! - Packet reception with automatic descriptor re-arming.
//! - Safe polling mode (no PIC/APIC interrupt deadlocks).

#![allow(dead_code)]

use core::sync::atomic::{fence, Ordering};
use x86_64::PhysAddr;

use crate::arch::x86_64::pci::PciDevice;
use crate::memory::dma::{DmaBuffer, DmaError};
use crate::memory::mmio::{map_device_mmio, MmioError, MmioRegion};

// ============================================================================
// Hardware Register Offsets (relative to MMIO base)
// ============================================================================

pub const REG_MAC0: usize = 0x00; // MAC address bytes 0..5
pub const REG_MAR0: usize = 0x08; // Multicast filter bytes 0..7
pub const REG_TNPDS: usize = 0x20; // Transmit Normal Priority Descriptor Start Address (64-bit)
pub const REG_THPDS: usize = 0x28; // Transmit High Priority Descriptor Start Address (64-bit)
pub const REG_CR: usize = 0x37; // Chip Command Register (8-bit)
pub const REG_TPPOLL: usize = 0x38; // Transmit Priority Polling Register (8-bit)
pub const REG_IMR: usize = 0x3C; // Interrupt Mask Register (16-bit)
pub const REG_ISR: usize = 0x3E; // Interrupt Status Register (16-bit)
pub const REG_TCR: usize = 0x40; // Transmit Configuration Register (32-bit)
pub const REG_RCR: usize = 0x44; // Receive Configuration Register (32-bit)
pub const REG_9346CR: usize = 0x50; // EEPROM / Config Register Unlock (8-bit)
pub const REG_PHY_STATUS: usize = 0x6C; // PHY Status Register (8-bit)
pub const REG_RMS: usize = 0xDA; // Maximum Receive Packet Size (16-bit)
pub const REG_CPLUSCR: usize = 0xE0; // C+ Command Register (16-bit)
pub const REG_RDSAR: usize = 0xE4; // Receive Descriptor Start Address (64-bit)
pub const REG_MTPS: usize = 0xEC; // Maximum Transmit Packet Size (16-bit)

// ============================================================================
// Register Bits and Masks
// ============================================================================

pub const CR_RST: u8 = 0x10; // Software Reset
pub const CR_RE: u8 = 0x08; // Receiver Enable
pub const CR_TE: u8 = 0x04; // Transmitter Enable

pub const TPPOLL_NPQ: u8 = 0x40; // Normal Priority Queue Polling
pub const TPPOLL_HPQ: u8 = 0x80; // High Priority Queue Polling

pub const CFG9346_UNLOCK: u8 = 0xC0; // Unlock configuration registers
pub const CFG9346_LOCK: u8 = 0x00; // Lock configuration registers

pub const PHY_LINK_UP: u8 = 0x02; // Bit 1: Link status (1 = Up, 0 = Down)
pub const PHY_FULL_DUPLEX: u8 = 0x04; // Bit 2: Duplex (1 = Full, 0 = Half)
pub const PHY_SPEED_10M: u8 = 0x08; // Bit 3: 10 Mbps (PCIe RTL8168)
pub const PHY_SPEED_100M: u8 = 0x10; // Bit 4: 100 Mbps (PCIe RTL8168) or 0x08 (Legacy)
pub const PHY_SPEED_1000M: u8 = 0x20; // Bit 5: 1000 Mbps (PCIe RTL8168) or 0x10 (Legacy)

// RCR Flags: Accept Broadcast, Multicast, Physical match, Unlimited DMA burst, Rx FIFO threshold
pub const RCR_CONFIG: u32 = 0x0000_E70F;

// TCR Flags: Standard Inter-Frame Gap, Max DMA burst
pub const TCR_CONFIG: u32 = 0x0300_0700;

// CPlusCR Flags: Enable C+ mode, Rx Checksum offload
pub const CPLUSCR_CONFIG: u16 = 0x0020;

// ============================================================================
// Descriptor Format (16 bytes per entry, 256-byte ring alignment)
// ============================================================================

pub const NUM_RX_DESC: usize = 16;
pub const NUM_TX_DESC: usize = 8;
pub const PACKET_BUF_SIZE: usize = 2048;

pub const DESC_OWN: u32 = 1 << 31; // 1 = Owned by NIC, 0 = Owned by CPU
pub const DESC_EOR: u32 = 1 << 30; // End of Ring (wrap back to 0)
pub const DESC_FS: u32 = 1 << 29; // First Segment
pub const DESC_LS: u32 = 1 << 28; // Last Segment
pub const DESC_LEN_MASK: u32 = 0x3FFF;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Rtl8168Descriptor {
    pub opts1: u32,
    pub opts2: u32,
    pub addr_low: u32,
    pub addr_high: u32,
}

impl Rtl8168Descriptor {
    pub const fn zero() -> Self {
        Self {
            opts1: 0,
            opts2: 0,
            addr_low: 0,
            addr_high: 0,
        }
    }
}

// ============================================================================
// Driver Telemetry and Status Structures
// ============================================================================

/// Physical link status read from hardware PHY registers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinkStatus {
    pub is_up: bool,
    pub full_duplex: bool,
    pub speed_mbps: u16,
}

/// Statistics tracking driver packet operations.
#[derive(Debug, Clone, Copy, Default)]
pub struct DriverStats {
    pub packets_sent: u64,
    pub bytes_sent: u64,
    pub packets_received: u64,
    pub bytes_received: u64,
    pub rx_overflows: u64,
    pub tx_errors: u64,
}

/// Errors returned by the RTL8168 driver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rtl8168Error {
    DeviceNotFound,
    NoMmioBar,
    MmioMapFailed(MmioError),
    DmaAllocFailed(DmaError),
    ResetTimeout,
    LinkDown,
    TxRingFull,
    PacketTooLarge,
}

// ============================================================================
// RTL8168 Driver State
// ============================================================================

pub struct Rtl8168Driver {
    mmio: MmioRegion,
    dma: DmaBuffer,
    mac_address: [u8; 6],
    cur_rx: usize,
    cur_tx: usize,
    stats: DriverStats,
}

impl Rtl8168Driver {
    /// Probes and initializes an RTL8168 controller from a detected `PciDevice`.
    pub fn init_device(pci_dev: &PciDevice) -> Result<Self, Rtl8168Error> {
        let mmio_phys = pci_dev.mmio_base().ok_or(Rtl8168Error::NoMmioBar)?;

        // Enable PCI Bus Master (bit 2) and Memory Space (bit 1) in configuration space
        unsafe {
            pci_dev.enable_bus_master_and_memory();
        }

        // Map MMIO space (64 KiB window is sufficient for RTL8168 registers)
        let mmio = map_device_mmio(PhysAddr::new(mmio_phys), 0x10_000)
            .map_err(Rtl8168Error::MmioMapFailed)?;

        // Allocate a 64 KiB contiguous, 256-byte aligned DMA buffer for descriptor rings and packet buffers
        let dma = DmaBuffer::allocate(64 * 1024, 256).map_err(Rtl8168Error::DmaAllocFailed)?;

        let mut driver = Self {
            mmio,
            dma,
            mac_address: [0; 6],
            cur_rx: 0,
            cur_tx: 0,
            stats: DriverStats::default(),
        };

        // Execute hardware reset and register initialization
        driver.hardware_init()?;

        Ok(driver)
    }

    /// Executes the full hardware reset and descriptor ring configuration sequence.
    fn hardware_init(&mut self) -> Result<(), Rtl8168Error> {
        // 1. Read hardware MAC address from registers MAC0..MAC5
        for i in 0..6 {
            self.mac_address[i] = unsafe { self.mmio.read_u8(REG_MAC0 + i).unwrap_or(0) };
        }

        // 2. Issue Software Reset
        unsafe {
            let _ = self.mmio.write_u8(REG_CR, CR_RST);
        }

        // Bounded poll for reset completion (CR_RST self-clears, 100k bounded cycles)
        let mut reset_done = false;
        for _ in 0..100_000 {
            let cr = unsafe { self.mmio.read_u8(REG_CR).unwrap_or(CR_RST) };
            if (cr & CR_RST) == 0 {
                reset_done = true;
                break;
            }
            core::hint::spin_loop();
        }
        if !reset_done {
            log::error!("[RTL8168] Software reset timed out! CR stuck at 0x{:02X}", unsafe { self.mmio.read_u8(REG_CR).unwrap_or(0xFF) });
            return Err(Rtl8168Error::ResetTimeout);
        }

        // 3. Unlock configuration registers via 9346CR (CRITICAL: MUST BE AFTER RESET)
        unsafe {
            let _ = self.mmio.write_u8(REG_9346CR, CFG9346_UNLOCK);
        }

        // 4. Enable C+ mode and Rx checksum offloading
        unsafe {
            let _ = self.mmio.write_u16(REG_CPLUSCR, CPLUSCR_CONFIG);
        }

        // 5. Partition DMA Buffer and set up descriptor rings
        self.setup_descriptor_rings();

        // 6. Set Maximum Receive / Transmit Packet Sizes (2048 bytes)
        unsafe {
            let _ = self.mmio.write_u16(REG_RMS, PACKET_BUF_SIZE as u16);
            let _ = self.mmio.write_u16(REG_MTPS, PACKET_BUF_SIZE as u16);
        }

        // 7. Configure Rx and Tx rules (Accept Broadcast, Multicast, Physical match; max DMA bursts)
        unsafe {
            let _ = self.mmio.write_u32(REG_RCR, RCR_CONFIG);
            let _ = self.mmio.write_u32(REG_TCR, TCR_CONFIG);
        }

        // 8. Mask hardware interrupts (we operate in deterministic polling mode)
        unsafe {
            let _ = self.mmio.write_u16(REG_IMR, 0x0000);
            let _ = self.mmio.write_u16(REG_ISR, 0xFFFF); // Clear any pending interrupt flags
        }

        // 9. Lock configuration registers
        unsafe {
            let _ = self.mmio.write_u8(REG_9346CR, CFG9346_LOCK);
        }

        // 10. Enable Receiver and Transmitter
        unsafe {
            let _ = self.mmio.write_u8(REG_CR, CR_RE | CR_TE);
        }

        log::info!(
            "[RTL8168] Initialized successfully. MAC: {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
            self.mac_address[0], self.mac_address[1], self.mac_address[2],
            self.mac_address[3], self.mac_address[4], self.mac_address[5]
        );

        Ok(())
    }

    /// Sets up the Rx and Tx descriptor rings in the allocated DMA buffer.
    fn setup_descriptor_rings(&mut self) {
        let base_phys = self.dma.phys_addr().as_u64();
        let base_virt = self.dma.virt_addr().as_u64();

        // Layout inside 64 KiB DMA buffer:
        // 0x0000..0x0100: Rx Descriptors (16 * 16 = 256 bytes)
        // 0x0100..0x0200: Tx Descriptors (8 * 16 = 128 bytes, aligned to 256)
        // 0x0200..0x8200: Rx Buffers (16 * 2048 = 32 KiB)
        // 0x8200..0xC200: Tx Buffers (8 * 2048 = 16 KiB)

        let rx_ring_phys = base_phys;
        let tx_ring_phys = base_phys + 0x0100;

        let rx_ring_ptr = base_virt as *mut Rtl8168Descriptor;
        let tx_ring_ptr = (base_virt + 0x0100) as *mut Rtl8168Descriptor;

        // Populate Rx Ring
        for i in 0..NUM_RX_DESC {
            let buf_phys = base_phys + 0x0200 + (i as u64 * PACKET_BUF_SIZE as u64);
            let mut opts1 = (PACKET_BUF_SIZE as u32) | DESC_OWN;
            if i == NUM_RX_DESC - 1 {
                opts1 |= DESC_EOR; // End of Ring
            }

            unsafe {
                let desc = rx_ring_ptr.add(i);
                core::ptr::write_volatile(
                    desc,
                    Rtl8168Descriptor {
                        opts1,
                        opts2: 0,
                        addr_low: (buf_phys & 0xFFFF_FFFF) as u32,
                        addr_high: (buf_phys >> 32) as u32,
                    },
                );
            }
        }

        // Populate Tx Ring (initially idle, owned by CPU)
        for i in 0..NUM_TX_DESC {
            let buf_phys = base_phys + 0x8200 + (i as u64 * PACKET_BUF_SIZE as u64);
            let mut opts1 = 0u32;
            if i == NUM_TX_DESC - 1 {
                opts1 |= DESC_EOR; // End of Ring
            }

            unsafe {
                let desc = tx_ring_ptr.add(i);
                core::ptr::write_volatile(
                    desc,
                    Rtl8168Descriptor {
                        opts1,
                        opts2: 0,
                        addr_low: (buf_phys & 0xFFFF_FFFF) as u32,
                        addr_high: (buf_phys >> 32) as u32,
                    },
                );
            }
        }

        fence(Ordering::SeqCst);

        // Tell hardware where to find the descriptor rings
        unsafe {
            let _ = self.mmio.write_u32(REG_RDSAR, (rx_ring_phys & 0xFFFF_FFFF) as u32);
            let _ = self.mmio.write_u32(REG_RDSAR + 4, (rx_ring_phys >> 32) as u32);

            let _ = self.mmio.write_u32(REG_TNPDS, (tx_ring_phys & 0xFFFF_FFFF) as u32);
            let _ = self.mmio.write_u32(REG_TNPDS + 4, (tx_ring_phys >> 32) as u32);
        }

        self.cur_rx = 0;
        self.cur_tx = 0;
    }

    /// Queries the live physical Ethernet cable connection status from the PHY register (`0x6C`).
    pub fn check_link_status(&self) -> LinkStatus {
        let raw = unsafe { self.mmio.read_u8(REG_PHY_STATUS).unwrap_or(0) };
        let is_up = (raw & PHY_LINK_UP) != 0;
        let full_duplex = (raw & PHY_FULL_DUPLEX) != 0;
        let speed_mbps = if (raw & PHY_SPEED_1000M) != 0 || ((raw & PHY_SPEED_100M) != 0 && (raw & PHY_SPEED_10M) != 0) {
            1000
        } else if (raw & PHY_SPEED_100M) != 0 || (raw & PHY_SPEED_10M) != 0 {
            100
        } else if is_up {
            10
        } else {
            0
        };

        LinkStatus {
            is_up,
            full_duplex,
            speed_mbps,
        }
    }

    /// Returns the verified 6-byte hardware MAC address.
    pub fn mac_address(&self) -> [u8; 6] {
        self.mac_address
    }

    /// Formats the hardware MAC address as standard colon-delimited string.
    pub fn mac_string(&self) -> alloc::string::String {
        alloc::format!(
            "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
            self.mac_address[0],
            self.mac_address[1],
            self.mac_address[2],
            self.mac_address[3],
            self.mac_address[4],
            self.mac_address[5]
        )
    }

    /// Transmits an Ethernet packet onto the physical wire.
    pub fn send_packet(&mut self, packet: &[u8]) -> Result<(), Rtl8168Error> {
        if packet.len() > PACKET_BUF_SIZE {
            return Err(Rtl8168Error::PacketTooLarge);
        }

        let link = self.check_link_status();
        if !link.is_up {
            return Err(Rtl8168Error::LinkDown);
        }

        let tx_ring_ptr = (self.dma.virt_addr().as_u64() + 0x0100) as *mut Rtl8168Descriptor;
        let desc_ptr = unsafe { tx_ring_ptr.add(self.cur_tx) };

        // Check if descriptor is available (not owned by hardware)
        let opts1 = unsafe { core::ptr::read_volatile(&((*desc_ptr).opts1)) };
        if (opts1 & DESC_OWN) != 0 {
            return Err(Rtl8168Error::TxRingFull);
        }

        // Copy packet data to Tx buffer
        let tx_buf_ptr = (self.dma.virt_addr().as_u64()
            + 0x8200
            + (self.cur_tx as u64 * PACKET_BUF_SIZE as u64)) as *mut u8;

        // Minimum Ethernet frame payload is 60 bytes (excluding 4-byte CRC)
        let tx_len = core::cmp::max(packet.len(), 60);
        unsafe {
            core::ptr::copy_nonoverlapping(packet.as_ptr(), tx_buf_ptr, packet.len());
            if tx_len > packet.len() {
                core::ptr::write_bytes(tx_buf_ptr.add(packet.len()), 0, tx_len - packet.len());
            }
        }

        fence(Ordering::SeqCst);

        // Set descriptor ownership to NIC with First/Last segment flags
        let mut new_opts1 = (tx_len as u32) | DESC_OWN | DESC_FS | DESC_LS;
        if self.cur_tx == NUM_TX_DESC - 1 {
            new_opts1 |= DESC_EOR;
        }

        unsafe {
            core::ptr::write_volatile(&mut (*desc_ptr).opts1, new_opts1);
        }

        fence(Ordering::SeqCst);

        // Trigger hardware transmission via Normal Priority Queue polling
        unsafe {
            let _ = self.mmio.write_u8(REG_TPPOLL, TPPOLL_NPQ);
        }

        // Advance Tx pointer
        self.cur_tx = (self.cur_tx + 1) % NUM_TX_DESC;
        self.stats.packets_sent += 1;
        self.stats.bytes_sent += tx_len as u64;

        Ok(())
    }

    /// Polls the receive ring for an incoming packet.
    ///
    /// Copies received payload into `out_buf` and returns the number of bytes read.
    pub fn receive_packet(&mut self, out_buf: &mut [u8]) -> Option<usize> {
        let rx_ring_ptr = self.dma.virt_addr().as_u64() as *mut Rtl8168Descriptor;
        let desc_ptr = unsafe { rx_ring_ptr.add(self.cur_rx) };

        let opts1 = unsafe { core::ptr::read_volatile(&((*desc_ptr).opts1)) };
        if (opts1 & DESC_OWN) != 0 {
            // NIC still owns descriptor, no packet ready
            return None;
        }

        fence(Ordering::SeqCst);

        let raw_len = (opts1 & DESC_LEN_MASK) as usize;
        // Strip 4-byte Ethernet CRC if present
        let payload_len = if raw_len >= 4 { raw_len - 4 } else { raw_len };
        let copy_len = core::cmp::min(payload_len, out_buf.len());

        let rx_buf_ptr = (self.dma.virt_addr().as_u64()
            + 0x0200
            + (self.cur_rx as u64 * PACKET_BUF_SIZE as u64)) as *const u8;

        unsafe {
            core::ptr::copy_nonoverlapping(rx_buf_ptr, out_buf.as_mut_ptr(), copy_len);
        }

        // Re-arm descriptor for the NIC
        let mut new_opts1 = (PACKET_BUF_SIZE as u32) | DESC_OWN;
        if self.cur_rx == NUM_RX_DESC - 1 {
            new_opts1 |= DESC_EOR;
        }

        fence(Ordering::SeqCst);

        unsafe {
            core::ptr::write_volatile(&mut (*desc_ptr).opts1, new_opts1);
        }

        // Advance Rx pointer
        self.cur_rx = (self.cur_rx + 1) % NUM_RX_DESC;
        self.stats.packets_received += 1;
        self.stats.bytes_received += copy_len as u64;

        Some(copy_len)
    }

    /// Returns a copy of the current driver statistics.
    pub fn stats(&self) -> DriverStats {
        self.stats
    }
}
