//! PCI (Peripheral Component Interconnect) Bus Enumeration and Configuration Driver.
//!
//! Implements PCI Configuration Mechanism #1 using I/O ports 0xCF8 (CONFIG_ADDRESS)
//! and 0xCFC (CONFIG_DATA). Discovers hardware peripherals, decodes standard PCI
//! configuration headers, classifies network controllers (Class 0x02), and detects
//! target network chipsets including the Realtek RTL8852BE-VT Wi-Fi 6 adapter and
//! Realtek Gaming GbE / RTL8168 Ethernet controllers on the HP Victus platform.

#![allow(dead_code)]

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use x86_64::instructions::port::Port;
use x86_64::PhysAddr;

/// PCI Configuration Mechanism #1 I/O Ports.
pub const PCI_CONFIG_ADDRESS_PORT: u16 = 0x0CF8;
pub const PCI_CONFIG_DATA_PORT: u16 = 0x0CFC;

/// Sentinel vendor ID indicating an empty or non-existent PCI slot.
pub const PCI_INVALID_VENDOR_ID: u16 = 0xFFFF;

/// PCI Class Code for Network Controllers.
pub const PCI_CLASS_NETWORK: u8 = 0x02;

/// PCI Subclasses under Network Controllers.
pub const PCI_SUBCLASS_ETHERNET: u8 = 0x00;
pub const PCI_SUBCLASS_TOKEN_RING: u8 = 0x01;
pub const PCI_SUBCLASS_FDDI: u8 = 0x02;
pub const PCI_SUBCLASS_ATM: u8 = 0x03;
pub const PCI_SUBCLASS_ISDN: u8 = 0x04;
pub const PCI_SUBCLASS_WORLDFIP: u8 = 0x05;
pub const PCI_SUBCLASS_PICMG: u8 = 0x06;
pub const PCI_SUBCLASS_INFINIBAND: u8 = 0x07;
pub const PCI_SUBCLASS_FABRIC: u8 = 0x08;
pub const PCI_SUBCLASS_WIRELESS_OTHER: u8 = 0x80;

/// Well-known PCI Vendor IDs.
pub const VENDOR_REALTEK: u16 = 0x10EC;
pub const VENDOR_INTEL: u16 = 0x8086;
pub const VENDOR_AMD: u16 = 0x1022;
pub const VENDOR_REDHAT_VIRTIO: u16 = 0x1AF4;

/// PCIe ECAM (Enhanced Configuration Access Mechanism) configuration descriptor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PciEcamConfig {
    pub base_addr: u64,
    pub segment_group: u16,
    pub start_bus: u8,
    pub end_bus: u8,
    pub mmio_virt_base: Option<u64>,
}

impl PciEcamConfig {
    pub const fn new(base_addr: u64, segment_group: u16, start_bus: u8, end_bus: u8) -> Self {
        Self {
            base_addr,
            segment_group,
            start_bus,
            end_bus,
            mmio_virt_base: None,
        }
    }
}

static PCI_ECAM: spin::Mutex<Option<PciEcamConfig>> = spin::Mutex::new(None);

/// Initializes PCIe ECAM with the physical base address and bus bounds from ACPI MCFG.
///
/// Maps the lower bus aperture (Buses 0..=7, 8 MiB) into dedicated uncacheable MMIO
/// address space with `PageTableFlags::NO_CACHE` to comply with PCI Express Base
/// Specification Section 7.2.2 on AMD Ryzen Zen 4 architectures.
pub fn set_ecam_config(mut config: PciEcamConfig) {
    let mapped_region = crate::memory::mmio::map_device_mmio(
        PhysAddr::new(config.base_addr),
        8 * 1024 * 1024,
    );
    if let Ok(reg) = mapped_region {
        log::info!(
            "[PCI] PCIe ECAM aperture 0..=7 mapped to dedicated NO_CACHE MMIO at {:#018x}",
            reg.virt_addr().as_u64()
        );
        config.mmio_virt_base = Some(reg.virt_addr().as_u64());
    } else {
        log::warn!("[PCI] Dedicated NO_CACHE MMIO mapping failed; falling back to direct physical mapping");
    }

    log::info!(
        "[PCI] PCIe ECAM (MMCONFIG) activated: Base={:#018x}, Seg={}, Buses={}..={}",
        config.base_addr,
        config.segment_group,
        config.start_bus,
        config.end_bus
    );
    *PCI_ECAM.lock() = Some(config);
}

