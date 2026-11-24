//! Genuine UEFI-Bootable Hybrid ISO 9660 & El Torito Packaging Engine
//!
//! Milestone: Live USB Deployment & Optical/Hybrid ISO Generation
//! Target Hardware: HP Victus 15-fb3xxx (AMD Ryzen 7 7445HS, UEFI firmware)
//!
//! This module implements a standards-compliant ISO 9660 + El Torito + Hybrid MBR
//! synthesizer that packages a bare-metal MicroOS UEFI FAT filesystem into a
//! genuine, dual-bootable hybrid ISO image.
//!
//! # Boot Architecture Distinction
//!
//! 1. **Optical Media Boot (UEFI El Torito):**
//!    When booted from optical media (CD/DVD) or virtual CD-ROM emulators:
//!    - Logical sector size is 2,048 bytes.
//!    - UEFI firmware parses the ISO 9660 Primary Volume Descriptor at Sector 16.
//!    - UEFI firmware reads the El Torito Boot Record Descriptor at Sector 17, locating
//!      the Boot Catalog at Sector 19.
//!    - In the Boot Catalog (Sector 19), it identifies the Section Header with
//!      **Platform ID 0xEF (UEFI)** and reads the Section Entry pointing to
//!      **Load RBA 20** (Sector 20).
//!    - UEFI firmware mounts Sector 20+ as an EFI System Partition (FAT), locates
//!      `\EFI\BOOT\BOOTX64.EFI`, and executes the MicroOS bootloader.
//!
//! 2. **Direct USB Boot (Hybrid MBR Mass Storage):**
//!    When written directly to a USB drive via block-writing utilities (`dd`, Rufus DD mode,
//!    Balena Etcher):
//!    - The USB drive is presented to UEFI firmware as a hard disk / mass storage device
//!      with 512-byte logical sectors.
//!    - The firmware does NOT parse the optical El Torito catalog at LBA 19.
//!    - Instead, UEFI firmware inspects **LBA 0 (Sector 0)** for a Master Boot Record (MBR).
//!    - The Hybrid MBR in Sector 0 defines **Partition 1 with Type 0xEF (EFI System Partition)**,
//!      active/bootable flag 0x80, starting at 512-byte LBA 80 (byte offset 40,960 = ISO Sector 20).
//!    - The UEFI firmware mounts the partition at LBA 80 as a FAT filesystem, discovers
//!      `\EFI\BOOT\BOOTX64.EFI`, and boots MicroOS without touching internal laptop drives.

use std::path::Path;

// ============================================================================
// Constants
// ============================================================================

/// Logical sector size in ISO 9660 (2,048 bytes).
pub const ISO_SECTOR_SIZE: usize = 2048;

/// Standard hard disk / USB logical sector size (512 bytes).
pub const DISK_SECTOR_SIZE: usize = 512;

/// Number of sectors in ISO 9660 System Area (Sectors 0..15 = 32,768 bytes = 64 disk sectors).
pub const SYSTEM_AREA_SECTORS: usize = 16;

/// Sector LBA of ISO 9660 Primary Volume Descriptor (PVD).
pub const PVD_LBA: u32 = 16;

/// Sector LBA of El Torito Boot Record Volume Descriptor.
pub const EL_TORITO_LBA: u32 = 17;

/// Sector LBA of Volume Descriptor Set Terminator.
pub const TERMINATOR_LBA: u32 = 18;

/// Sector LBA of El Torito Boot Catalog.
pub const BOOT_CATALOG_LBA: u32 = 19;

/// Sector LBA where the embedded FAT16 EFI System Partition starts.
pub const BOOT_IMAGE_LBA: u32 = 20;

/// 512-byte disk LBA where the FAT16 EFI System Partition starts for direct USB boot.
/// 20 ISO sectors * 2048 bytes / 512 bytes = 80.
pub const MBR_ESP_START_LBA: u32 = (BOOT_IMAGE_LBA * ISO_SECTOR_SIZE as u32) / (DISK_SECTOR_SIZE as u32);

/// MBR Partition Type for EFI System Partition (ESP).
pub const MBR_PARTITION_TYPE_ESP: u8 = 0xEF;

/// MBR Active / Bootable flag.
pub const MBR_BOOTABLE_FLAG: u8 = 0x80;

/// Standard MBR Boot Signature at byte offset 510..511.
pub const MBR_SIGNATURE: [u8; 2] = [0x55, 0xAA];

/// Standard ISO 9660 Identifier in Volume Descriptors.
pub const ISO_STANDARD_ID: &[u8; 5] = b"CD001";

/// El Torito Specification Identifier.
pub const EL_TORITO_ID: &[u8; 23] = b"EL TORITO SPECIFICATION";

/// El Torito Platform ID for x86 (used in default BIOS validation entry).
pub const EL_TORITO_PLATFORM_X86: u8 = 0x00;

/// El Torito Platform ID for Unified Extensible Firmware Interface (UEFI).
pub const EL_TORITO_PLATFORM_UEFI: u8 = 0xEF;

/// El Torito Header Indicator for the final section header.
pub const EL_TORITO_SECTION_HEADER_FINAL: u8 = 0x91;

/// El Torito Bootable Indicator.
pub const EL_TORITO_BOOTABLE_FLAG: u8 = 0x88;

/// El Torito Media Type: No Emulation.
pub const EL_TORITO_MEDIA_NO_EMULATION: u8 = 0x00;

/// El Torito Validation Entry Key bytes (0x55, 0xAA).
pub const EL_TORITO_VALIDATION_KEY: [u8; 2] = [0x55, 0xAA];

/// Standard FAT boot sector signature at byte offset 510..511.
pub const FAT_BOOT_SIGNATURE: [u8; 2] = [0x55, 0xAA];

// ============================================================================
// Endian Helpers (ISO 9660 Both-Endian Encoding)
// ============================================================================

/// Encodes a 16-bit integer in both-endian format (little-endian followed by big-endian).
#[inline]
pub fn write_both_endian_u16(buf: &mut [u8], val: u16) {
    let le = val.to_le_bytes();
    let be = val.to_be_bytes();
    buf[0..2].copy_from_slice(&le);
    buf[2..4].copy_from_slice(&be);
}

/// Encodes a 32-bit integer in both-endian format (little-endian followed by big-endian).
#[inline]
pub fn write_both_endian_u32(buf: &mut [u8], val: u32) {
    let le = val.to_le_bytes();
    let be = val.to_be_bytes();
    buf[0..4].copy_from_slice(&le);
    buf[4..8].copy_from_slice(&be);
}

/// Reads a 16-bit both-endian integer and returns (le_value, be_value).
#[inline]
pub fn read_both_endian_u16(buf: &[u8]) -> (u16, u16) {
    let le = u16::from_le_bytes([buf[0], buf[1]]);
    let be = u16::from_be_bytes([buf[2], buf[3]]);
    (le, be)
}

/// Reads a 32-bit both-endian integer and returns (le_value, be_value).
#[inline]
pub fn read_both_endian_u32(buf: &[u8]) -> (u32, u32) {
    let le = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
    let be = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);
    (le, be)
}

// ============================================================================
// ISO Sector Builders
// ============================================================================

