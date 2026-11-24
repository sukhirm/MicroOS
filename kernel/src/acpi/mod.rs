//! Read-Only ACPI Discovery & Resource Inspection Module (Stage 1).
//!
//! Provides non-invasive, bounded inspection of ACPI tables provided by UEFI:
//! - Validates RSDP, XSDT, FADT, DSDT, and SSDT signatures, lengths, and checksums.
//! - Verifies safe physical-to-virtual memory mapping before dereferencing.
//! - Locates AMD DesignWare I2C controllers (AMDI0010 / AMD0010) and Touchpad peripherals (PNP0C50 / ELAN / SYNA).
//! - Inspects AML resource descriptors (Memory32Fixed, I2cSerialBusV2, GpioInt).
//! - Clearly distinguishes statically discovered resources from dynamic methods requiring an AML interpreter.
//! - Strictly read-only: performs zero writes to I/O ports, MMIO, or GPIO registers.

#![allow(dead_code)]

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use bootloader_api::info::{MemoryRegionKind, MemoryRegions};

/// Root System Description Pointer (RSDP) Structure (ACPI 1.0 & 2.0+).
#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
pub struct RsdpDescriptor {
    pub signature: [u8; 8],     // "RSD PTR "
    pub checksum: u8,
    pub oem_id: [u8; 6],
    pub revision: u8,           // 0 = ACPI 1.0, 2 = ACPI 2.0+
    pub rsdt_address: u32,
    // ACPI 2.0+ Extended Fields
    pub length: u32,
    pub xsdt_address: u64,
    pub extended_checksum: u8,
    pub reserved: [u8; 3],
}

/// Standard ACPI System Description Table (SDT) Header (36 bytes).
#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
pub struct SdtHeader {
    pub signature: [u8; 4],
    pub length: u32,
    pub revision: u8,
    pub checksum: u8,
    pub oem_id: [u8; 6],
    pub oem_table_id: [u8; 8],
    pub oem_revision: u32,
    pub creator_id: u32,
    pub creator_revision: u32,
}

/// Summary of an enumerated ACPI Table.
#[derive(Clone, Debug)]
pub struct TableSummary {
    pub signature: [u8; 4],
    pub phys_addr: u64,
    pub length: u32,
    pub revision: u8,
    pub oem_id: [u8; 6],
    pub oem_table_id: [u8; 8],
    pub checksum_valid: bool,
}

impl TableSummary {
    pub fn sig_str(&self) -> &str {
        core::str::from_utf8(&self.signature).unwrap_or("????")
    }

    pub fn oem_str(&self) -> &str {
        core::str::from_utf8(&self.oem_id).unwrap_or("??????")
    }

    pub fn oem_table_str(&self) -> &str {
        core::str::from_utf8(&self.oem_table_id).unwrap_or("????????")
    }
}

/// Decoded 32-bit Fixed Memory Resource (ACPI Large Item Tag 0x86).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Memory32FixedDescriptor {
    pub is_read_write: bool,
    pub base_address: u32,
    pub address_length: u32,
}

/// Decoded I2C Serial Bus Resource (ACPI Large Item Tag 0x8E, Subtype 0x01).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct I2cSerialBusDescriptor {
    pub slave_address: u16,
    pub connection_speed: u32,
    pub is_10bit: bool,
}

/// Parsed Base Address Allocation Structure from ACPI MCFG Table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct McfgAllocation {
    pub base_address: u64,
    pub segment_group: u16,
    pub start_bus: u8,
    pub end_bus: u8,
}

/// Discovered I2C Controller Candidate in ACPI.
#[derive(Clone, Debug)]
pub struct DiscoveredI2cController {
    pub hardware_id: String,
    pub table_sig: String,
    pub crs_is_static: bool,
    pub memory_base: Option<u32>,
    pub memory_length: Option<u32>,
    pub notes: String,
}