/// Returns true if PCIe ECAM is configured and active.
pub fn is_ecam_active() -> bool {
    PCI_ECAM.lock().is_some()
}

/// Returns the active ECAM configuration if set.
pub fn get_ecam_config() -> Option<PciEcamConfig> {
    *PCI_ECAM.lock()
}

/// Calculates the virtual address for a PCIe configuration register via ECAM.
///
/// Prefers dedicated uncacheable (`NO_CACHE`) MMIO window for buses 0..=7; falls back to direct physical mapping.
#[inline(always)]
pub fn pci_ecam_virt_addr(ecam: &PciEcamConfig, bus: u8, device: u8, function: u8, offset: u16) -> Option<u64> {
    if bus < ecam.start_bus || bus > ecam.end_bus {
        return None;
    }
    let bus_offset = ((bus - ecam.start_bus) as u64) << 20;
    let dev_offset = ((device & 0x1F) as u64) << 15;
    let func_offset = ((function & 0x07) as u64) << 12;
    let reg_offset = (offset & 0x0FFF) as u64;
    let rel_offset = bus_offset + dev_offset + func_offset + reg_offset;

    if bus < 8 {
        if let Some(virt_base) = ecam.mmio_virt_base {
            return Some(virt_base + rel_offset);
        }
    }

    let phys_offset = crate::memory::phys_mem_offset();
    if phys_offset == 0 {
        return None;
    }
    let phys_addr = ecam.base_addr + rel_offset;
    Some(phys_offset + phys_addr)
}

/// Computes the 32-bit PCI Configuration Address for port 0xCF8.
#[inline(always)]
pub const fn pci_config_address(bus: u8, device: u8, function: u8, offset: u8) -> u32 {
    (1u32 << 31)
        | ((bus as u32) << 16)
        | (((device & 0x1F) as u32) << 11)
        | (((function & 0x07) as u32) << 8)
        | ((offset as u32) & 0xFC)
}

/// Reads a 32-bit dword from PCI configuration space.
///
/// Uses PCIe ECAM (memory-mapped) if configured from ACPI MCFG; falls back to legacy
/// I/O ports 0xCF8/0xCFC (Mechanism #1) when ECAM is unavailable or out-of-range.
pub unsafe fn pci_read_config_u32(bus: u8, device: u8, function: u8, offset: u8) -> u32 {
    let ecam_opt = *PCI_ECAM.lock();
    if let Some(ecam) = ecam_opt {
        if let Some(virt_addr) = pci_ecam_virt_addr(&ecam, bus, device, function, offset as u16) {
            let aligned_addr = virt_addr & !3;
            return core::ptr::read_volatile(aligned_addr as *const u32);
        }
    }

    let address = pci_config_address(bus, device, function, offset);
    let mut addr_port: Port<u32> = Port::new(PCI_CONFIG_ADDRESS_PORT);
    let mut data_port: Port<u32> = Port::new(PCI_CONFIG_DATA_PORT);
    addr_port.write(address);
    data_port.read()
}

/// Reads a 16-bit word from PCI configuration space.
pub unsafe fn pci_read_config_u16(bus: u8, device: u8, function: u8, offset: u8) -> u16 {
    let dword = pci_read_config_u32(bus, device, function, offset);
    let shift = (offset & 2) * 8;
    ((dword >> shift) & 0xFFFF) as u16
}

/// Reads an 8-bit byte from PCI configuration space.
pub unsafe fn pci_read_config_u8(bus: u8, device: u8, function: u8, offset: u8) -> u8 {
    let dword = pci_read_config_u32(bus, device, function, offset);
    let shift = (offset & 3) * 8;
    ((dword >> shift) & 0xFF) as u8
}

/// Writes a 32-bit dword to PCI configuration space.
///
/// Uses PCIe ECAM (memory-mapped) if configured from ACPI MCFG; falls back to legacy
/// I/O ports 0xCF8/0xCFC (Mechanism #1) when ECAM is unavailable or out-of-range.
pub unsafe fn pci_write_config_u32(bus: u8, device: u8, function: u8, offset: u8, value: u32) {
    let ecam_opt = *PCI_ECAM.lock();
    if let Some(ecam) = ecam_opt {
        if let Some(virt_addr) = pci_ecam_virt_addr(&ecam, bus, device, function, offset as u16) {
            let aligned_addr = virt_addr & !3;
            core::ptr::write_volatile(aligned_addr as *mut u32, value);
            return;
        }
    }

    let address = pci_config_address(bus, device, function, offset);
    let mut addr_port: Port<u32> = Port::new(PCI_CONFIG_ADDRESS_PORT);
    let mut data_port: Port<u32> = Port::new(PCI_CONFIG_DATA_PORT);
    addr_port.write(address);
    data_port.write(value);
}