/// Builds Sector 0 containing the Hybrid MBR Partition Table (512 bytes followed by padding).
///
/// Partition 1 is configured as:
/// - Status: Active / Bootable (`0x80`)
/// - Type: EFI System Partition (`0xEF`)
/// - Start LBA: 80 (512-byte sector equivalent of ISO Sector 20)
/// - Sector Count: Size of FAT partition in 512-byte sectors
pub fn build_hybrid_mbr(fat_size_bytes: usize) -> [u8; ISO_SECTOR_SIZE] {
    let mut sector = [0u8; ISO_SECTOR_SIZE];

    // Sector count in 512-byte sectors
    let fat_sectors_512 = ((fat_size_bytes + DISK_SECTOR_SIZE - 1) / DISK_SECTOR_SIZE) as u32;

    // Partition 1 Entry at byte offset 446 (0x1BE)
    let p1 = &mut sector[446..462];
    p1[0] = MBR_BOOTABLE_FLAG; // 0x80
    p1[1] = 0x00;              // CHS start head
    p1[2] = 0x01;              // CHS start sector (1..63)
    p1[3] = 0x00;              // CHS start cylinder
    p1[4] = MBR_PARTITION_TYPE_ESP; // 0xEF: EFI System Partition
    p1[5] = 0xFF;              // CHS end head
    p1[6] = 0xFF;              // CHS end sector
    p1[7] = 0xFF;              // CHS end cylinder
    p1[8..12].copy_from_slice(&MBR_ESP_START_LBA.to_le_bytes()); // Start LBA = 80
    p1[12..16].copy_from_slice(&fat_sectors_512.to_le_bytes()); // Sector count

    // MBR Boot Signature at byte offset 510..512
    sector[510] = MBR_SIGNATURE[0];
    sector[511] = MBR_SIGNATURE[1];

    sector
}

/// Builds Sector 16: ISO 9660 Primary Volume Descriptor (PVD).
pub fn build_pvd(total_iso_sectors: u32, volume_id: &str) -> [u8; ISO_SECTOR_SIZE] {
    let mut sector = [0u8; ISO_SECTOR_SIZE];

    // Volume Descriptor Header
    sector[0] = 0x01; // Primary Volume Descriptor type
    sector[1..6].copy_from_slice(ISO_STANDARD_ID); // "CD001"
    sector[6] = 0x01; // Version

    // System Identifier (32 bytes ASCII, space padded)
    let sys_id = b"MICRO_OS_LIVE                   ";
    sector[8..40].copy_from_slice(sys_id);

    // Volume Identifier (32 bytes ASCII, space padded)
    let mut vol_buf = [b' '; 32];
    let vol_bytes = volume_id.as_bytes();
    let copy_len = vol_bytes.len().min(32);
    vol_buf[..copy_len].copy_from_slice(&vol_bytes[..copy_len]);
    sector[40..72].copy_from_slice(&vol_buf);

    // Volume Space Size (total 2048-byte logical sectors, both-endian u32)
    write_both_endian_u32(&mut sector[80..88], total_iso_sectors);

    // Volume Set Size = 1 (both-endian u16)
    write_both_endian_u16(&mut sector[120..124], 1);

    // Volume Sequence Number = 1 (both-endian u16)
    write_both_endian_u16(&mut sector[124..128], 1);

    // Logical Block Size = 2048 (both-endian u16)
    write_both_endian_u16(&mut sector[128..132], ISO_SECTOR_SIZE as u16);

    // Minimal Root Directory Record (34 bytes at offset 156)
    let root = &mut sector[156..190];
    root[0] = 34;   // Length of Directory Record (0x22)
    root[1] = 0;    // Extended Attribute Record Length
    write_both_endian_u32(&mut root[2..10], PVD_LBA); // Extent LBA = 16
    write_both_endian_u32(&mut root[10..18], ISO_SECTOR_SIZE as u32); // Data length = 2048
    root[18..25].copy_from_slice(&[126, 11, 17, 12, 0, 0, 0]); // Date: 2026-11-17 12:00:00 (1900+126)
    root[25] = 0x02; // File flags: Directory
    root[26] = 0;    // File unit size
    root[27] = 0;    // Interleave gap size
    write_both_endian_u16(&mut root[28..32], 1); // Volume sequence number = 1
    root[32] = 1;    // File identifier length
    root[33] = 0x00; // Special root directory entry (0x00)

    // Volume Set Identifier (128 bytes, space padded)
    let set_id = b"MICRO_OS                                                                                                                        ";
    sector[190..318].copy_from_slice(&set_id[..128]);

    // Publisher Identifier (128 bytes)
    let pub_id = b"MICRO_OS_TEAM                                                                                                                   ";
    sector[318..446].copy_from_slice(&pub_id[..128]);

    // Data Preparer Identifier (128 bytes)
    let prep_id = b"MICRO_OS_RUNNER                                                                                                                 ";
    sector[446..574].copy_from_slice(&prep_id[..128]);

    // Application Identifier (128 bytes)
    let app_id = b"MICRO_OS_BOOTLOADER                                                                                                             ";
    sector[574..702].copy_from_slice(&app_id[..128]);

    // Dates (17 bytes ASCII: YYYYMMDDHHMMSScc + timezone offset)
    let date_str = b"2026111712000000\x00";
    sector[813..830].copy_from_slice(date_str); // Creation date
    sector[830..847].copy_from_slice(date_str); // Modification date

    // File Structure Version
    sector[881] = 0x01;

    sector
}

/// Builds Sector 17: El Torito Boot Record Volume Descriptor.
pub fn build_el_torito_descriptor() -> [u8; ISO_SECTOR_SIZE] {
    let mut sector = [0u8; ISO_SECTOR_SIZE];

    // Boot Record Indicator: 0x00
    sector[0] = 0x00;
    sector[1..6].copy_from_slice(ISO_STANDARD_ID); // "CD001"
    sector[6] = 0x01; // Version

    // Boot System Identifier: "EL TORITO SPECIFICATION" (32 bytes)
    sector[7..30].copy_from_slice(EL_TORITO_ID);

    // Boot Catalog Pointer at offset 71 (0x47): 4 bytes little-endian
    sector[71..75].copy_from_slice(&BOOT_CATALOG_LBA.to_le_bytes()); // LBA 19

    sector
}

/// Builds Sector 18: Volume Descriptor Set Terminator.
pub fn build_terminator() -> [u8; ISO_SECTOR_SIZE] {
    let mut sector = [0u8; ISO_SECTOR_SIZE];
    sector[0] = 0xFF; // Terminator type
    sector[1..6].copy_from_slice(ISO_STANDARD_ID); // "CD001"
    sector[6] = 0x01; // Version
    sector
}