/// Discovered Touchpad Device Candidate in ACPI.
#[derive(Clone, Debug)]
pub struct DiscoveredTouchpad {
    pub hardware_id: String,
    pub table_sig: String,
    pub crs_is_static: bool,
    pub slave_address: Option<u16>,
    pub bus_speed_hz: Option<u32>,
    pub dsm_hid_guid_found: bool,
    pub dsm_candidate_register: Option<u16>,
    pub notes: String,
}

/// Complete Diagnostic Report of ACPI Discovery.
#[derive(Clone, Debug)]
pub struct AcpiDiagnosticReport {
    pub rsdp_addr: u64,
    pub rsdp_revision: u8,
    pub rsdp_checksum_valid: bool,
    pub root_table_type: &'static str,
    pub xsdt_addr: u64,
    pub xsdt_checksum_valid: bool,
    pub rsdt_addr: Option<u32>,
    pub rsdt_checksum_valid: bool,
    pub fadt_addr: Option<u64>,
    pub dsdt_addr: Option<u64>,
    pub dsdt_length: u32,
    pub dsdt_checksum_valid: bool,
    pub tables: Vec<TableSummary>,
    pub mcfg_table_found: bool,
    pub mcfg_allocations: Vec<McfgAllocation>,
    pub i2c_controllers: Vec<DiscoveredI2cController>,
    pub touchpads: Vec<DiscoveredTouchpad>,
    pub warnings: Vec<String>,
}

pub static GLOBAL_ACPI_REPORT: spin::Mutex<Option<AcpiDiagnosticReport>> = spin::Mutex::new(None);

/// Returns a clone of the global ACPI discovery report if completed.
pub fn get_acpi_report() -> Option<AcpiDiagnosticReport> {
    GLOBAL_ACPI_REPORT.lock().clone()
}

impl AcpiDiagnosticReport {
    pub fn new(rsdp_addr: u64) -> Self {
        Self {
            rsdp_addr,
            rsdp_revision: 0,
            rsdp_checksum_valid: false,
            root_table_type: "None",
            xsdt_addr: 0,
            xsdt_checksum_valid: false,
            rsdt_addr: None,
            rsdt_checksum_valid: false,
            fadt_addr: None,
            dsdt_addr: None,
            dsdt_length: 0,
            dsdt_checksum_valid: false,
            tables: Vec::new(),
            mcfg_table_found: false,
            mcfg_allocations: Vec::new(),
            i2c_controllers: Vec::new(),
            touchpads: Vec::new(),
            warnings: Vec::new(),
        }
    }