/// Writes a 16-bit word to PCI configuration space.
pub unsafe fn pci_write_config_u16(bus: u8, device: u8, function: u8, offset: u8, value: u16) {
    let ecam_opt = *PCI_ECAM.lock();
    if let Some(ecam) = ecam_opt {
        if let Some(virt_addr) = pci_ecam_virt_addr(&ecam, bus, device, function, offset as u16) {
            let aligned_addr = virt_addr & !1;
            core::ptr::write_volatile(aligned_addr as *mut u16, value);
            return;
        }
    }

    let address = pci_config_address(bus, device, function, offset);
    let mut addr_port: Port<u32> = Port::new(PCI_CONFIG_ADDRESS_PORT);
    addr_port.write(address);
    let mut data_port: Port<u16> = Port::new(PCI_CONFIG_DATA_PORT + (offset as u16 & 2));
    data_port.write(value);
}

/// Writes an 8-bit byte to PCI configuration space.
pub unsafe fn pci_write_config_u8(bus: u8, device: u8, function: u8, offset: u8, value: u8) {
    let ecam_opt = *PCI_ECAM.lock();
    if let Some(ecam) = ecam_opt {
        if let Some(virt_addr) = pci_ecam_virt_addr(&ecam, bus, device, function, offset as u16) {
            core::ptr::write_volatile(virt_addr as *mut u8, value);
            return;
        }
    }

    let address = pci_config_address(bus, device, function, offset);
    let mut addr_port: Port<u32> = Port::new(PCI_CONFIG_ADDRESS_PORT);
    addr_port.write(address);
    let mut data_port: Port<u8> = Port::new(PCI_CONFIG_DATA_PORT + (offset as u16 & 3));
    data_port.write(value);
}

/// Bus, Device, and Function coordinate addressing a PCI device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PciBdf {
    pub bus: u8,
    pub device: u8,
    pub function: u8,
}

impl PciBdf {
    pub const fn new(bus: u8, device: u8, function: u8) -> Self {
        Self { bus, device, function }
    }

    pub fn to_string(&self) -> String {
        format!("{:02X}:{:02X}.{}", self.bus, self.device, self.function)
    }
}

/// Deserialized descriptor of a physical PCI device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PciDevice {
    pub bdf: PciBdf,
    pub vendor_id: u16,
    pub device_id: u16,
    pub command: u16,
    pub status: u16,
    pub revision_id: u8,
    pub prog_if: u8,
    pub subclass: u8,
    pub class_code: u8,
    pub header_type: u8,
    pub subsystem_vendor_id: u16,
    pub subsystem_id: u16,
    pub bar0: u32,
    pub bar1: u32,
    pub bar2: u32,
    pub interrupt_line: u8,
    pub interrupt_pin: u8,
}

impl PciDevice {
    /// Reads and constructs a `PciDevice` from hardware configuration space.
    pub fn probe(bus: u8, device: u8, function: u8) -> Option<Self> {
        unsafe {
            let vendor_id = pci_read_config_u16(bus, device, function, 0x00);
            if vendor_id == PCI_INVALID_VENDOR_ID || vendor_id == 0 {
                return None;
            }

            let device_id = pci_read_config_u16(bus, device, function, 0x02);
            let command = pci_read_config_u16(bus, device, function, 0x04);
            let status = pci_read_config_u16(bus, device, function, 0x06);

            let dword2 = pci_read_config_u32(bus, device, function, 0x08);
            let revision_id = (dword2 & 0xFF) as u8;
            let prog_if = ((dword2 >> 8) & 0xFF) as u8;
            let subclass = ((dword2 >> 16) & 0xFF) as u8;
            let class_code = ((dword2 >> 24) & 0xFF) as u8;

            let header_type = pci_read_config_u8(bus, device, function, 0x0E);

            let bar0 = pci_read_config_u32(bus, device, function, 0x10);
            let bar1 = pci_read_config_u32(bus, device, function, 0x14);
            let bar2 = pci_read_config_u32(bus, device, function, 0x18);

            let dword11 = pci_read_config_u32(bus, device, function, 0x2C);
            let subsystem_vendor_id = (dword11 & 0xFFFF) as u16;
            let subsystem_id = ((dword11 >> 16) & 0xFFFF) as u16;

            let interrupt_line = pci_read_config_u8(bus, device, function, 0x3C);
            let interrupt_pin = pci_read_config_u8(bus, device, function, 0x3D);

            Some(Self {
                bdf: PciBdf::new(bus, device, function),
                vendor_id,
                device_id,
                command,
                status,
                revision_id,
                prog_if,
                subclass,
                class_code,
                header_type,
                subsystem_vendor_id,
                subsystem_id,
                bar0,
                bar1,
                bar2,
                interrupt_line,
                interrupt_pin,
            })
        }
    }