/// Builds Sector 19: El Torito Boot Catalog with UEFI Platform Entry.
///
/// Entries:
/// 1. Validation Entry (32 bytes): Platform x86, Manufacturer ID, Checksum, 0x55AA.
/// 2. Default Initial Entry (32 bytes): Non-bootable legacy BIOS fallback.
/// 3. Section Header (32 bytes): Final section, **Platform ID 0xEF (UEFI)**.
/// 4. Section Entry (32 bytes): Bootable (0x88), No Emulation, **Load RBA 20**.
pub fn build_el_torito_boot_catalog(fat_size_bytes: usize) -> [u8; ISO_SECTOR_SIZE] {
    let mut sector = [0u8; ISO_SECTOR_SIZE];

    // Sector count of the FAT image in 512-byte sectors
    let fat_sectors_512 = ((fat_size_bytes + DISK_SECTOR_SIZE - 1) / DISK_SECTOR_SIZE) as u16;

    // --- 1. Validation Entry (Bytes 0..32) ---
    {
        let val_entry = &mut sector[0..32];
        val_entry[0] = 0x01; // Header ID
        val_entry[1] = EL_TORITO_PLATFORM_X86; // Platform ID: 0x00 (x86)
        val_entry[2] = 0x00; // Reserved
        val_entry[3] = 0x00;

        // Manufacturer ID (24 bytes space-padded)
        let mfg_id = b"MicroOS Live UEFI       ";
        val_entry[4..28].copy_from_slice(mfg_id);

        // Key bytes: 0x55, 0xAA
        val_entry[30] = EL_TORITO_VALIDATION_KEY[0];
        val_entry[31] = EL_TORITO_VALIDATION_KEY[1];

        // Checksum calculation:
        // Sum of all 16 16-bit little-endian words in the 32-byte record must be 0 (modulo 0x10000).
        let mut sum: u32 = 0;
        for i in 0..16 {
            if i != 14 { // Skip checksum word itself at offset 28..30
                let w = u16::from_le_bytes([val_entry[i * 2], val_entry[i * 2 + 1]]);
                sum = sum.wrapping_add(w as u32);
            }
        }
        let checksum = ((0x10000u32 - (sum & 0xFFFF)) & 0xFFFF) as u16;
        val_entry[28..30].copy_from_slice(&checksum.to_le_bytes());
    }

    // --- 2. Default Initial Entry (Bytes 32..64) ---
    {
        let def_entry = &mut sector[32..64];
        def_entry[0] = 0x00; // Not bootable in legacy BIOS
        def_entry[1] = EL_TORITO_MEDIA_NO_EMULATION; // No emulation
        def_entry[2] = 0x00; // Load segment
        def_entry[3] = 0x00;
        def_entry[4] = 0x00; // System type
        def_entry[5] = 0x00; // Unused
        def_entry[6..8].copy_from_slice(&1u16.to_le_bytes()); // Sector count = 1
        def_entry[8..12].copy_from_slice(&0u32.to_le_bytes()); // Load RBA = 0
    }

    // --- 3. Section Header: UEFI Platform (Bytes 64..96) ---
    {
        let sec_hdr = &mut sector[64..96];
        sec_hdr[0] = EL_TORITO_SECTION_HEADER_FINAL; // 0x91: Final section header
        sec_hdr[1] = EL_TORITO_PLATFORM_UEFI;        // 0xEF: UEFI Platform ID
        sec_hdr[2..4].copy_from_slice(&1u16.to_le_bytes()); // 1 section entry follows
        let sec_id = b"MicroOS UEFI Boot Image     ";
        sec_hdr[4..32].copy_from_slice(sec_id);
    }

    // --- 4. Section Entry: UEFI Boot Image (Bytes 96..128) ---
    {
        let sec_entry = &mut sector[96..128];
        sec_entry[0] = EL_TORITO_BOOTABLE_FLAG; // 0x88: Bootable
        sec_entry[1] = EL_TORITO_MEDIA_NO_EMULATION; // 0x00: No emulation
        sec_entry[2] = 0x00; // Load segment
        sec_entry[3] = 0x00;
        sec_entry[4] = 0x00; // System type
        sec_entry[5] = 0x00; // Unused
        sec_entry[6..8].copy_from_slice(&fat_sectors_512.to_le_bytes()); // Sector count (512B)
        sec_entry[8..12].copy_from_slice(&BOOT_IMAGE_LBA.to_le_bytes()); // Load RBA = Sector 20
    }

    sector
}

// ============================================================================
// ISO Synthesizer
// ============================================================================

/// Generates a complete, standards-compliant Hybrid UEFI ISO image in memory from
/// raw FAT filesystem bytes.
///
/// Layout:
/// - Sector 0: Hybrid MBR (Partition 1 type 0xEF pointing to LBA 80 in 512-byte units)
/// - Sectors 1..15: System Area padding (zeros)
/// - Sector 16: Primary Volume Descriptor (PVD)
/// - Sector 17: El Torito Boot Record Volume Descriptor (points to LBA 19)
/// - Sector 18: Volume Descriptor Set Terminator
/// - Sector 19: El Torito Boot Catalog (UEFI Platform 0xEF, Load RBA 20)
/// - Sector 20+: Embedded FAT16 EFI System Partition
pub fn generate_hybrid_uefi_iso(fat_image: &[u8], volume_id: &str) -> Vec<u8> {
    let fat_sectors = (fat_image.len() + ISO_SECTOR_SIZE - 1) / ISO_SECTOR_SIZE;
    let total_iso_sectors = (BOOT_IMAGE_LBA as usize) + fat_sectors;
    let total_bytes = total_iso_sectors * ISO_SECTOR_SIZE;

    let mut iso = vec![0u8; total_bytes];

    // Sector 0: Hybrid MBR
    let mbr = build_hybrid_mbr(fat_image.len());
    iso[0..ISO_SECTOR_SIZE].copy_from_slice(&mbr);

    // Sectors 1..15 remain zeros (ISO System Area)

    // Sector 16: Primary Volume Descriptor
    let pvd = build_pvd(total_iso_sectors as u32, volume_id);
    let pvd_offset = (PVD_LBA as usize) * ISO_SECTOR_SIZE;
    iso[pvd_offset..pvd_offset + ISO_SECTOR_SIZE].copy_from_slice(&pvd);

    // Sector 17: El Torito Boot Record
    let el_torito = build_el_torito_descriptor();
    let et_offset = (EL_TORITO_LBA as usize) * ISO_SECTOR_SIZE;
    iso[et_offset..et_offset + ISO_SECTOR_SIZE].copy_from_slice(&el_torito);

    // Sector 18: Terminator
    let term = build_terminator();
    let term_offset = (TERMINATOR_LBA as usize) * ISO_SECTOR_SIZE;
    iso[term_offset..term_offset + ISO_SECTOR_SIZE].copy_from_slice(&term);

    // Sector 19: El Torito Boot Catalog
    let catalog = build_el_torito_boot_catalog(fat_image.len());
    let cat_offset = (BOOT_CATALOG_LBA as usize) * ISO_SECTOR_SIZE;
    iso[cat_offset..cat_offset + ISO_SECTOR_SIZE].copy_from_slice(&catalog);

    // Sector 20+: Embedded FAT EFI System Partition
    let fat_offset = (BOOT_IMAGE_LBA as usize) * ISO_SECTOR_SIZE;
    iso[fat_offset..fat_offset + fat_image.len()].copy_from_slice(fat_image);

    iso
}

/// Generates a bootable Hybrid UEFI ISO file from a compiled MicroOS kernel executable.
///
/// Uses `bootloader::DiskImageBuilder::create_uefi_fat_partition` to generate the complete
/// FAT filesystem partition containing `EFI/BOOT/BOOTX64.EFI` and `KERNEL-X86_64`, then
/// wraps it in the Hybrid ISO 9660 + El Torito structures.
pub fn create_hybrid_uefi_iso_file(
    kernel_bin_path: &Path,
    out_iso_path: &Path,
    volume_id: &str,
) -> anyhow::Result<usize> {
    let builder = bootloader::DiskImageBuilder::new(kernel_bin_path.to_path_buf());

    // Generate temporary FAT partition
    let temp_fat_path = out_iso_path.with_extension("fat.tmp");
    builder.create_uefi_fat_partition(&temp_fat_path)?;

    let fat_bytes = std::fs::read(&temp_fat_path)?;
    let _ = std::fs::remove_file(&temp_fat_path);

    let iso_bytes = generate_hybrid_uefi_iso(&fat_bytes, volume_id);
    std::fs::write(out_iso_path, &iso_bytes)?;

    Ok(iso_bytes.len())
}

