//! Realtek RTL8852BE PCIe DMA Descriptor Rings (Milestone 3 & 4).
//!
//! Implements hardware TX Buffer Descriptors (TxBD) and RX Buffer Descriptors (RxBD)
//! compliant with Realtek RTL8852B PCIe specifications (Linux rtw89 pci.h / pci.c).
//! Provides bounded synchronous submission for FWCMD (Channel 12) and asynchronous
//! rings for 802.11 management/data transmission and reception.

#![allow(dead_code)]

use crate::memory::dma::{DmaBuffer, DmaError};
use crate::memory::mmio::MmioRegion;
use crate::net::rtl8852be_fw::build_txwd_fwcmd;

// ============================================================================
// RTL8852B PCIe DMA Register Offsets
// ============================================================================

pub const R_AX_PCIE_INIT_CFG1: usize = 0x1000;
pub const B_AX_RXHCI_EN: u32 = 1 << 13;
pub const B_AX_TXHCI_EN: u32 = 1 << 11;
pub const B_AX_RXBD_MODE: u32 = 1 << 18;

// Channel 12 (FWCMD Queue)
pub const R_AX_CH12_TXBD_NUM: usize = 0x1038;
pub const R_AX_CH12_TXBD_IDX: usize = 0x1080;
pub const R_AX_CH12_TXBD_DESA_L: usize = 0x1160;
pub const R_AX_CH12_TXBD_DESA_H: usize = 0x1164;

// Channel 8 (Management Queue Band 0)
pub const R_AX_CH8_TXBD_NUM: usize = 0x1034;
pub const R_AX_CH8_TXBD_IDX: usize = 0x1078;
pub const R_AX_CH8_TXBD_DESA_L: usize = 0x1150;
pub const R_AX_CH8_TXBD_DESA_H: usize = 0x1154;

// Channel 0 (AC_BE Queue Band 0)
pub const R_AX_ACH0_TXBD_NUM: usize = 0x1024;
pub const R_AX_ACH0_TXBD_IDX: usize = 0x1058;
pub const R_AX_ACH0_TXBD_DESA_L: usize = 0x1110;
pub const R_AX_ACH0_TXBD_DESA_H: usize = 0x1114;

// RX Queue (RXQ)
pub const R_AX_RXQ_RXBD_NUM: usize = 0x1020;
pub const R_AX_RXQ_RXBD_IDX: usize = 0x1050;
pub const R_AX_RXQ_RXBD_DESA_L: usize = 0x1100;
pub const R_AX_RXQ_RXBD_DESA_H: usize = 0x1104;

pub const RTW89_PCI_TXBD_OPT_LS: u16 = 1 << 14;
pub const TXBD_HW_IDX_MASK: u32 = 0x0FFF_0000;
pub const TXBD_HOST_IDX_MASK: u32 = 0x0000_0FFF;

pub const DEFAULT_TX_RING_LEN: usize = 16;
pub const DEFAULT_RX_RING_LEN: usize = 32;
pub const BUFFER_SIZE_PER_PKT: usize = 2048;

/// 8-byte Hardware TX Buffer Descriptor (TxBD).
#[repr(C, packed)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Rtw89TxBd {
    pub length: u16,
    pub opt: u16,
    pub dma: u32,
}

/// 8-byte Hardware RX Buffer Descriptor (RxBD).
#[repr(C, packed)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Rtw89RxBd {
    pub buf_size: u16,
    pub opt: u16,
    pub dma: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DmaRingError {
    BufferAllocationFailed(DmaError),
    PayloadTooLarge,
    RingFull,
    HardwareDmaTimeout,
    MmioAccessFailed,
}

/// TX Ring Manager for FWCMD or 802.11 Transmissions.
pub struct Rtl8852beTxRing {
    pub ring_dma: DmaBuffer,
    pub buffer_dma: DmaBuffer,
    pub len: usize,
    pub host_idx: usize,
    pub reg_num: usize,
    pub reg_idx: usize,
    pub reg_desa_l: usize,
    pub reg_desa_h: usize,
}