    /// Returns `true` if this device is classified as a Network Controller (Class 0x02).
    pub fn is_network_controller(&self) -> bool {
        self.class_code == PCI_CLASS_NETWORK
    }

    /// Returns `true` if this device is an Ethernet Controller (Class 0x02, Subclass 0x00).
    pub fn is_ethernet_controller(&self) -> bool {
        self.class_code == PCI_CLASS_NETWORK && self.subclass == PCI_SUBCLASS_ETHERNET
    }

    /// Returns `true` if this device is a Wireless / Wi-Fi Controller (Class 0x02, Subclass 0x80).
    pub fn is_wireless_controller(&self) -> bool {
        self.class_code == PCI_CLASS_NETWORK && self.subclass == PCI_SUBCLASS_WIRELESS_OTHER
    }

    /// Returns human-readable vendor name string.
    pub fn vendor_name(&self) -> &'static str {
        match self.vendor_id {
            VENDOR_REALTEK => "Realtek Semiconductor Corp.",
            VENDOR_INTEL => "Intel Corporation",
            VENDOR_AMD => "Advanced Micro Devices, Inc.",
            VENDOR_REDHAT_VIRTIO => "Red Hat / VirtIO Network",
            0x10DE => "NVIDIA Corporation",
            0x14E4 => "Broadcom Inc.",
            0x168C => "Qualcomm Atheros",
            _ => "Unknown PCI Vendor",
        }
    }

    /// Returns human-readable device model name.
    pub fn device_name(&self) -> &'static str {
        match (self.vendor_id, self.device_id) {
            // Realtek Wi-Fi 6
            (VENDOR_REALTEK, 0xB852) | (VENDOR_REALTEK, 0x8852) | (VENDOR_REALTEK, 0xC852) => {
                "Realtek RTL8852BE-VT Wi-Fi 6 802.11ax PCIe Adapter"
            }
            (VENDOR_REALTEK, 0xB851) | (VENDOR_REALTEK, 0xB85B) => {
                "Realtek RTL8851BE 802.11ax PCIe Adapter"
            }
            (VENDOR_REALTEK, 0xA852) => {
                "Realtek RTL8852AE Wi-Fi 6 802.11ax PCIe Adapter"
            }
            // Realtek Ethernet
            (VENDOR_REALTEK, 0x8168) | (VENDOR_REALTEK, 0x8161) | (VENDOR_REALTEK, 0x8111) => {
                "Realtek Gaming GbE Controller (RTL8168/8111)"
            }
            (VENDOR_REALTEK, 0x8125) | (VENDOR_REALTEK, 0x3000) => {
                "Realtek RTL8125 2.5GbE Gaming Ethernet Controller"
            }
            (VENDOR_REALTEK, 0x8139) => "Realtek RTL-8139 Fast Ethernet NIC",
            // Intel Ethernet
            (VENDOR_INTEL, 0x100E) => "Intel 82540EM Gigabit Ethernet Controller",
            (VENDOR_INTEL, 0x100F) => "Intel 82545EM Gigabit Ethernet Controller",
            (VENDOR_INTEL, 0x1539) | (VENDOR_INTEL, 0x15F3) => "Intel I211/I225 Gigabit Ethernet Controller",
            // VirtIO
            (VENDOR_REDHAT_VIRTIO, 0x1000) | (VENDOR_REDHAT_VIRTIO, 0x1041) => {
                "VirtIO Network Paravirtualized Adapter"
            }
            _ => {
                if self.is_wireless_controller() {
                    "Generic PCI Wireless / 802.11 Network Adapter"
                } else if self.is_ethernet_controller() {
                    "Generic PCI Ethernet NIC"
                } else if self.is_network_controller() {
                    "Generic PCI Network Controller"
                } else {
                    "Standard PCI Peripheral"
                }
            }
        }
    }

    /// Returns human-readable class and subclass string.
    pub fn class_name(&self) -> &'static str {
        match (self.class_code, self.subclass) {
            (PCI_CLASS_NETWORK, PCI_SUBCLASS_ETHERNET) => "Ethernet Controller (802.3)",
            (PCI_CLASS_NETWORK, PCI_SUBCLASS_WIRELESS_OTHER) => "Wireless Network Controller (802.11 / Wi-Fi)",
            (PCI_CLASS_NETWORK, _) => "Network Controller",
            (0x01, 0x06) => "SATA / AHCI Storage Controller",
            (0x01, 0x08) => "NVMe Non-Volatile Memory Controller",
            (0x03, 0x00) => "VGA / Display Controller",
            (0x04, 0x03) => "HD Audio Controller",
            (0x06, 0x00) => "Host Bridge",
            (0x06, 0x01) => "ISA / LPC Bridge",
            (0x06, 0x04) => "PCI-to-PCI Bridge",
            (0x0C, 0x03) => "USB xHCI / Host Controller",
            _ => "Other Peripheral Device",
        }
    }

    /// Enables Bus Master (DMA) and Memory Space access in the PCI Command Register,
    /// while explicitly setting Bit 10 (Interrupt Disable) to prevent unhandled INTx assertion.
    pub unsafe fn enable_bus_master_and_memory(&self) {
        let cmd = pci_read_config_u16(self.bdf.bus, self.bdf.device, self.bdf.function, 0x04);
        // Bit 1 = Memory Space, Bit 2 = Bus Master, Bit 10 = Interrupt Disable (INTx# masked)
        let new_cmd = (cmd | 0x0006) | 0x0400;
        pci_write_config_u16(self.bdf.bus, self.bdf.device, self.bdf.function, 0x04, new_cmd);
    }

    /// Resolves the primary MMIO physical base address from BAR0, BAR2, or BAR4 (32-bit or 64-bit).
    pub fn mmio_base(&self) -> Option<u64> {
        // Check BAR0 first
        if self.bar0 != 0 && (self.bar0 & 1) == 0 {
            let is_64bit = (self.bar0 & 0x06) == 0x04;
            let low = (self.bar0 & 0xFFFF_FFF0) as u64;
            if is_64bit {
                let high = (self.bar1 as u64) << 32;
                return Some(high | low);
            } else {
                return Some(low);
            }
        }
        // Check BAR2 if BAR0 is I/O
        if self.bar2 != 0 && (self.bar2 & 1) == 0 {
            let is_64bit = (self.bar2 & 0x06) == 0x04;
            let low = (self.bar2 & 0xFFFF_FFF0) as u64;
            if is_64bit {
                let bar3 = unsafe {
                    pci_read_config_u32(self.bdf.bus, self.bdf.device, self.bdf.function, 0x1C)
                };
                let high = (bar3 as u64) << 32;
                return Some(high | low);
            } else {
                return Some(low);
            }
        }
        // Check BAR4 if BAR0 and BAR2 are I/O
        let bar4 = unsafe { pci_read_config_u32(self.bdf.bus, self.bdf.device, self.bdf.function, 0x20) };
        if bar4 != 0 && (bar4 & 1) == 0 {
            let is_64bit = (bar4 & 0x06) == 0x04;
            let low = (bar4 & 0xFFFF_FFF0) as u64;
            if is_64bit {
                let bar5 = unsafe { pci_read_config_u32(self.bdf.bus, self.bdf.device, self.bdf.function, 0x24) };
                let high = (bar5 as u64) << 32;
                return Some(high | low);
            } else {
                return Some(low);
            }
        }
        None
    }

    /// Locates the specified PCI capability in configuration space (if present).
    pub fn find_capability(&self, target_cap_id: u8) -> Option<u8> {
        if (self.status & 0x0010) == 0 {
            return None;
        }

        let mut cap_ptr = unsafe {
            pci_read_config_u8(self.bdf.bus, self.bdf.device, self.bdf.function, 0x34)
        } & 0xFC;

        let mut count = 0;
        while cap_ptr >= 0x40 && count < 48 {
            let cap_id = unsafe {
                pci_read_config_u8(self.bdf.bus, self.bdf.device, self.bdf.function, cap_ptr)
            };
            if cap_id == target_cap_id {
                return Some(cap_ptr);
            }
            cap_ptr = unsafe {
                pci_read_config_u8(self.bdf.bus, self.bdf.device, self.bdf.function, cap_ptr + 1)
            } & 0xFC;
            count += 1;
        }
        None
    }

    /// Sets the PCI Power Management state to D0 (fully operational).
    ///
    /// Reads PMCSR (offset +4 in PM capability); if device is in D1/D2/D3hot, transitions to D0.
    pub unsafe fn set_power_state_d0(&self) -> bool {
        if let Some(pm_offset) = self.find_capability(0x01) {
            let pmcsr = pci_read_config_u16(self.bdf.bus, self.bdf.device, self.bdf.function, pm_offset + 4);
            let current_state = pmcsr & 0x0003;
            if current_state != 0 {
                let new_pmcsr = pmcsr & !0x0003;
                pci_write_config_u16(self.bdf.bus, self.bdf.device, self.bdf.function, pm_offset + 4, new_pmcsr);
                for _ in 0..100_000 {
                    core::hint::spin_loop();
                }
            }
            true
        } else {
            false
        }
    }

    /// Disables PCIe Active State Power Management (ASPM L0s and L1).
    ///
    /// Prevents DMA transaction stalls and timeouts on AMD Ryzen platforms.
    pub unsafe fn disable_pcie_aspm(&self) -> bool {
        if let Some(exp_offset) = self.find_capability(0x10) {
            let lnkctl = pci_read_config_u16(self.bdf.bus, self.bdf.device, self.bdf.function, exp_offset + 0x10);
            let new_lnkctl = lnkctl & !0x0003;
            pci_write_config_u16(self.bdf.bus, self.bdf.device, self.bdf.function, exp_offset + 0x10, new_lnkctl);
            true
        } else {
            false
        }
    }
}