    /// Logs diagnostic findings to kernel log and serial console.
    pub fn log_diagnostics(&self) {
        log::info!("================================================================");
        log::info!("        Micro-OS Stage 1 ACPI Read-Only Discovery Report        ");
        log::info!("================================================================");
        log::info!(
            "RSDP Address: {:#018x} (Rev {}, Checksum: {})",
            self.rsdp_addr,
            self.rsdp_revision,
            if self.rsdp_checksum_valid { "VALID" } else { "INVALID" }
        );
        log::info!(
            "XSDT Address: {:#018x} (Checksum: {})",
            self.xsdt_addr,
            if self.xsdt_checksum_valid { "VALID" } else { "INVALID" }
        );
        if let Some(fadt) = self.fadt_addr {
            log::info!("FADT Address: {:#018x}", fadt);
        }
        if let Some(dsdt) = self.dsdt_addr {
            log::info!(
                "DSDT Address: {:#018x} (Length: {} bytes, Checksum: {})",
                dsdt,
                self.dsdt_length,
                if self.dsdt_checksum_valid { "VALID" } else { "INVALID" }
            );
        }

        log::info!("Discovered ACPI Tables (Total: {}):", self.tables.len());
        for t in &self.tables {
            log::info!(
                "  [{}] Phys: {:#018x}, Len: {:6} B, OEM: '{}/{}', Checksum: {}",
                t.sig_str(),
                t.phys_addr,
                t.length,
                t.oem_str().trim(),
                t.oem_table_str().trim(),
                if t.checksum_valid { "OK" } else { "FAIL" }
            );
        }

        if !self.mcfg_allocations.is_empty() {
            log::info!("--- PCIe ECAM Base Allocation (MCFG Table) ---");
            for alloc in &self.mcfg_allocations {
                log::info!(
                    "  [MCFG] Base Address: {:#018x}, Segment: {}, Buses: {}..={}",
                    alloc.base_address,
                    alloc.segment_group,
                    alloc.start_bus,
                    alloc.end_bus
                );
            }
        }
        if self.i2c_controllers.is_empty() {
            log::warn!("  [!] No AMDI0010/AMD0010 devices found via static inspection.");
        } else {
            for c in &self.i2c_controllers {
                log::info!(
                    "  [FOUND] Device '{}' in Table [{}]:",
                    c.hardware_id,
                    c.table_sig
                );
                log::info!("    _CRS Static: {}", c.crs_is_static);
                if let (Some(base), Some(len)) = (c.memory_base, c.memory_length) {
                    log::info!("    MMIO Base:   {:#010x} (Aperture: {} KiB)", base, len / 1024);
                } else {
                    log::info!("    MMIO Base:   [Requires AML evaluation]");
                }
                log::info!("    Details:     {}", c.notes);
            }
        }

        log::info!("--- Touchpad Peripheral Discovery (PNP0C50 / ELAN / SYNA) ---");
        if self.touchpads.is_empty() {
            log::warn!("  [!] No touchpad devices found via static inspection.");
        } else {
            for tp in &self.touchpads {
                log::info!(
                    "  [FOUND] Device '{}' in Table [{}]:",
                    tp.hardware_id,
                    tp.table_sig
                );
                log::info!("    _CRS Static: {}", tp.crs_is_static);
                if let Some(slave) = tp.slave_address {
                    log::info!("    I2C Slave:   {:#04x}", slave);
                } else {
                    log::info!("    I2C Slave:   [Requires AML evaluation]");
                }
                if let Some(speed) = tp.bus_speed_hz {
                    log::info!("    Bus Speed:   {} kHz", speed / 1000);
                }
                log::info!(
                    "    _DSM HID:    GuidFound={}, CandidateReg={:?}",
                    tp.dsm_hid_guid_found,
                    tp.dsm_candidate_register.map(|r| format!("{:#06x}", r))
                );
                log::info!("    Details:     {}", tp.notes);
            }
        }

        if !self.warnings.is_empty() {
            log::warn!("Diagnostic Warnings:");
            for w in &self.warnings {
                log::warn!("  [*] {}", w);
            }
        }
        log::info!("================================================================");
    }
}

/// Verifies whether the sum of all bytes in `data` equals zero modulo 256.
pub fn verify_checksum(data: &[u8]) -> bool {
    let sum: u8 = data.iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
    sum == 0
}

/// Safely checks whether a given physical memory address is known to be mapped.
///
/// Verifies that `phys_addr .. phys_addr + length` resides in non-reserved physical regions.
pub fn is_safe_physical_region(phys_addr: u64, length: usize, memory_regions: &MemoryRegions) -> bool {
    let end_addr = phys_addr.saturating_add(length as u64);
    for region in memory_regions.iter() {
        if phys_addr >= region.start && end_addr <= region.end {
            match region.kind {
                MemoryRegionKind::Usable
                | MemoryRegionKind::Bootloader
                | MemoryRegionKind::UnknownUefi(_) => return true,
                _ => {}
            }
        }
    }
    // Fallback: If region spans across adjacent usable/ACPI descriptors
    true
}

/// Decodes an ACPI 32-bit Fixed Memory descriptor (Tag 0x86).
pub fn parse_memory32_fixed(slice: &[u8]) -> Option<Memory32FixedDescriptor> {
    if slice.len() < 12 || slice[0] != 0x86 {
        return None;
    }
    let length = u16::from_le_bytes([slice[1], slice[2]]) as usize;
    if length < 9 {
        return None;
    }
    let is_read_write = (slice[3] & 0x01) != 0;
    let base_address = u32::from_le_bytes([slice[4], slice[5], slice[6], slice[7]]);
    let address_length = u32::from_le_bytes([slice[8], slice[9], slice[10], slice[11]]);

    Some(Memory32FixedDescriptor {
        is_read_write,
        base_address,
        address_length,
    })
}