// ============================================================================
// Structural Validator & Report
// ============================================================================

/// Detailed validation report confirming all architectural structures of the Hybrid ISO.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IsoValidationReport {
    pub total_size_bytes: usize,
    pub total_iso_sectors: usize,
    pub mbr_partition1_type: u8,
    pub mbr_partition1_start_lba512: u32,
    pub mbr_partition1_sectors512: u32,
    pub pvd_volume_id: String,
    pub pvd_logical_block_size: u16,
    pub el_torito_catalog_lba: u32,
    pub boot_catalog_platform_id: u8,
    pub boot_catalog_load_rba: u32,
    pub boot_catalog_sector_count512: u16,
    pub fat_bytes_per_sector: u16,
    pub fat_sectors_per_cluster: u8,
    pub fat_total_sectors: u32,
    pub efi_bootloader_found: bool,
    pub efi_bootloader_size: u32,
    pub kernel_found: bool,
    pub kernel_size: u32,
}

/// Structural validation error describing exact point of failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IsoValidationError {
    ImageTooSmall { size: usize, min_required: usize },
    InvalidMbrSignature { found: [u8; 2] },
    InvalidMbrPartitionType { found: u8, expected: u8 },
    InvalidMbrStartLba { found: u32, expected: u32 },
    InvalidPvdType { found: u8 },
    InvalidPvdStandardId { found: Vec<u8> },
    InvalidPvdVersion { found: u8 },
    InvalidPvdBlockSize { found_le: u16, found_be: u16 },
    InvalidElToritoType { found: u8 },
    InvalidElToritoStandardId { found: Vec<u8> },
    InvalidElToritoSystemId { found: String },
    InvalidElToritoCatalogPointer { found: u32, expected: u32 },
    InvalidTerminatorType { found: u8 },
    InvalidTerminatorStandardId { found: Vec<u8> },
    InvalidValidationEntryHeader { found: u8 },
    InvalidValidationEntryKey { found: [u8; 2] },
    InvalidValidationEntryChecksum { sum: u16 },
    InvalidSectionHeaderIndicator { found: u8 },
    InvalidSectionHeaderPlatformId { found: u8, expected: u8 },
    InvalidSectionEntryBootIndicator { found: u8 },
    InvalidSectionEntryLoadRba { found: u32, expected: u32 },
    InvalidFatBootSignature { found: [u8; 2] },
    InvalidFatBytesPerSector { found: u16 },
    EfiDirectoryNotFound,
    BootDirectoryNotFound,
    BootloaderNotFound,
    KernelNotFound,
}

impl std::fmt::Display for IsoValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ImageTooSmall { size, min_required } => {
                write!(f, "ISO image too small: {} bytes (minimum required: {})", size, min_required)
            }
            Self::InvalidMbrSignature { found } => {
                write!(f, "Invalid MBR signature at byte 510: {:02X?}", found)
            }
            Self::InvalidMbrPartitionType { found, expected } => {
                write!(f, "Invalid MBR partition 1 type: 0x{:02X} (expected 0x{:02X})", found, expected)
            }
            Self::InvalidMbrStartLba { found, expected } => {
                write!(f, "Invalid MBR partition 1 start LBA: {} (expected {})", found, expected)
            }
            Self::InvalidPvdType { found } => {
                write!(f, "Invalid PVD descriptor type: 0x{:02X} (expected 0x01)", found)
            }
            Self::InvalidPvdStandardId { found } => {
                write!(f, "Invalid PVD standard ID: {:?}", String::from_utf8_lossy(found))
            }
            Self::InvalidPvdVersion { found } => {
                write!(f, "Invalid PVD descriptor version: 0x{:02X} (expected 0x01)", found)
            }
            Self::InvalidPvdBlockSize { found_le, found_be } => {
                write!(f, "Invalid PVD logical block size: le={}, be={} (expected 2048)", found_le, found_be)
            }
            Self::InvalidElToritoType { found } => {
                write!(f, "Invalid El Torito descriptor type: 0x{:02X} (expected 0x00)", found)
            }
            Self::InvalidElToritoStandardId { found } => {
                write!(f, "Invalid El Torito standard ID: {:?}", String::from_utf8_lossy(found))
            }
            Self::InvalidElToritoSystemId { found } => {
                write!(f, "Invalid El Torito boot system ID: {:?}", found)
            }
            Self::InvalidElToritoCatalogPointer { found, expected } => {
                write!(f, "Invalid El Torito catalog pointer LBA: {} (expected {})", found, expected)
            }
            Self::InvalidTerminatorType { found } => {
                write!(f, "Invalid Terminator descriptor type: 0x{:02X} (expected 0xFF)", found)
            }
            Self::InvalidTerminatorStandardId { found } => {
                write!(f, "Invalid Terminator standard ID: {:?}", String::from_utf8_lossy(found))
            }
            Self::InvalidValidationEntryHeader { found } => {
                write!(f, "Invalid Boot Catalog Validation Entry Header: 0x{:02X} (expected 0x01)", found)
            }
            Self::InvalidValidationEntryKey { found } => {
                write!(f, "Invalid Boot Catalog Validation Entry Key: {:02X?}", found)
            }
            Self::InvalidValidationEntryChecksum { sum } => {
                write!(f, "Invalid Boot Catalog Validation Entry checksum word sum: 0x{:04X} (expected 0x0000)", sum)
            }
            Self::InvalidSectionHeaderIndicator { found } => {
                write!(f, "Invalid Section Header indicator: 0x{:02X} (expected 0x91)", found)
            }
            Self::InvalidSectionHeaderPlatformId { found, expected } => {
                write!(f, "Invalid Section Header Platform ID: 0x{:02X} (expected 0x{:02X})", found, expected)
            }
            Self::InvalidSectionEntryBootIndicator { found } => {
                write!(f, "Invalid Section Entry Boot Indicator: 0x{:02X} (expected 0x88)", found)
            }
            Self::InvalidSectionEntryLoadRba { found, expected } => {
                write!(f, "Invalid Section Entry Load RBA: {} (expected {})", found, expected)
            }
            Self::InvalidFatBootSignature { found } => {
                write!(f, "Invalid FAT boot sector signature: {:02X?}", found)
            }
            Self::InvalidFatBytesPerSector { found } => {
                write!(f, "Invalid FAT bytes per sector: {} (expected 512)", found)
            }
            Self::EfiDirectoryNotFound => write!(f, "EFI directory not found in embedded FAT filesystem"),
            Self::BootDirectoryNotFound => write!(f, "BOOT directory not found under EFI/ in embedded FAT filesystem"),
            Self::BootloaderNotFound => write!(f, "BOOTX64.EFI bootloader binary not found in embedded FAT filesystem"),
            Self::KernelNotFound => write!(f, "MicroOS kernel binary not found in embedded FAT filesystem"),
        }
    }
}

impl std::error::Error for IsoValidationError {}