/// Classification status of a probed PCI slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PciSlotStatus {
    Present,
    Empty,      // 0xFFFF or floating bus 0xFFFFFFFF
    AllZero,    // 0x00000000
    ReadError,
}

/// Raw hardware probe telemetry for a specific PCI BDF coordinate.
#[derive(Clone, Debug)]
pub struct PciSlotProbe {
    pub bdf: PciBdf,
    pub status: PciSlotStatus,
    pub raw_dword0: u32,
    pub vendor_id: u16,
    pub device_id: u16,
}

/// Probes a single PCI slot function and returns its raw hardware status.
pub fn probe_slot_raw(bus: u8, device: u8, function: u8) -> PciSlotProbe {
    let dword0 = unsafe { pci_read_config_u32(bus, device, function, 0x00) };
    let vendor_id = (dword0 & 0xFFFF) as u16;
    let device_id = ((dword0 >> 16) & 0xFFFF) as u16;

    let status = if dword0 == 0xFFFF_FFFF || vendor_id == 0xFFFF {
        PciSlotStatus::Empty
    } else if dword0 == 0x0000_0000 || vendor_id == 0x0000 {
        PciSlotStatus::AllZero
    } else {
        PciSlotStatus::Present
    };

    PciSlotProbe {
        bdf: PciBdf::new(bus, device, function),
        status,
        raw_dword0: dword0,
        vendor_id,
        device_id,
    }
}