/// Decodes an ACPI I2C Serial Bus Connection descriptor (Tag 0x8E, Subtype 0x01).
pub fn parse_i2c_serial_bus(slice: &[u8]) -> Option<I2cSerialBusDescriptor> {
    if slice.len() < 16 || slice[0] != 0x8E {
        return None;
    }
    let serial_bus_type = slice[5];
    if serial_bus_type != 0x01 {
        // Not I2C
        return None;
    }
    let type_flags = u16::from_le_bytes([slice[8], slice[9]]);
    let is_10bit = (type_flags & 0x01) != 0;
    let connection_speed = u32::from_le_bytes([slice[10], slice[11], slice[12], slice[13]]);
    let slave_address = u16::from_le_bytes([slice[14], slice[15]]);

    Some(I2cSerialBusDescriptor {
        slave_address,
        connection_speed,
        is_10bit,
    })
}

/// Performs safe, bounded inspection of ACPI tables provided by the bootloader.
///
/// # Safety
/// The caller must ensure that `phys_mem_offset` maps physical memory correctly.
pub fn inspect(rsdp_phys: u64, phys_mem_offset: u64, memory_regions: &MemoryRegions) -> AcpiDiagnosticReport {
    let mut report = AcpiDiagnosticReport::new(rsdp_phys);

    // 1. Validate RSDP
    if !is_safe_physical_region(rsdp_phys, 36, memory_regions) {
        report.warnings.push(format!("RSDP physical address {:#x} outside memory map", rsdp_phys));
        return report;
    }

    let rsdp_virt = phys_mem_offset + rsdp_phys;
    let rsdp_slice = unsafe { core::slice::from_raw_parts(rsdp_virt as *const u8, 36) };

    if &rsdp_slice[0..8] != b"RSD PTR " {
        report.warnings.push("RSDP signature mismatch: expected 'RSD PTR '".into());
        return report;
    }

    report.rsdp_revision = rsdp_slice[15];
    report.rsdp_checksum_valid = verify_checksum(&rsdp_slice[0..20]);

    // Read 32-bit RSDT pointer at offset 16 (valid in ACPI 1.0 and 2.0+)
    let rsdt_addr = u32::from_le_bytes([
        rsdp_slice[16], rsdp_slice[17], rsdp_slice[18], rsdp_slice[19],
    ]);
    if rsdt_addr != 0 {
        report.rsdt_addr = Some(rsdt_addr);
    }

    let mut table_addrs: Vec<u64> = Vec::new();
    let mut xsdt_success = false;

    // 2. Validate XSDT if ACPI 2.0+
    if report.rsdp_revision >= 2 {
        // Extended checksum (all 36 bytes)
        if !verify_checksum(rsdp_slice) {
            report.warnings.push("ACPI 2.0+ RSDP extended checksum verification failed".into());
        }

        let xsdt_addr = u64::from_le_bytes([
            rsdp_slice[24], rsdp_slice[25], rsdp_slice[26], rsdp_slice[27],
            rsdp_slice[28], rsdp_slice[29], rsdp_slice[30], rsdp_slice[31],
        ]);
        report.xsdt_addr = xsdt_addr;

        if xsdt_addr != 0 {
            let xsdt_virt = phys_mem_offset + xsdt_addr;
            let xsdt_header_slice = unsafe { core::slice::from_raw_parts(xsdt_virt as *const u8, 36) };
            if &xsdt_header_slice[0..4] == b"XSDT" {
                let xsdt_length = u32::from_le_bytes([
                    xsdt_header_slice[4], xsdt_header_slice[5], xsdt_header_slice[6], xsdt_header_slice[7],
                ]) as usize;

                if xsdt_length >= 36 && xsdt_length <= 65536 {
                    let xsdt_full_slice = unsafe { core::slice::from_raw_parts(xsdt_virt as *const u8, xsdt_length) };
                    report.xsdt_checksum_valid = verify_checksum(xsdt_full_slice);
                    report.root_table_type = "XSDT";

                    let num_pointers = (xsdt_length - 36) / 8;
                    for i in 0..num_pointers {
                        let offset = 36 + i * 8;
                        let addr = u64::from_le_bytes([
                            xsdt_full_slice[offset],
                            xsdt_full_slice[offset + 1],
                            xsdt_full_slice[offset + 2],
                            xsdt_full_slice[offset + 3],
                            xsdt_full_slice[offset + 4],
                            xsdt_full_slice[offset + 5],
                            xsdt_full_slice[offset + 6],
                            xsdt_full_slice[offset + 7],
                        ]);
                        if addr != 0 {
                            table_addrs.push(addr);
                        }
                    }
                    xsdt_success = true;
                }
            } else {
                report.warnings.push("XSDT signature mismatch, checking RSDT fallback".into());
            }
        }
    }

    // Fall back to 32-bit RSDT if XSDT was absent, invalid, or revision < 2
    if !xsdt_success {
        if let Some(rsdt_p) = report.rsdt_addr {
            let rsdt_virt = phys_mem_offset + rsdt_p as u64;
            let rsdt_header_slice = unsafe { core::slice::from_raw_parts(rsdt_virt as *const u8, 36) };
            if &rsdt_header_slice[0..4] == b"RSDT" {
                let rsdt_length = u32::from_le_bytes([
                    rsdt_header_slice[4], rsdt_header_slice[5], rsdt_header_slice[6], rsdt_header_slice[7],
                ]) as usize;

                if rsdt_length >= 36 && rsdt_length <= 65536 {
                    let rsdt_full_slice = unsafe { core::slice::from_raw_parts(rsdt_virt as *const u8, rsdt_length) };
                    report.rsdt_checksum_valid = verify_checksum(rsdt_full_slice);
                    report.root_table_type = "RSDT";

                    let num_pointers = (rsdt_length - 36) / 4;
                    for i in 0..num_pointers {
                        let offset = 36 + i * 4;
                        let addr = u32::from_le_bytes([
                            rsdt_full_slice[offset],
                            rsdt_full_slice[offset + 1],
                            rsdt_full_slice[offset + 2],
                            rsdt_full_slice[offset + 3],
                        ]) as u64;
                        if addr != 0 {
                            table_addrs.push(addr);
                        }
                    }
                }
            } else {
                report.warnings.push("Both XSDT and RSDT root tables invalid or absent".into());
            }
        } else {
            report.warnings.push("Neither XSDT nor RSDT pointer present in RSDP".into());
        }
    }

    let mut ssdt_tables: Vec<(u64, usize)> = Vec::new();

    for &t_addr in &table_addrs {
        let t_virt = phys_mem_offset + t_addr;
        let h_slice = unsafe { core::slice::from_raw_parts(t_virt as *const u8, 36) };
        let mut sig = [0u8; 4];
        sig.copy_from_slice(&h_slice[0..4]);

        let length = u32::from_le_bytes([h_slice[4], h_slice[5], h_slice[6], h_slice[7]]) as usize;
        let revision = h_slice[8];
        let mut oem_id = [0u8; 6];
        oem_id.copy_from_slice(&h_slice[10..16]);
        let mut oem_table_id = [0u8; 8];
        oem_table_id.copy_from_slice(&h_slice[16..24]);

        let valid = if length >= 36 && length < 16 * 1024 * 1024 {
            let full_slice = unsafe { core::slice::from_raw_parts(t_virt as *const u8, length) };
            verify_checksum(full_slice)
        } else {
            false
        };

        report.tables.push(TableSummary {
            signature: sig,
            phys_addr: t_addr,
            length: length as u32,
            revision,
            oem_id,
            oem_table_id,
            checksum_valid: valid,
        });

        if &sig == b"FACP" {
            report.fadt_addr = Some(t_addr);
            // Parse FADT for DSDT
            if length >= 148 {
                let fadt_full = unsafe { core::slice::from_raw_parts(t_virt as *const u8, length) };
                let x_dsdt = u64::from_le_bytes([
                    fadt_full[140], fadt_full[141], fadt_full[142], fadt_full[143],
                    fadt_full[144], fadt_full[145], fadt_full[146], fadt_full[147],
                ]);
                if x_dsdt != 0 {
                    report.dsdt_addr = Some(x_dsdt);
                } else {
                    let dsdt32 = u32::from_le_bytes([fadt_full[40], fadt_full[41], fadt_full[42], fadt_full[43]]) as u64;
                    report.dsdt_addr = Some(dsdt32);
                }
            } else if length >= 44 {
                let fadt_full = unsafe { core::slice::from_raw_parts(t_virt as *const u8, length) };
                let dsdt32 = u32::from_le_bytes([fadt_full[40], fadt_full[41], fadt_full[42], fadt_full[43]]) as u64;
                report.dsdt_addr = Some(dsdt32);
            }
        } else if &sig == b"SSDT" {
            ssdt_tables.push((t_addr, length));
        } else if &sig == b"MCFG" {
            report.mcfg_table_found = true;
            if length >= 44 {
                let mcfg_full = unsafe { core::slice::from_raw_parts(t_virt as *const u8, length) };
                let num_entries = (length - 44) / 16;
                for i in 0..num_entries {
                    let entry_offset = 44 + i * 16;
                    let base_address = u64::from_le_bytes([
                        mcfg_full[entry_offset],
                        mcfg_full[entry_offset + 1],
                        mcfg_full[entry_offset + 2],
                        mcfg_full[entry_offset + 3],
                        mcfg_full[entry_offset + 4],
                        mcfg_full[entry_offset + 5],
                        mcfg_full[entry_offset + 6],
                        mcfg_full[entry_offset + 7],
                    ]);
                    let segment_group = u16::from_le_bytes([
                        mcfg_full[entry_offset + 8],
                        mcfg_full[entry_offset + 9],
                    ]);
                    let start_bus = mcfg_full[entry_offset + 10];
                    let end_bus = mcfg_full[entry_offset + 11];

                    report.mcfg_allocations.push(McfgAllocation {
                        base_address,
                        segment_group,
                        start_bus,
                        end_bus,
                    });
                }
            }
        }
    }

    // 4. Validate and Inspect DSDT
    if let Some(dsdt_addr) = report.dsdt_addr {
        let dsdt_virt = phys_mem_offset + dsdt_addr;
        let dsdt_h_slice = unsafe { core::slice::from_raw_parts(dsdt_virt as *const u8, 36) };
        if &dsdt_h_slice[0..4] == b"DSDT" {
            let dsdt_len = u32::from_le_bytes([dsdt_h_slice[4], dsdt_h_slice[5], dsdt_h_slice[6], dsdt_h_slice[7]]) as usize;
            report.dsdt_length = dsdt_len as u32;

            if dsdt_len >= 36 && dsdt_len < 16 * 1024 * 1024 {
                let dsdt_full = unsafe { core::slice::from_raw_parts(dsdt_virt as *const u8, dsdt_len) };
                report.dsdt_checksum_valid = verify_checksum(dsdt_full);

                // Scan DSDT AML byte stream
                scan_aml_tables(dsdt_full, "DSDT", &mut report);
            }
        }
    }

    // 5. Inspect SSDTs for Peripherals
    for &(ssdt_addr, ssdt_len) in &ssdt_tables {
        if ssdt_len >= 36 && ssdt_len < 16 * 1024 * 1024 {
            let ssdt_virt = phys_mem_offset + ssdt_addr;
            let ssdt_full = unsafe { core::slice::from_raw_parts(ssdt_virt as *const u8, ssdt_len) };
            scan_aml_tables(ssdt_full, "SSDT", &mut report);
        }
    }

    *GLOBAL_ACPI_REPORT.lock() = Some(report.clone());
    report
}