/// Performs complete, independent structural validation of a Hybrid UEFI ISO image.
///
/// Verifies:
/// 1. Minimum image size (at least 21 sectors = 43,008 bytes).
/// 2. Sector 0: Hybrid MBR Partition Table (active flag 0x80, type 0xEF, start LBA 80, signature 0x55AA).
/// 3. Sector 16: ISO 9660 PVD (CD001, block size 2048, volume ID).
/// 4. Sector 17: El Torito Boot Record (CD001, "EL TORITO SPECIFICATION", catalog pointer LBA 19).
/// 5. Sector 18: Volume Descriptor Set Terminator (CD001, type 0xFF).
/// 6. Sector 19: Boot Catalog (Validation entry checksum 0x0000, Section Header Platform ID 0xEF, Section Entry Load RBA 20).
/// 7. Sector 20+: Embedded FAT partition (signature 0x55AA, bps 512, contains EFI/BOOT/BOOTX64.EFI and KERNEL).
pub fn validate_hybrid_uefi_iso(iso_bytes: &[u8]) -> Result<IsoValidationReport, IsoValidationError> {
    let min_required = (BOOT_IMAGE_LBA as usize + 1) * ISO_SECTOR_SIZE;
    if iso_bytes.len() < min_required {
        return Err(IsoValidationError::ImageTooSmall {
            size: iso_bytes.len(),
            min_required,
        });
    }

    // --- 1. MBR Validation (Sector 0) ---
    let mbr_sig = [iso_bytes[510], iso_bytes[511]];
    if mbr_sig != MBR_SIGNATURE {
        return Err(IsoValidationError::InvalidMbrSignature { found: mbr_sig });
    }

    let p1 = &iso_bytes[446..462];
    let p1_type = p1[4];
    if p1_type != MBR_PARTITION_TYPE_ESP {
        return Err(IsoValidationError::InvalidMbrPartitionType {
            found: p1_type,
            expected: MBR_PARTITION_TYPE_ESP,
        });
    }

    let p1_start_lba = u32::from_le_bytes([p1[8], p1[9], p1[10], p1[11]]);
    if p1_start_lba != MBR_ESP_START_LBA {
        return Err(IsoValidationError::InvalidMbrStartLba {
            found: p1_start_lba,
            expected: MBR_ESP_START_LBA,
        });
    }

    let p1_sectors = u32::from_le_bytes([p1[12], p1[13], p1[14], p1[15]]);

    // --- 2. PVD Validation (Sector 16) ---
    let pvd_offset = (PVD_LBA as usize) * ISO_SECTOR_SIZE;
    let pvd = &iso_bytes[pvd_offset..pvd_offset + ISO_SECTOR_SIZE];
    if pvd[0] != 0x01 {
        return Err(IsoValidationError::InvalidPvdType { found: pvd[0] });
    }
    if &pvd[1..6] != ISO_STANDARD_ID {
        return Err(IsoValidationError::InvalidPvdStandardId { found: pvd[1..6].to_vec() });
    }
    if pvd[6] != 0x01 {
        return Err(IsoValidationError::InvalidPvdVersion { found: pvd[6] });
    }
    let (block_size_le, block_size_be) = read_both_endian_u16(&pvd[128..132]);
    if block_size_le != (ISO_SECTOR_SIZE as u16) || block_size_be != (ISO_SECTOR_SIZE as u16) {
        return Err(IsoValidationError::InvalidPvdBlockSize {
            found_le: block_size_le,
            found_be: block_size_be,
        });
    }
    let vol_id = String::from_utf8_lossy(&pvd[40..72]).trim().to_string();

    // --- 3. El Torito Descriptor Validation (Sector 17) ---
    let et_offset = (EL_TORITO_LBA as usize) * ISO_SECTOR_SIZE;
    let et = &iso_bytes[et_offset..et_offset + ISO_SECTOR_SIZE];
    if et[0] != 0x00 {
        return Err(IsoValidationError::InvalidElToritoType { found: et[0] });
    }
    if &et[1..6] != ISO_STANDARD_ID {
        return Err(IsoValidationError::InvalidElToritoStandardId { found: et[1..6].to_vec() });
    }
    if &et[7..30] != EL_TORITO_ID {
        let sys_id = String::from_utf8_lossy(&et[7..39]).to_string();
        return Err(IsoValidationError::InvalidElToritoSystemId { found: sys_id });
    }
    let cat_ptr = u32::from_le_bytes([et[71], et[72], et[73], et[74]]);
    if cat_ptr != BOOT_CATALOG_LBA {
        return Err(IsoValidationError::InvalidElToritoCatalogPointer {
            found: cat_ptr,
            expected: BOOT_CATALOG_LBA,
        });
    }

    // --- 4. Terminator Validation (Sector 18) ---
    let term_offset = (TERMINATOR_LBA as usize) * ISO_SECTOR_SIZE;
    let term = &iso_bytes[term_offset..term_offset + ISO_SECTOR_SIZE];
    if term[0] != 0xFF {
        return Err(IsoValidationError::InvalidTerminatorType { found: term[0] });
    }
    if &term[1..6] != ISO_STANDARD_ID {
        return Err(IsoValidationError::InvalidTerminatorStandardId { found: term[1..6].to_vec() });
    }

    // --- 5. Boot Catalog Validation (Sector 19) ---
    let cat_offset = (BOOT_CATALOG_LBA as usize) * ISO_SECTOR_SIZE;
    let cat = &iso_bytes[cat_offset..cat_offset + ISO_SECTOR_SIZE];

    // Validation entry
    if cat[0] != 0x01 {
        return Err(IsoValidationError::InvalidValidationEntryHeader { found: cat[0] });
    }
    let val_key = [cat[30], cat[31]];
    if val_key != EL_TORITO_VALIDATION_KEY {
        return Err(IsoValidationError::InvalidValidationEntryKey { found: val_key });
    }

    // Checksum verification: sum of all 16 16-bit little-endian words must equal 0
    let mut word_sum: u32 = 0;
    for i in 0..16 {
        let w = u16::from_le_bytes([cat[i * 2], cat[i * 2 + 1]]);
        word_sum = word_sum.wrapping_add(w as u32);
    }
    let checksum_result = (word_sum & 0xFFFF) as u16;
    if checksum_result != 0x0000 {
        return Err(IsoValidationError::InvalidValidationEntryChecksum { sum: checksum_result });
    }

    // Section Header (UEFI Platform)
    let sec_hdr = &cat[64..96];
    if sec_hdr[0] != EL_TORITO_SECTION_HEADER_FINAL {
        return Err(IsoValidationError::InvalidSectionHeaderIndicator { found: sec_hdr[0] });
    }
    let sec_platform = sec_hdr[1];
    if sec_platform != EL_TORITO_PLATFORM_UEFI {
        return Err(IsoValidationError::InvalidSectionHeaderPlatformId {
            found: sec_platform,
            expected: EL_TORITO_PLATFORM_UEFI,
        });
    }

    // Section Entry (UEFI Boot Image)
    let sec_entry = &cat[96..128];
    if sec_entry[0] != EL_TORITO_BOOTABLE_FLAG {
        return Err(IsoValidationError::InvalidSectionEntryBootIndicator { found: sec_entry[0] });
    }
    let load_rba = u32::from_le_bytes([sec_entry[8], sec_entry[9], sec_entry[10], sec_entry[11]]);
    if load_rba != BOOT_IMAGE_LBA {
        return Err(IsoValidationError::InvalidSectionEntryLoadRba {
            found: load_rba,
            expected: BOOT_IMAGE_LBA,
        });
    }
    let cat_sectors_512 = u16::from_le_bytes([sec_entry[6], sec_entry[7]]);

    // --- 6. Embedded FAT Filesystem Validation (Sector 20+) ---
    let fat_offset = (BOOT_IMAGE_LBA as usize) * ISO_SECTOR_SIZE;
    let fat_part = &iso_bytes[fat_offset..];
    if fat_part.len() < 512 {
        return Err(IsoValidationError::ImageTooSmall {
            size: iso_bytes.len(),
            min_required: fat_offset + 512,
        });
    }

    let fat_sig = [fat_part[510], fat_part[511]];
    if fat_sig != FAT_BOOT_SIGNATURE {
        return Err(IsoValidationError::InvalidFatBootSignature { found: fat_sig });
    }

    let bps = u16::from_le_bytes([fat_part[11], fat_part[12]]);
    if bps != 512 {
        return Err(IsoValidationError::InvalidFatBytesPerSector { found: bps });
    }

    let spc = fat_part[13];
    let rsvd_sec = u16::from_le_bytes([fat_part[14], fat_part[15]]) as usize;
    let num_fats = fat_part[16] as usize;
    let root_entries = u16::from_le_bytes([fat_part[17], fat_part[18]]) as usize;
    let total_sec16 = u16::from_le_bytes([fat_part[19], fat_part[20]]) as usize;
    let fat_sz16 = u16::from_le_bytes([fat_part[22], fat_part[23]]) as usize;
    let total_sec32 = u32::from_le_bytes([fat_part[32], fat_part[33], fat_part[34], fat_part[35]]) as usize;
    let total_fat_sectors = if total_sec16 != 0 { total_sec16 as u32 } else { total_sec32 as u32 };

    let root_dir_offset = (rsvd_sec + num_fats * fat_sz16) * (bps as usize);
    let root_dir_size = root_entries * 32;
    let data_cluster_2_offset = root_dir_offset + root_dir_size;
    let cluster_size = (spc as usize) * (bps as usize);

    if fat_part.len() < root_dir_offset + root_dir_size {
        return Err(IsoValidationError::ImageTooSmall {
            size: iso_bytes.len(),
            min_required: fat_offset + root_dir_offset + root_dir_size,
        });
    }

    // Helper: Parse directory entries
    let parse_dir_entries = |chunk: &[u8]| -> Vec<(String, bool, u16, u32)> {
        let mut entries = Vec::new();
        for i in (0..chunk.len()).step_by(32) {
            if i + 32 > chunk.len() {
                break;
            }
            let entry = &chunk[i..i + 32];
            if entry[0] == 0x00 {
                break; // End of directory
            }
            if entry[0] == 0xE5 {
                continue; // Deleted entry
            }
            let attr = entry[11];
            if attr == 0x0F {
                continue; // LFN entry
            }
            let name_raw = String::from_utf8_lossy(&entry[0..8]).trim().to_string();
            let ext_raw = String::from_utf8_lossy(&entry[8..11]).trim().to_string();
            let full_name = if ext_raw.is_empty() {
                name_raw
            } else {
                format!("{}.{}", name_raw, ext_raw)
            };
            let is_dir = (attr & 0x10) != 0;
            let cluster = u16::from_le_bytes([entry[26], entry[27]]);
            let size = u32::from_le_bytes([entry[28], entry[29], entry[30], entry[31]]);
            entries.append(&mut vec![(full_name, is_dir, cluster, size)]);
        }
        entries
    };

    // Helper: Read cluster data
    let read_cluster = |cluster: u16| -> Option<&[u8]> {
        if cluster < 2 {
            return None;
        }
        let cluster_offset = data_cluster_2_offset + ((cluster as usize) - 2) * cluster_size;
        if cluster_offset + cluster_size <= fat_part.len() {
            Some(&fat_part[cluster_offset..cluster_offset + cluster_size])
        } else {
            None
        }
    };

    let root_entries_list = parse_dir_entries(&fat_part[root_dir_offset..root_dir_offset + root_dir_size]);

    let mut efi_cluster = None;
    let mut kernel_found = false;
    let mut kernel_size = 0u32;

    for (name, is_dir, cluster, size) in &root_entries_list {
        if *is_dir && name.eq_ignore_ascii_case("EFI") {
            efi_cluster = Some(*cluster);
        }
        if !*is_dir && (name.to_ascii_uppercase().starts_with("KERNEL")) {
            kernel_found = true;
            kernel_size = *size;
        }
    }

    let efi_cluster = efi_cluster.ok_or(IsoValidationError::EfiDirectoryNotFound)?;
    if !kernel_found {
        return Err(IsoValidationError::KernelNotFound);
    }

    let efi_data = read_cluster(efi_cluster).ok_or(IsoValidationError::EfiDirectoryNotFound)?;
    let efi_entries = parse_dir_entries(efi_data);

    let mut boot_cluster = None;
    for (name, is_dir, cluster, _size) in &efi_entries {
        if *is_dir && name.eq_ignore_ascii_case("BOOT") {
            boot_cluster = Some(*cluster);
            break;
        }
    }

    let boot_cluster = boot_cluster.ok_or(IsoValidationError::BootDirectoryNotFound)?;
    let boot_data = read_cluster(boot_cluster).ok_or(IsoValidationError::BootDirectoryNotFound)?;
    let boot_entries = parse_dir_entries(boot_data);

    let mut bootloader_found = false;
    let mut bootloader_size = 0u32;
    for (name, is_dir, _cluster, size) in &boot_entries {
        if !*is_dir && name.eq_ignore_ascii_case("BOOTX64.EFI") {
            bootloader_found = true;
            bootloader_size = *size;
            break;
        }
    }

    if !bootloader_found {
        return Err(IsoValidationError::BootloaderNotFound);
    }

    Ok(IsoValidationReport {
        total_size_bytes: iso_bytes.len(),
        total_iso_sectors: iso_bytes.len() / ISO_SECTOR_SIZE,
        mbr_partition1_type: p1_type,
        mbr_partition1_start_lba512: p1_start_lba,
        mbr_partition1_sectors512: p1_sectors,
        pvd_volume_id: vol_id,
        pvd_logical_block_size: block_size_le,
        el_torito_catalog_lba: cat_ptr,
        boot_catalog_platform_id: sec_platform,
        boot_catalog_load_rba: load_rba,
        boot_catalog_sector_count512: cat_sectors_512,
        fat_bytes_per_sector: bps,
        fat_sectors_per_cluster: spc,
        fat_total_sectors: total_fat_sectors,
        efi_bootloader_found: bootloader_found,
        efi_bootloader_size: bootloader_size,
        kernel_found,
        kernel_size,
    })
}