impl Rtl8852beTxRing {
    /// Internal constructor for creating and initializing a TX DMA Ring for any specified hardware channel.
    pub fn new_channel_ring(
        mmio: &MmioRegion,
        reg_num: usize,
        reg_idx: usize,
        reg_desa_l: usize,
        reg_desa_h: usize,
    ) -> Result<Self, DmaRingError> {
        let len = DEFAULT_TX_RING_LEN;
        let ring_size = len * core::mem::size_of::<Rtw89TxBd>(); // 16 * 8 = 128 bytes
        let ring_dma = DmaBuffer::allocate(ring_size, 256)
            .map_err(DmaRingError::BufferAllocationFailed)?;

        let buffer_pool_size = len * BUFFER_SIZE_PER_PKT; // 16 * 2048 = 32 KiB
        let buffer_dma = DmaBuffer::allocate(buffer_pool_size, 4096)
            .map_err(DmaRingError::BufferAllocationFailed)?;

        // Zero descriptor table
        unsafe {
            core::ptr::write_bytes(ring_dma.virt_addr().as_mut_ptr::<u8>(), 0, ring_size);
        }

        let ring = Self {
            ring_dma,
            buffer_dma,
            len,
            host_idx: 0,
            reg_num,
            reg_idx,
            reg_desa_l,
            reg_desa_h,
        };

        // Program ring physical address into device registers
        let ring_phys = ring.ring_dma.phys_addr().as_u64();
        unsafe {
            mmio.write_u32(ring.reg_desa_l, (ring_phys & 0xFFFF_FFFF) as u32)
                .map_err(|_| DmaRingError::MmioAccessFailed)?;
            mmio.write_u32(ring.reg_desa_h, ((ring_phys >> 32) & 0xFFFF_FFFF) as u32)
                .map_err(|_| DmaRingError::MmioAccessFailed)?;
            mmio.write_u16(ring.reg_num, len as u16)
                .map_err(|_| DmaRingError::MmioAccessFailed)?;
            mmio.write_u16(ring.reg_idx, 0)
                .map_err(|_| DmaRingError::MmioAccessFailed)?;
        }

        Ok(ring)
    }

    /// Creates and initializes a new TX DMA Ring for Channel 12 (FWCMD Queue).
    pub fn new_fwcmd_ring(mmio: &MmioRegion) -> Result<Self, DmaRingError> {
        Self::new_channel_ring(
            mmio,
            R_AX_CH12_TXBD_NUM,
            R_AX_CH12_TXBD_IDX,
            R_AX_CH12_TXBD_DESA_L,
            R_AX_CH12_TXBD_DESA_H,
        )
    }

    /// Creates and initializes a new TX DMA Ring for Channel 8 (802.11 Management Queue).
    pub fn new_mgmt_ring(mmio: &MmioRegion) -> Result<Self, DmaRingError> {
        Self::new_channel_ring(
            mmio,
            R_AX_CH8_TXBD_NUM,
            R_AX_CH8_TXBD_IDX,
            R_AX_CH8_TXBD_DESA_L,
            R_AX_CH8_TXBD_DESA_H,
        )
    }

    /// Creates and initializes a new TX DMA Ring for Channel 0 (802.11 AC_BE Data Queue).
    pub fn new_data_ring(mmio: &MmioRegion) -> Result<Self, DmaRingError> {
        Self::new_channel_ring(
            mmio,
            R_AX_ACH0_TXBD_NUM,
            R_AX_ACH0_TXBD_IDX,
            R_AX_ACH0_TXBD_DESA_L,
            R_AX_ACH0_TXBD_DESA_H,
        )
    }

    /// Asynchronously submits an 802.11 packet (Management or Data) into the TX descriptor ring.
    pub fn submit_packet(
        &mut self,
        mmio: &MmioRegion,
        payload: &[u8],
    ) -> Result<(), DmaRingError> {
        let total_pkt_len = 16 + payload.len();
        if total_pkt_len > BUFFER_SIZE_PER_PKT {
            return Err(DmaRingError::PayloadTooLarge);
        }

        let slot = self.host_idx;
        let buf_offset = slot * BUFFER_SIZE_PER_PKT;
        let buf_phys = self.buffer_dma.phys_addr().as_u64() + buf_offset as u64;

        // Build standard 16-byte TXWD descriptor prefix for 802.11 frames (rx_type = 0)
        let dword0: u32 = (total_pkt_len as u32) & 0x3FFF;
        let mut txwd = [0u8; 16];
        txwd[0..4].copy_from_slice(&dword0.to_le_bytes());

        unsafe {
            let buf_ptr = self.buffer_dma.virt_addr().as_mut_ptr::<u8>().add(buf_offset);
            core::ptr::copy_nonoverlapping(txwd.as_ptr(), buf_ptr, 16);
            core::ptr::copy_nonoverlapping(payload.as_ptr(), buf_ptr.add(16), payload.len());
        }

        let opt_hi = (((buf_phys >> 32) & 0xFF) as u16) << 6;
        let opt = RTW89_PCI_TXBD_OPT_LS | opt_hi;
        let desc = Rtw89TxBd {
            length: total_pkt_len as u16,
            opt,
            dma: (buf_phys & 0xFFFF_FFFF) as u32,
        };

        unsafe {
            let desc_ptr = (self.ring_dma.virt_addr().as_mut_ptr::<u8>()
                as *mut Rtw89TxBd)
                .add(slot);
            core::ptr::write_volatile(desc_ptr, desc);
        }

        let next_idx = (self.host_idx + 1) % self.len;
        self.host_idx = next_idx;

        unsafe {
            mmio.write_u16(self.reg_idx, next_idx as u16)
                .map_err(|_| DmaRingError::MmioAccessFailed)?;
        }

        Ok(())
    }