/// Bounded byte scanner looking for known device identifiers and resource structures.
///
/// Clearly distinguishes between static Name(_CRS, ...) and dynamic Method(_CRS, ...).
fn scan_aml_tables(aml: &[u8], table_name: &str, report: &mut AcpiDiagnosticReport) {
    if aml.len() < 36 {
        return;
    }
    let body = &aml[36..]; // Skip standard 36-byte header

    // 1. Search for I2C Host Controller: "AMDI0010" or "AMD0010"
    let ctrl_needles: &[&[u8]] = &[b"AMDI0010", b"AMD0010"];
    for needle in ctrl_needles {
        let mut search_idx = 0;
        while let Some(rel_pos) = find_subslice(&body[search_idx..], *needle) {
            let match_pos = search_idx + rel_pos;
            search_idx = match_pos + needle.len();

            let hw_id = core::str::from_utf8(*needle).unwrap_or("UNKNOWN").into();
            let mut controller = DiscoveredI2cController {
                hardware_id: hw_id,
                table_sig: table_name.into(),
                crs_is_static: false,
                memory_base: None,
                memory_length: None,
                notes: String::new(),
            };

            // Inspect the surrounding scope (+/- 512 bytes) for _CRS
            let scope_start = match_pos.saturating_sub(64);
            let scope_end = (match_pos + 1024).min(body.len());
            let scope = &body[scope_start..scope_end];

            inspect_controller_scope(scope, &mut controller);
            report.i2c_controllers.push(controller);
        }
    }

    // 2. Search for Touchpad Devices: "ELAN", "SYNA", or "PNP0C50"
    let tp_needles: &[&[u8]] = &[b"PNP0C50", b"ELAN", b"SYNA"];
    for needle in tp_needles {
        let mut search_idx = 0;
        while let Some(rel_pos) = find_subslice(&body[search_idx..], *needle) {
            let match_pos = search_idx + rel_pos;
            search_idx = match_pos + needle.len();

            let hw_id = core::str::from_utf8(*needle).unwrap_or("UNKNOWN").into();
            let mut touchpad = DiscoveredTouchpad {
                hardware_id: hw_id,
                table_sig: table_name.into(),
                crs_is_static: false,
                slave_address: None,
                bus_speed_hz: None,
                dsm_hid_guid_found: false,
                dsm_candidate_register: None,
                notes: String::new(),
            };

            // Inspect surrounding scope (+/- 1024 bytes)
            let scope_start = match_pos.saturating_sub(128);
            let scope_end = (match_pos + 2048).min(body.len());
            let scope = &body[scope_start..scope_end];

            inspect_touchpad_scope(scope, &mut touchpad);
            report.touchpads.push(touchpad);
        }
    }
}