/// Result of comparing Bus 0, Device 0, Function 0 reads between Legacy I/O port 0xCF8 and PCIe ECAM.
#[derive(Clone, Debug)]
pub struct EcamVerificationResult {
    pub ecam_configured: bool,
    pub ecam_base_phys: u64,
    pub ecam_mapped_virt: Option<u64>,
    pub ecam_mapping_mode: &'static str,
    pub legacy_dword0: u32,
    pub ecam_dword0: u32,
    pub legacy_vendor: u16,
    pub legacy_device: u16,
    pub ecam_vendor: u16,
    pub ecam_device: u16,
    pub matches: bool,
    pub status_summary: &'static str,
}

/// Verifies whether PCIe ECAM configuration reads match Legacy Mechanism #1 on Bus 0.
pub fn verify_bus0_ecam_vs_legacy() -> EcamVerificationResult {
    // 1. Read Bus 0, Dev 0, Func 0 via Legacy I/O Ports 0xCF8 / 0xCFC
    let legacy_dword0 = unsafe {
        let address = pci_config_address(0, 0, 0, 0);
        let mut addr_port: Port<u32> = Port::new(PCI_CONFIG_ADDRESS_PORT);
        let mut data_port: Port<u32> = Port::new(PCI_CONFIG_DATA_PORT);
        addr_port.write(address);
        data_port.read()
    };
    let legacy_vendor = (legacy_dword0 & 0xFFFF) as u16;
    let legacy_device = ((legacy_dword0 >> 16) & 0xFFFF) as u16;

    // 2. Read Bus 0, Dev 0, Func 0 via PCIe ECAM (MMCONFIG)
    let ecam_opt = get_ecam_config();
    let (ecam_configured, ecam_base_phys, ecam_mapped_virt, ecam_mapping_mode, ecam_dword0) = match ecam_opt {
        Some(ecam) => {
            let mode = if ecam.mmio_virt_base.is_some() {
                "Dedicated NO_CACHE MMIO"
            } else {
                "Physical Memory Direct-Map (WB)"
            };
            if let Some(virt) = pci_ecam_virt_addr(&ecam, 0, 0, 0, 0) {
                let val = unsafe { core::ptr::read_volatile(virt as *const u32) };
                (true, ecam.base_addr, Some(virt), mode, val)
            } else {
                (true, ecam.base_addr, None, "Translation Failed", 0xFFFF_FFFF)
            }
        }
        None => (false, 0, None, "ECAM Inactive (ACPI MCFG Absent)", 0xFFFF_FFFF),
    };
    let ecam_vendor = (ecam_dword0 & 0xFFFF) as u16;
    let ecam_device = ((ecam_dword0 >> 16) & 0xFFFF) as u16;

    let matches = ecam_configured
        && legacy_dword0 == ecam_dword0
        && legacy_dword0 != 0xFFFF_FFFF
        && legacy_dword0 != 0;

    let status_summary = if !ecam_configured {
        "ECAM INACTIVE: ACPI MCFG table not provided; fallback to I/O ports 0xCF8/0xCFC."
    } else if matches {
        "VERIFIED MATCH: ECAM reads operational; bit-identical to legacy I/O port on Bus 0."
    } else if ecam_dword0 == 0xFFFF_FFFF && legacy_dword0 != 0xFFFF_FFFF {
        "ECAM READ FAULT: Legacy port read Host Bridge, but ECAM returned floating bus (0xFFFFFFFF)."
    } else if ecam_dword0 == 0x0000_0000 {
        "ECAM ZERO FAULT: ECAM MMIO returned 0x00000000 (unmapped or unpowered aperture)."
    } else {
        "MISMATCH: Legacy port and ECAM returned different configuration values."
    };

    EcamVerificationResult {
        ecam_configured,
        ecam_base_phys,
        ecam_mapped_virt,
        ecam_mapping_mode,
        legacy_dword0,
        ecam_dword0,
        legacy_vendor,
        legacy_device,
        ecam_vendor,
        ecam_device,
        matches,
        status_summary,
    }
}