    /// Submits a FWCMD packet (with 16-byte TXWD prefix) and synchronously waits for DMA completion.
    pub fn submit_fwcmd_sync(
        &mut self,
        mmio: &MmioRegion,
        payload: &[u8],
        is_fwdl: bool,
    ) -> Result<(), DmaRingError> {
        let total_pkt_len = 16 + payload.len();
        if total_pkt_len > BUFFER_SIZE_PER_PKT {
            return Err(DmaRingError::PayloadTooLarge);
        }

        let slot = self.host_idx;
        let buf_offset = slot * BUFFER_SIZE_PER_PKT;
        let buf_phys = self.buffer_dma.phys_addr().as_u64() + buf_offset as u64;

        // Build 16-byte TXWD prefix and copy payload
        let txwd = build_txwd_fwcmd(total_pkt_len, is_fwdl);
        unsafe {
            let buf_ptr = self.buffer_dma.virt_addr().as_mut_ptr::<u8>().add(buf_offset);
            core::ptr::copy_nonoverlapping(txwd.as_ptr(), buf_ptr, 16);
            core::ptr::copy_nonoverlapping(payload.as_ptr(), buf_ptr.add(16), payload.len());
        }

        // Configure TxBD descriptor in ring
        let opt_hi = (((buf_phys >> 32) & 0xFF) as u16) << 6;
        let opt = RTW89_PCI_TXBD_OPT_LS | opt_hi;
        let desc = Rtw89TxBd {
            length: total_pkt_len as u16,
            opt,
            dma: (buf_phys & 0xFFFF_FFFF) as u32,
        };

        unsafe {
            let desc_ptr = (self.ring_dma.virt_addr().as_mut_ptr::<u8>()
                as *mut Rtw89TxBd)
                .add(slot);
            core::ptr::write_volatile(desc_ptr, desc);
        }

        // Advance host index and notify hardware
        let next_idx = (self.host_idx + 1) % self.len;
        self.host_idx = next_idx;

        unsafe {
            mmio.write_u16(self.reg_idx, next_idx as u16)
                .map_err(|_| DmaRingError::MmioAccessFailed)?;
        }

        // Bounded poll for hardware completion: wait until hardware read index matches next_idx
        for _ in 0..100_000 {
            let reg_val = unsafe { mmio.read_u32(self.reg_idx).unwrap_or(0xFFFF_FFFF) };
            if reg_val == 0xFFFF_FFFF {
                return Err(DmaRingError::HardwareDmaTimeout);
            }
            let hw_idx = ((reg_val >> 16) & 0x0FFF) as usize;
            if hw_idx == next_idx {
                return Ok(());
            }
            core::hint::spin_loop();
        }

        Err(DmaRingError::HardwareDmaTimeout)
    }
}

/// RX Ring Manager for receiving 802.11 management frames (Beacons, Probe Responses) and data.
pub struct Rtl8852beRxRing {
    pub ring_dma: DmaBuffer,
    pub buffer_dma: DmaBuffer,
    pub len: usize,
    pub read_idx: usize,
    pub reg_num: usize,
    pub reg_idx: usize,
    pub reg_desa_l: usize,
    pub reg_desa_h: usize,
}