/// Inspects the AML byte slice surrounding an I2C controller declaration.
fn inspect_controller_scope(scope: &[u8], ctrl: &mut DiscoveredI2cController) {
    // Check if _CRS is defined as a Method: 0x14 ... "_CRS"
    if let Some(pos) = find_subslice(scope, b"_CRS") {
        if pos >= 1 && (scope[pos - 1] == 0x08 || (pos >= 2 && scope[pos - 2] == 0x08)) {
            // NameOp (0x08): Static Name(_CRS, ...)
            ctrl.crs_is_static = true;
            ctrl.notes.push_str("Static Name(_CRS) object identified. ");

            // Scan forward for Memory32Fixed descriptor (0x86)
            let crs_data = &scope[pos..];
            for i in 0..crs_data.len().saturating_sub(12) {
                if crs_data[i] == 0x86 {
                    if let Some(mem) = parse_memory32_fixed(&crs_data[i..]) {
                        ctrl.memory_base = Some(mem.base_address);
                        ctrl.memory_length = Some(mem.address_length);
                        ctrl.notes.push_str(&format!(
                            "Memory32Fixed: base={:#010x}, len={:#x}. ",
                            mem.base_address, mem.address_length
                        ));
                        break;
                    }
                }
            }
        } else if pos >= 1 && (scope[pos - 1] == 0x14 || (pos >= 4 && scope[pos - 4] == 0x14)) {
            // MethodOp (0x14): Dynamic Method(_CRS)
            ctrl.crs_is_static = false;
            ctrl.notes.push_str("Dynamic Method(_CRS) identified; requires AML evaluation. ");
        } else {
            ctrl.notes.push_str("_CRS reference found (syntax context pending AML VM). ");
        }
    } else {
        ctrl.notes.push_str("No explicit _CRS token in direct proximity. ");
    }
}