/// Decoded descriptor of a PCI-to-PCI Bridge (Root Port).
#[derive(Clone, Debug)]
pub struct PciBridgeInfo {
    pub bdf: PciBdf,
    pub vendor_id: u16,
    pub device_id: u16,
    pub primary_bus: u8,
    pub secondary_bus: u8,
    pub subordinate_bus: u8,
    pub command: u16,
    pub secondary_status: u16,
    pub bridge_control: u16,
    pub downstream_device_present: bool,
    pub downstream_vid: u16,
    pub downstream_did: u16,
}

/// Scans Bus 0 for all PCI-to-PCI bridges (Class 0x06, Subclass 0x04) and inspects secondary routing.
pub fn scan_pci_bridges() -> Vec<PciBridgeInfo> {
    let mut bridges = Vec::new();
    for dev in 0..32 {
        for func in 0..8 {
            let class_code = unsafe { pci_read_config_u8(0, dev, func, 0x0B) };
            let subclass = unsafe { pci_read_config_u8(0, dev, func, 0x0A) };
            let header_type = unsafe { pci_read_config_u8(0, dev, func, 0x0E) };

            if (class_code == 0x06 && subclass == 0x04) || (header_type & 0x7F) == 0x01 {
                let vendor_id = unsafe { pci_read_config_u16(0, dev, func, 0x00) };
                if vendor_id == 0xFFFF || vendor_id == 0 {
                    continue;
                }
                let device_id = unsafe { pci_read_config_u16(0, dev, func, 0x02) };
                let command = unsafe { pci_read_config_u16(0, dev, func, 0x04) };
                let primary_bus = unsafe { pci_read_config_u8(0, dev, func, 0x18) };
                let secondary_bus = unsafe { pci_read_config_u8(0, dev, func, 0x19) };
                let subordinate_bus = unsafe { pci_read_config_u8(0, dev, func, 0x1A) };
                let secondary_status = unsafe { pci_read_config_u16(0, dev, func, 0x1E) };
                let bridge_control = unsafe { pci_read_config_u16(0, dev, func, 0x3E) };

                let (downstream_device_present, downstream_vid, downstream_did) = if secondary_bus != 0 {
                    let d_probe = probe_slot_raw(secondary_bus, 0, 0);
                    if d_probe.status == PciSlotStatus::Present {
                        (true, d_probe.vendor_id, d_probe.device_id)
                    } else {
                        (false, d_probe.vendor_id, d_probe.device_id)
                    }
                } else {
                    (false, 0xFFFF, 0xFFFF)
                };

                bridges.push(PciBridgeInfo {
                    bdf: PciBdf::new(0, dev, func),
                    vendor_id,
                    device_id,
                    primary_bus,
                    secondary_bus,
                    subordinate_bus,
                    command,
                    secondary_status,
                    bridge_control,
                    downstream_device_present,
                    downstream_vid,
                    downstream_did,
                });
            }
        }
    }
    bridges
}