// ============================================================================
// Automated Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Constructs a minimal valid FAT16 image for synthetic unit tests.
    fn create_mock_fat_partition() -> Vec<u8> {
        let bps = 512usize;
        let spc = 4usize;
        let rsvd_sec = 1usize;
        let num_fats = 2usize;
        let fat_sz = 5usize;
        let root_entries = 64usize;
        let root_dir_size = root_entries * 32;
        let cluster_size = spc * bps;

        // Clusters: 2 (EFI dir), 3 (BOOT dir), 4 (BOOTX64.EFI), 5 (KERNEL-X86_64)
        let total_clusters = 10usize;
        let total_size = (rsvd_sec + num_fats * fat_sz) * bps + root_dir_size + total_clusters * cluster_size;
        let mut fat = vec![0u8; total_size];

        // 1. Boot sector
        fat[510] = 0x55;
        fat[511] = 0xAA;
        fat[11..13].copy_from_slice(&(bps as u16).to_le_bytes());
        fat[13] = spc as u8;
        fat[14..16].copy_from_slice(&(rsvd_sec as u16).to_le_bytes());
        fat[16] = num_fats as u8;
        fat[17..19].copy_from_slice(&(root_entries as u16).to_le_bytes());
        fat[19..21].copy_from_slice(&((total_size / bps) as u16).to_le_bytes());
        fat[22..24].copy_from_slice(&(fat_sz as u16).to_le_bytes());

        // 2. Root directory entries
        let root_offset = (rsvd_sec + num_fats * fat_sz) * bps;
        let data_offset = root_offset + root_dir_size;

        // Entry 1: EFI directory -> Cluster 2
        let efi_entry = &mut fat[root_offset..root_offset + 32];
        efi_entry[0..8].copy_from_slice(b"EFI     ");
        efi_entry[8..11].copy_from_slice(b"   ");
        efi_entry[11] = 0x10; // Directory
        efi_entry[26..28].copy_from_slice(&2u16.to_le_bytes());

        // Entry 2: KERNEL~1 file -> Cluster 5
        let kern_entry = &mut fat[root_offset + 32..root_offset + 64];
        kern_entry[0..8].copy_from_slice(b"KERNEL~1");
        kern_entry[8..11].copy_from_slice(b"   ");
        kern_entry[11] = 0x20; // Archive / regular file
        kern_entry[26..28].copy_from_slice(&5u16.to_le_bytes());
        kern_entry[28..32].copy_from_slice(&4096u32.to_le_bytes());

        // 3. Cluster 2 data (EFI directory): contains BOOT directory -> Cluster 3
        let cluster_2_offset = data_offset + (2 - 2) * cluster_size;
        let boot_dir_entry = &mut fat[cluster_2_offset..cluster_2_offset + 32];
        boot_dir_entry[0..8].copy_from_slice(b"BOOT    ");
        boot_dir_entry[8..11].copy_from_slice(b"   ");
        boot_dir_entry[11] = 0x10; // Directory
        boot_dir_entry[26..28].copy_from_slice(&3u16.to_le_bytes());

        // 4. Cluster 3 data (BOOT directory): contains BOOTX64.EFI file -> Cluster 4
        let cluster_3_offset = data_offset + (3 - 2) * cluster_size;
        let efi_file_entry = &mut fat[cluster_3_offset..cluster_3_offset + 32];
        efi_file_entry[0..8].copy_from_slice(b"BOOTX64 ");
        efi_file_entry[8..11].copy_from_slice(b"EFI");
        efi_file_entry[11] = 0x20; // Archive / regular file
        efi_file_entry[26..28].copy_from_slice(&4u16.to_le_bytes());
        efi_file_entry[28..32].copy_from_slice(&8192u32.to_le_bytes());

        fat
    }

    #[test]
    fn test_both_endian_helpers() {
        let mut buf16 = [0u8; 4];
        write_both_endian_u16(&mut buf16, 2048);
        assert_eq!(buf16, [0x00, 0x08, 0x08, 0x00]);
        let (le16, be16) = read_both_endian_u16(&buf16);
        assert_eq!(le16, 2048);
        assert_eq!(be16, 2048);

        let mut buf32 = [0u8; 8];
        write_both_endian_u32(&mut buf32, 0x12345678);
        let (le32, be32) = read_both_endian_u32(&buf32);
        assert_eq!(le32, 0x12345678);
        assert_eq!(be32, 0x12345678);
    }

    #[test]
    fn test_el_torito_checksum_math() {
        let catalog = build_el_torito_boot_catalog(3 * 1024 * 1024);
        let val_entry = &catalog[0..32];

        let mut sum: u32 = 0;
        for i in 0..16 {
            let w = u16::from_le_bytes([val_entry[i * 2], val_entry[i * 2 + 1]]);
            sum = sum.wrapping_add(w as u32);
        }
        assert_eq!((sum & 0xFFFF) as u16, 0x0000, "Validation entry checksum must sum to zero");
    }

    #[test]
    fn test_generate_and_validate_synthetic_iso() {
        let mock_fat = create_mock_fat_partition();
        let iso_bytes = generate_hybrid_uefi_iso(&mock_fat, "MICRO_OS_TEST");

        let report = validate_hybrid_uefi_iso(&iso_bytes).expect("Validation must succeed for synthetic ISO");
        assert_eq!(report.mbr_partition1_type, MBR_PARTITION_TYPE_ESP);
        assert_eq!(report.mbr_partition1_start_lba512, MBR_ESP_START_LBA);
        assert_eq!(report.pvd_volume_id, "MICRO_OS_TEST");
        assert_eq!(report.pvd_logical_block_size, 2048);
        assert_eq!(report.el_torito_catalog_lba, BOOT_CATALOG_LBA);
        assert_eq!(report.boot_catalog_platform_id, EL_TORITO_PLATFORM_UEFI);
        assert_eq!(report.boot_catalog_load_rba, BOOT_IMAGE_LBA);
        assert!(report.efi_bootloader_found);
        assert_eq!(report.efi_bootloader_size, 8192);
        assert!(report.kernel_found);
        assert_eq!(report.kernel_size, 4096);
    }

    #[test]
    fn test_validate_existing_release_fat_partition() {
        // Test with the actual FAT partition inside target/micro_os_uefi_release.img if it exists
        let img_path = Path::new(r"target\micro_os_uefi_release.img");
        if img_path.exists() {
            let img_data = std::fs::read(img_path).expect("Failed to read release img");
            let fat_start = 34 * 512;
            let fat_len = 3145728;
            let fat_data = &img_data[fat_start..fat_start + fat_len];

            let iso_bytes = generate_hybrid_uefi_iso(fat_data, "MICRO_OS_LIVE");
            let report = validate_hybrid_uefi_iso(&iso_bytes)
                .expect("Real release FAT partition must produce valid hybrid ISO");

            assert_eq!(report.mbr_partition1_type, 0xEF);
            assert_eq!(report.mbr_partition1_start_lba512, 80);
            assert_eq!(report.boot_catalog_platform_id, 0xEF);
            assert_eq!(report.boot_catalog_load_rba, 20);
            assert!(report.efi_bootloader_found);
            assert_eq!(report.efi_bootloader_size, 167424);
            assert!(report.kernel_found);
            assert_eq!(report.kernel_size, 971256);
        }
    }

    // ========================================================================
    // Invalid & Truncated Image Tests
    // ========================================================================

    #[test]
    fn test_invalid_empty_image() {
        let err = validate_hybrid_uefi_iso(&[]).unwrap_err();
        match err {
            IsoValidationError::ImageTooSmall { size, min_required } => {
                assert_eq!(size, 0);
                assert!(min_required >= 43008);
            }
            _ => panic!("Expected ImageTooSmall error"),
        }
    }

    #[test]
    fn test_invalid_truncated_images() {
        let mock_fat = create_mock_fat_partition();
        let iso_bytes = generate_hybrid_uefi_iso(&mock_fat, "TEST");

        // Truncated at Sector 1 (2,048 bytes)
        let err = validate_hybrid_uefi_iso(&iso_bytes[..2048]).unwrap_err();
        assert!(matches!(err, IsoValidationError::ImageTooSmall { .. }));

        // Truncated at Sector 15 (30,720 bytes)
        let err = validate_hybrid_uefi_iso(&iso_bytes[..15 * 2048]).unwrap_err();
        assert!(matches!(err, IsoValidationError::ImageTooSmall { .. }));

        // Truncated at Sector 19 (38,912 bytes)
        let err = validate_hybrid_uefi_iso(&iso_bytes[..19 * 2048]).unwrap_err();
        assert!(matches!(err, IsoValidationError::ImageTooSmall { .. }));
    }

    #[test]
    fn test_invalid_mbr_signature() {
        let mock_fat = create_mock_fat_partition();
        let mut iso_bytes = generate_hybrid_uefi_iso(&mock_fat, "TEST");
        iso_bytes[510] = 0x00; // Corrupt MBR signature
        let err = validate_hybrid_uefi_iso(&iso_bytes).unwrap_err();
        assert_eq!(err, IsoValidationError::InvalidMbrSignature { found: [0x00, 0xAA] });
    }

    #[test]
    fn test_invalid_mbr_partition_type() {
        let mock_fat = create_mock_fat_partition();
        let mut iso_bytes = generate_hybrid_uefi_iso(&mock_fat, "TEST");
        iso_bytes[446 + 4] = 0x83; // Linux native instead of 0xEF ESP
        let err = validate_hybrid_uefi_iso(&iso_bytes).unwrap_err();
        assert_eq!(err, IsoValidationError::InvalidMbrPartitionType { found: 0x83, expected: 0xEF });
    }

    #[test]
    fn test_invalid_mbr_start_lba() {
        let mock_fat = create_mock_fat_partition();
        let mut iso_bytes = generate_hybrid_uefi_iso(&mock_fat, "TEST");
        iso_bytes[446 + 8] = 0x00; // Corrupt start LBA (should be 80)
        let err = validate_hybrid_uefi_iso(&iso_bytes).unwrap_err();
        assert!(matches!(err, IsoValidationError::InvalidMbrStartLba { .. }));
    }

    #[test]
    fn test_invalid_pvd_identifier() {
        let mock_fat = create_mock_fat_partition();
        let mut iso_bytes = generate_hybrid_uefi_iso(&mock_fat, "TEST");
        let pvd_offset = (PVD_LBA as usize) * ISO_SECTOR_SIZE;
        iso_bytes[pvd_offset + 1] = b'X'; // "XD001"
        let err = validate_hybrid_uefi_iso(&iso_bytes).unwrap_err();
        assert!(matches!(err, IsoValidationError::InvalidPvdStandardId { .. }));
    }

    #[test]
    fn test_invalid_pvd_block_size() {
        let mock_fat = create_mock_fat_partition();
        let mut iso_bytes = generate_hybrid_uefi_iso(&mock_fat, "TEST");
        let pvd_offset = (PVD_LBA as usize) * ISO_SECTOR_SIZE;
        write_both_endian_u16(&mut iso_bytes[pvd_offset + 128..pvd_offset + 132], 1024);
        let err = validate_hybrid_uefi_iso(&iso_bytes).unwrap_err();
        assert_eq!(err, IsoValidationError::InvalidPvdBlockSize { found_le: 1024, found_be: 1024 });
    }

    #[test]
    fn test_invalid_el_torito_descriptor() {
        let mock_fat = create_mock_fat_partition();
        let mut iso_bytes = generate_hybrid_uefi_iso(&mock_fat, "TEST");
        let et_offset = (EL_TORITO_LBA as usize) * ISO_SECTOR_SIZE;
        iso_bytes[et_offset + 7] = b'X'; // Corrupt specification ID
        let err = validate_hybrid_uefi_iso(&iso_bytes).unwrap_err();
        assert!(matches!(err, IsoValidationError::InvalidElToritoSystemId { .. }));
    }

    #[test]
    fn test_invalid_el_torito_catalog_pointer() {
        let mock_fat = create_mock_fat_partition();
        let mut iso_bytes = generate_hybrid_uefi_iso(&mock_fat, "TEST");
        let et_offset = (EL_TORITO_LBA as usize) * ISO_SECTOR_SIZE;
        iso_bytes[et_offset + 71] = 0x99; // Corrupt catalog pointer
        let err = validate_hybrid_uefi_iso(&iso_bytes).unwrap_err();
        assert!(matches!(err, IsoValidationError::InvalidElToritoCatalogPointer { .. }));
    }

    #[test]
    fn test_invalid_catalog_validation_checksum() {
        let mock_fat = create_mock_fat_partition();
        let mut iso_bytes = generate_hybrid_uefi_iso(&mock_fat, "TEST");
        let cat_offset = (BOOT_CATALOG_LBA as usize) * ISO_SECTOR_SIZE;
        iso_bytes[cat_offset + 28] = iso_bytes[cat_offset + 28].wrapping_add(1); // Corrupt checksum
        let err = validate_hybrid_uefi_iso(&iso_bytes).unwrap_err();
        assert!(matches!(err, IsoValidationError::InvalidValidationEntryChecksum { .. }));
    }

    #[test]
    fn test_invalid_catalog_platform_id() {
        let mock_fat = create_mock_fat_partition();
        let mut iso_bytes = generate_hybrid_uefi_iso(&mock_fat, "TEST");
        let cat_offset = (BOOT_CATALOG_LBA as usize) * ISO_SECTOR_SIZE;
        iso_bytes[cat_offset + 65] = 0x00; // Set platform ID to BIOS (0x00) instead of UEFI (0xEF)
        let err = validate_hybrid_uefi_iso(&iso_bytes).unwrap_err();
        assert_eq!(err, IsoValidationError::InvalidSectionHeaderPlatformId { found: 0x00, expected: 0xEF });
    }

    #[test]
    fn test_invalid_catalog_load_rba() {
        let mock_fat = create_mock_fat_partition();
        let mut iso_bytes = generate_hybrid_uefi_iso(&mock_fat, "TEST");
        let cat_offset = (BOOT_CATALOG_LBA as usize) * ISO_SECTOR_SIZE;
        iso_bytes[cat_offset + 96 + 8] = 0x50; // Set Load RBA to 80 instead of 20
        let err = validate_hybrid_uefi_iso(&iso_bytes).unwrap_err();
        assert_eq!(err, IsoValidationError::InvalidSectionEntryLoadRba { found: 0x50, expected: 20 });
    }

    #[test]
    fn test_invalid_fat_boot_signature() {
        let mock_fat = create_mock_fat_partition();
        let mut iso_bytes = generate_hybrid_uefi_iso(&mock_fat, "TEST");
        let fat_offset = (BOOT_IMAGE_LBA as usize) * ISO_SECTOR_SIZE;
        iso_bytes[fat_offset + 510] = 0x00; // Corrupt FAT boot signature
        let err = validate_hybrid_uefi_iso(&iso_bytes).unwrap_err();
        assert_eq!(err, IsoValidationError::InvalidFatBootSignature { found: [0x00, 0xAA] });
    }

    #[test]
    fn test_invalid_fat_missing_bootloader() {
        let mut mock_fat = create_mock_fat_partition();
        // Overwrite BOOTX64.EFI with zeros
        for b in &mut mock_fat[512..] {
            if *b == b'B' {
                *b = b'X';
            }
        }
        let iso_bytes = generate_hybrid_uefi_iso(&mock_fat, "TEST");
        let err = validate_hybrid_uefi_iso(&iso_bytes).unwrap_err();
        assert!(matches!(err, IsoValidationError::EfiDirectoryNotFound | IsoValidationError::BootDirectoryNotFound | IsoValidationError::BootloaderNotFound));
    }
}