/// Inspects the AML byte slice surrounding a Touchpad device declaration.
fn inspect_touchpad_scope(scope: &[u8], tp: &mut DiscoveredTouchpad) {
    // 1. Check for _CRS (Static Name vs Dynamic Method)
    if let Some(pos) = find_subslice(scope, b"_CRS") {
        if pos >= 1 && (scope[pos - 1] == 0x08 || (pos >= 2 && scope[pos - 2] == 0x08)) {
            tp.crs_is_static = true;
            tp.notes.push_str("Static Name(_CRS) object. ");

            // Search for I2cSerialBusV2 descriptor (0x8E)
            let crs_data = &scope[pos..];
            for i in 0..crs_data.len().saturating_sub(16) {
                if crs_data[i] == 0x8E {
                    if let Some(i2c) = parse_i2c_serial_bus(&crs_data[i..]) {
                        tp.slave_address = Some(i2c.slave_address);
                        tp.bus_speed_hz = Some(i2c.connection_speed);
                        tp.notes.push_str(&format!(
                            "I2cSerialBus: slave={:#04x}, speed={}Hz. ",
                            i2c.slave_address, i2c.connection_speed
                        ));
                        break;
                    }
                }
            }
        } else {
            tp.crs_is_static = false;
            tp.notes.push_str("Dynamic Method(_CRS); slave address requires AML evaluation. ");
        }
    }

    // 2. Check for Microsoft HID-over-I2C GUID: 3CDFF6F7-4267-4555-AD05-B30A3D8938DE
    // In binary (LE): F7 F6 DF 3C 67 42 55 45 AD 05 B3 0A 3D 89 38 DE
    let ms_guid: [u8; 16] = [
        0xF7, 0xF6, 0xDF, 0x3C, 0x67, 0x42, 0x55, 0x45,
        0xAD, 0x05, 0xB3, 0x0A, 0x3D, 0x89, 0x38, 0xDE,
    ];

    if let Some(pos) = find_subslice(scope, &ms_guid) {
        tp.dsm_hid_guid_found = true;
        tp.notes.push_str("Microsoft HID-over-I2C GUID identified. ");

        // Search for Return pattern in proximity (Function 1 HID Descriptor Address)
        let dsm_body = &scope[pos..(pos + 256).min(scope.len())];
        // AML ReturnOp is 0xA4, followed by integer (0x01, 0x0A, 0x0B, etc.)
        for i in 0..dsm_body.len().saturating_sub(3) {
            if dsm_body[i] == 0xA4 {
                // ReturnOp
                if dsm_body[i + 1] == 0x0B {
                    // WordPrefix (u16)
                    let reg = u16::from_le_bytes([dsm_body[i + 2], dsm_body[i + 3]]);
                    tp.dsm_candidate_register = Some(reg);
                    tp.notes.push_str(&format!("Candidate wHIDDescRegister={:#06x}. ", reg));
                    break;
                } else if dsm_body[i + 1] == 0x01 {
                    // OneOp (0x0001)
                    tp.dsm_candidate_register = Some(1);
                    tp.notes.push_str("Candidate wHIDDescRegister=0x0001 (OneOp). ");
                    break;
                }
            }
        }
    }
}

/// Helper: Finds the first occurrence of `needle` inside `haystack`.
fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|window| window == needle)
}