/// Comprehensive hardware bus telemetry snapshot for diagnostics.
#[derive(Clone, Debug)]
pub struct PciProbeTelemetry {
    pub total_slots_scanned: usize,
    pub present_count: usize,
    pub empty_count: usize,
    pub all_zero_count: usize,
    pub devices: Vec<PciDevice>,
    pub bridges: Vec<PciBridgeInfo>,
    pub ecam_verification: EcamVerificationResult,
}

pub static GLOBAL_PCI_TELEMETRY: spin::Mutex<Option<PciProbeTelemetry>> = spin::Mutex::new(None);

/// Returns the latest hardware probe telemetry snapshot if available.
pub fn get_pci_telemetry() -> Option<PciProbeTelemetry> {
    GLOBAL_PCI_TELEMETRY.lock().clone()
}

/// Performs a hardware scan across all PCI buses (32 devices, 8 functions)
/// and returns all detected peripheral devices while capturing complete diagnostic telemetry.
///
/// Uses PCIe ECAM (MMCONFIG) across buses when configured by ACPI MCFG,
/// guaranteeing discovery of secondary bus endpoints (Wi-Fi, GbE, NVMe) on AMD Ryzen platforms.
pub fn scan_pci_bus() -> Vec<PciDevice> {
    let mut devices = Vec::new();
    let mut total_slots = 0;
    let mut empty_slots = 0;
    let mut zero_slots = 0;

    let (start_bus, max_bus) = match get_ecam_config() {
        Some(ecam) => (ecam.start_bus, ecam.end_bus.min(15)),
        None => (0, 7), // Legacy fallback: scan buses 0..=7
    };

    for bus in start_bus..=max_bus {
        for dev in 0..32 {
            let p0 = probe_slot_raw(bus, dev, 0);
            total_slots += 1;
            match p0.status {
                PciSlotStatus::Empty => empty_slots += 1,
                PciSlotStatus::AllZero => zero_slots += 1,
                PciSlotStatus::ReadError => empty_slots += 1,
                PciSlotStatus::Present => {
                    if let Some(func0) = PciDevice::probe(bus, dev, 0) {
                        let is_multi_function = (func0.header_type & 0x80) != 0;
                        devices.push(func0);

                        if is_multi_function {
                            for func in 1..8 {
                                let pf = probe_slot_raw(bus, dev, func);
                                total_slots += 1;
                                match pf.status {
                                    PciSlotStatus::Empty => empty_slots += 1,
                                    PciSlotStatus::AllZero => zero_slots += 1,
                                    PciSlotStatus::ReadError => empty_slots += 1,
                                    PciSlotStatus::Present => {
                                        if let Some(pci_dev) = PciDevice::probe(bus, dev, func) {
                                            devices.push(pci_dev);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let bridges = scan_pci_bridges();
    let ecam_verification = verify_bus0_ecam_vs_legacy();

    let telemetry = PciProbeTelemetry {
        total_slots_scanned: total_slots,
        present_count: devices.len(),
        empty_count: empty_slots,
        all_zero_count: zero_slots,
        devices: devices.clone(),
        bridges,
        ecam_verification,
    };
    *GLOBAL_PCI_TELEMETRY.lock() = Some(telemetry);

    devices
}

/// Performs a hardware scan and returns only devices classified as Network Controllers (Class 0x02).
pub fn scan_pci_network_devices() -> Vec<PciDevice> {
    scan_pci_bus()
        .into_iter()
        .filter(|dev| dev.is_network_controller())
        .collect()
}