impl Rtl8852beRxRing {
    /// Creates and initializes the RX DMA Ring.
    pub fn new_rx_ring(mmio: &MmioRegion) -> Result<Self, DmaRingError> {
        let len = DEFAULT_RX_RING_LEN;
        let ring_size = len * core::mem::size_of::<Rtw89RxBd>(); // 32 * 8 = 256 bytes
        let ring_dma = DmaBuffer::allocate(ring_size, 256)
            .map_err(DmaRingError::BufferAllocationFailed)?;

        let buffer_pool_size = len * BUFFER_SIZE_PER_PKT; // 32 * 2048 = 64 KiB
        let buffer_dma = DmaBuffer::allocate(buffer_pool_size, 4096)
            .map_err(DmaRingError::BufferAllocationFailed)?;

        let ring = Self {
            ring_dma,
            buffer_dma,
            len,
            read_idx: 0,
            reg_num: R_AX_RXQ_RXBD_NUM,
            reg_idx: R_AX_RXQ_RXBD_IDX,
            reg_desa_l: R_AX_RXQ_RXBD_DESA_L,
            reg_desa_h: R_AX_RXQ_RXBD_DESA_H,
        };

        // Populate initial descriptors with buffer physical addresses
        for i in 0..len {
            let buf_phys = ring.buffer_dma.phys_addr().as_u64() + (i * BUFFER_SIZE_PER_PKT) as u64;
            let opt_hi = (((buf_phys >> 32) & 0xFF) as u16) << 6;
            let desc = Rtw89RxBd {
                buf_size: BUFFER_SIZE_PER_PKT as u16,
                opt: opt_hi,
                dma: (buf_phys & 0xFFFF_FFFF) as u32,
            };
            unsafe {
                let desc_ptr = (ring.ring_dma.virt_addr().as_mut_ptr::<u8>()
                    as *mut Rtw89RxBd)
                    .add(i);
                core::ptr::write_volatile(desc_ptr, desc);
            }
        }

        // Program device registers
        let ring_phys = ring.ring_dma.phys_addr().as_u64();
        unsafe {
            mmio.write_u32(ring.reg_desa_l, (ring_phys & 0xFFFF_FFFF) as u32)
                .map_err(|_| DmaRingError::MmioAccessFailed)?;
            mmio.write_u32(ring.reg_desa_h, ((ring_phys >> 32) & 0xFFFF_FFFF) as u32)
                .map_err(|_| DmaRingError::MmioAccessFailed)?;
            mmio.write_u16(ring.reg_num, len as u16)
                .map_err(|_| DmaRingError::MmioAccessFailed)?;
            // Announce all descriptors are ready to receive
            mmio.write_u16(ring.reg_idx, (len - 1) as u16)
                .map_err(|_| DmaRingError::MmioAccessFailed)?;
        }

        Ok(ring)
    }

    /// Polls for an incoming received 802.11 packet.
    ///
    /// Copies received frame bytes into `out_buf` and re-submits descriptor to hardware.
    pub fn poll_receive(&mut self, mmio: &MmioRegion, out_buf: &mut [u8]) -> Option<usize> {
        let reg_val = unsafe { mmio.read_u32(self.reg_idx).ok()? };
        let hw_idx = ((reg_val >> 16) & 0x0FFF) as usize;

        // Bounded check: if hardware index is out of ring bounds (e.g. uninitialized,
        // in reset, or floating bus returning 0xFFFF_FFFF), safely return None.
        if hw_idx >= self.len || hw_idx == self.read_idx {
            return None; // No valid packet available
        }

        let slot = self.read_idx;
        let buf_offset = slot * BUFFER_SIZE_PER_PKT;

        // In rtw89, RX frame contains a short rxdesc prefix (16 bytes) followed by 802.11 MPDU
        let buf_ptr = unsafe { self.buffer_dma.virt_addr().as_ptr::<u8>().add(buf_offset) };
        let dword0 = unsafe { core::ptr::read_volatile(buf_ptr as *const u32) };
        let pkt_len = (dword0 & 0x3FFF) as usize;

        if pkt_len == 0 || pkt_len > BUFFER_SIZE_PER_PKT {
            self.read_idx = (self.read_idx + 1) % self.len;
            return None;
        }

        let copy_len = pkt_len.min(out_buf.len());
        unsafe {
            core::ptr::copy_nonoverlapping(buf_ptr, out_buf.as_mut_ptr(), copy_len);
        }

        // Reclaim slot and notify hardware
        self.read_idx = (self.read_idx + 1) % self.len;
        unsafe {
            let _ = mmio.write_u16(self.reg_idx, ((self.read_idx + self.len - 1) % self.len) as u16);
        }

        Some(copy_len)
    }
}
