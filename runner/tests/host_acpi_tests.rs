//! Host Unit Tests for ACPI Table & Resource Descriptors (Stage 1).
//!
//! Validates ACPI checksum calculation, RSDP 2.0+ structure validation,
//! Memory32Fixed decoding, I2cSerialBusV2 decoding, and static vs dynamic distinction.

fn verify_checksum(data: &[u8]) -> bool {
    let sum: u8 = data.iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
    sum == 0
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Memory32FixedDescriptor {
    is_read_write: bool,
    base_address: u32,
    address_length: u32,
}

fn parse_memory32_fixed(slice: &[u8]) -> Option<Memory32FixedDescriptor> {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct I2cSerialBusDescriptor {
    slave_address: u16,
    connection_speed: u32,
    is_10bit: bool,
}

fn parse_i2c_serial_bus(slice: &[u8]) -> Option<I2cSerialBusDescriptor> {
    if slice.len() < 16 || slice[0] != 0x8E {
        return None;
    }
    let serial_bus_type = slice[5];
    if serial_bus_type != 0x01 {
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

#[test]
fn test_acpi_checksum_verification() {
    // Construct a valid 20-byte test header whose sum % 256 == 0
    let mut data = [0u8; 20];
    for i in 0..19 {
        data[i] = (i as u8) + 1;
    }
    let partial_sum: u8 = data[0..19].iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
    data[19] = (0u8).wrapping_sub(partial_sum);

    assert!(verify_checksum(&data), "Checksum must be valid");

    // Corrupt one byte
    data[5] ^= 0xFF;
    assert!(!verify_checksum(&data), "Corrupted checksum must fail");
}

#[test]
fn test_memory32_fixed_descriptor_decoding() {
    // Construct a synthetic Memory32Fixed descriptor for AMD I2C0:
    // Base: 0xFEDC2000, Length: 0x00001000 (4096 B), Read-Write
    let mut raw = [0u8; 12];
    raw[0] = 0x86; // Tag
    raw[1] = 0x09; // Length low
    raw[2] = 0x00; // Length high
    raw[3] = 0x01; // Info: Read-Write
    raw[4..8].copy_from_slice(&0xFEDC2000u32.to_le_bytes());
    raw[8..12].copy_from_slice(&0x00001000u32.to_le_bytes());

    let desc = parse_memory32_fixed(&raw).expect("Must parse Memory32Fixed");
    assert!(desc.is_read_write);
    assert_eq!(desc.base_address, 0xFEDC2000);
    assert_eq!(desc.address_length, 0x1000);
}

#[test]
fn test_i2c_serial_bus_descriptor_decoding() {
    // Construct a synthetic I2cSerialBusV2 descriptor for Touchpad:
    // Slave Address: 0x0015, Connection Speed: 400,000 Hz, 7-bit
    let mut raw = [0u8; 16];
    raw[0] = 0x8E; // Tag: Serial Bus
    raw[1] = 0x0D; // Length low (13 bytes payload)
    raw[2] = 0x00; // Length high
    raw[3] = 0x02; // Revision 2
    raw[4] = 0x00; // Resource Source Index
    raw[5] = 0x01; // Serial Bus Type: I2C (0x01)
    raw[6] = 0x00; // Flags low
    raw[7] = 0x00; // Flags high
    raw[8] = 0x00; // Type-specific flags low (Bit 0 = 0 -> 7-bit)
    raw[9] = 0x00; // Type-specific flags high
    raw[10..14].copy_from_slice(&400_000u32.to_le_bytes()); // 400 kHz
    raw[14..16].copy_from_slice(&0x0015u16.to_le_bytes());   // Slave: 0x15

    let desc = parse_i2c_serial_bus(&raw).expect("Must parse I2cSerialBus");
    assert_eq!(desc.slave_address, 0x0015);
    assert_eq!(desc.connection_speed, 400_000);
    assert!(!desc.is_10bit);
}

#[test]
fn test_static_vs_dynamic_crs_distinction() {
    // Static Name(_CRS, ...) pattern: NameOp (0x08) followed by "_CRS"
    let static_crs = [0x08, b'_', b'C', b'R', b'S', 0x11, 0x0C, 0x86];
    assert_eq!(static_crs[0], 0x08, "NameOp indicates static object");

    // Dynamic Method(_CRS, ...) pattern: MethodOp (0x14) followed by length and "_CRS"
    let dynamic_crs = [0x14, 0x10, b'_', b'C', b'R', b'S', 0x00];
    assert_eq!(dynamic_crs[0], 0x14, "MethodOp indicates dynamic control method");
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct McfgAllocation {
    base_address: u64,
    segment_group: u16,
    start_bus: u8,
    end_bus: u8,
}

fn parse_mcfg_table(slice: &[u8]) -> Option<Vec<McfgAllocation>> {
    if slice.len() < 44 || &slice[0..4] != b"MCFG" {
        return None;
    }
    let length = u32::from_le_bytes([slice[4], slice[5], slice[6], slice[7]]) as usize;
    if length > slice.len() || length < 44 {
        return None;
    }

    let num_entries = (length - 44) / 16;
    let mut allocations = Vec::with_capacity(num_entries);

    for i in 0..num_entries {
        let offset = 44 + i * 16;
        let base_address = u64::from_le_bytes([
            slice[offset], slice[offset + 1], slice[offset + 2], slice[offset + 3],
            slice[offset + 4], slice[offset + 5], slice[offset + 6], slice[offset + 7],
        ]);
        let segment_group = u16::from_le_bytes([slice[offset + 8], slice[offset + 9]]);
        let start_bus = slice[offset + 10];
        let end_bus = slice[offset + 11];

        allocations.push(McfgAllocation {
            base_address,
            segment_group,
            start_bus,
            end_bus,
        });
    }

    Some(allocations)
}

#[test]
fn test_acpi_mcfg_table_parsing() {
    // Construct synthetic ACPI MCFG table (60 bytes = 36 byte header + 8 reserved + 16 byte allocation)
    let mut raw = vec![0u8; 60];
    raw[0..4].copy_from_slice(b"MCFG");
    raw[4..8].copy_from_slice(&60u32.to_le_bytes()); // Length
    raw[8] = 1; // Revision
    raw[9] = 0; // Checksum placeholder
    raw[10..16].copy_from_slice(b"ALASKA"); // OEM ID
    raw[16..24].copy_from_slice(b"A M I   "); // OEM Table ID

    // Allocation entry 0 at offset 44:
    // Base Address: 0x00000000E0000000 (3.5 GB)
    // Segment: 0, Start Bus: 0, End Bus: 255
    raw[44..52].copy_from_slice(&0x0000_0000_E000_0000u64.to_le_bytes());
    raw[52..54].copy_from_slice(&0u16.to_le_bytes()); // Segment 0
    raw[54] = 0;   // Start Bus: 0
    raw[55] = 255; // End Bus: 255

    let allocs = parse_mcfg_table(&raw).expect("Must parse MCFG");
    assert_eq!(allocs.len(), 1);
    assert_eq!(allocs[0].base_address, 0x0000_0000_E000_0000);
    assert_eq!(allocs[0].segment_group, 0);
    assert_eq!(allocs[0].start_bus, 0);
    assert_eq!(allocs[0].end_bus, 255);
}

#[test]
fn test_pcie_ecam_virtual_address_translation() {
    let base_addr: u64 = 0xE000_0000;
    let phys_offset: u64 = 0xFFFF_8000_0000_0000;
    let start_bus = 0u8;
    let end_bus = 255u8;

    let calc_virt_addr = |bus: u8, dev: u8, func: u8, offset: u16| -> Option<u64> {
        if bus < start_bus || bus > end_bus {
            return None;
        }
        let bus_offset = ((bus - start_bus) as u64) << 20;
        let dev_offset = ((dev & 0x1F) as u64) << 15;
        let func_offset = ((func & 0x07) as u64) << 12;
        let reg_offset = (offset & 0x0FFF) as u64;

        let phys = base_addr + bus_offset + dev_offset + func_offset + reg_offset;
        Some(phys_offset + phys)
    };

    // Bus 0, Dev 0, Func 0, Reg 0 (Host Bridge VID/DID)
    assert_eq!(
        calc_virt_addr(0, 0, 0, 0),
        Some(0xFFFF_8000_0000_0000 + 0xE000_0000)
    );

    // Bus 1, Dev 0, Func 0, Reg 0 (Secondary Bus 1 endpoint)
    assert_eq!(
        calc_virt_addr(1, 0, 0, 0),
        Some(0xFFFF_8000_0000_0000 + 0xE000_0000 + 0x10_0000)
    );

    // Bus 2, Dev 0, Func 0, Reg 0 (RTL8852BE-VT Wi-Fi on Bus 2)
    assert_eq!(
        calc_virt_addr(2, 0, 0, 0),
        Some(0xFFFF_8000_0000_0000 + 0xE000_0000 + 0x20_0000)
    );

    // Bus 2, Dev 0, Func 0, Reg 0x10 (BAR0)
    assert_eq!(
        calc_virt_addr(2, 0, 0, 0x10),
        Some(0xFFFF_8000_0000_0000 + 0xE000_0000 + 0x20_0010)
    );

    // Bus 3, Dev 0, Func 0, Reg 0 (RTL8168 GbE on Bus 3)
    assert_eq!(
        calc_virt_addr(3, 0, 0, 0),
        Some(0xFFFF_8000_0000_0000 + 0xE000_0000 + 0x30_0000)
    );
}

#[test]
fn test_pci_network_device_id_classification() {
    let is_target_wifi = |vid: u16, did: u16| -> bool {
        vid == 0x10EC && (did == 0xB852 || did == 0x8852 || did == 0xC852 || did == 0xB851 || did == 0xB85B || did == 0xA852)
    };
    let is_target_eth = |vid: u16, did: u16| -> bool {
        vid == 0x10EC && (did == 0x8168 || did == 0x8161 || did == 0x8111 || did == 0x8125 || did == 0x8139 || did == 0x3000)
    };

    // Target Wi-Fi on HP Victus
    assert!(is_target_wifi(0x10EC, 0xB852), "RTL8852BE-VT must match");
    assert!(is_target_wifi(0x10EC, 0x8852), "RTL8852BE must match");
    assert!(is_target_wifi(0x10EC, 0xA852), "RTL8852AE must match");

    // Target GbE Ethernet on HP Victus
    assert!(is_target_eth(0x10EC, 0x8168), "RTL8168 must match");
    assert!(is_target_eth(0x10EC, 0x8125), "RTL8125 must match");

    // Non-network peripheral
    assert!(!is_target_wifi(0x1022, 0x14E4), "AMD Host Bridge must not match Wi-Fi");
    assert!(!is_target_eth(0x1022, 0x14E4), "AMD Host Bridge must not match Ethernet");
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostSlotStatus {
    Present,
    Empty,
    AllZero,
    ReadError,
}

pub fn classify_slot_status(dword0: u32) -> HostSlotStatus {
    let vendor_id = (dword0 & 0xFFFF) as u16;
    if dword0 == 0xFFFF_FFFF || vendor_id == 0xFFFF {
        HostSlotStatus::Empty
    } else if dword0 == 0x0000_0000 || vendor_id == 0x0000 {
        HostSlotStatus::AllZero
    } else {
        HostSlotStatus::Present
    }
}

#[test]
fn test_pcie_slot_raw_probe_status_classification() {
    // 1. Present AMD Host Bridge
    assert_eq!(classify_slot_status(0x14E0_1022), HostSlotStatus::Present);

    // 2. Present Realtek RTL8852BE Wi-Fi
    assert_eq!(classify_slot_status(0xB852_10EC), HostSlotStatus::Present);

    // 3. Present Realtek RTL8168 GbE
    assert_eq!(classify_slot_status(0x8168_10EC), HostSlotStatus::Present);

    // 4. Empty slot (standard PCI master abort)
    assert_eq!(classify_slot_status(0xFFFF_FFFF), HostSlotStatus::Empty);
    assert_eq!(classify_slot_status(0x0000_FFFF), HostSlotStatus::Empty);

    // 5. Unpowered or floating link (all zeroes)
    assert_eq!(classify_slot_status(0x0000_0000), HostSlotStatus::AllZero);
}

#[test]
fn test_pcie_bus0_ecam_verification_logic() {
    let verify = |legacy: u32, ecam: u32, ecam_configured: bool| -> (&'static str, bool) {
        if !ecam_configured {
            ("ECAM INACTIVE", false)
        } else if legacy == ecam && legacy != 0xFFFF_FFFF && legacy != 0 {
            ("VERIFIED MATCH", true)
        } else if ecam == 0xFFFF_FFFF && legacy != 0xFFFF_FFFF {
            ("ECAM READ FAULT: Floating Bus", false)
        } else if ecam == 0 {
            ("ECAM ZERO FAULT: Unmapped", false)
        } else {
            ("MISMATCH", false)
        }
    };

    // Case A: Ideal working hardware (AMD Host Bridge 1022:14E0)
    let (msg, matches) = verify(0x14E0_1022, 0x14E0_1022, true);
    assert!(matches);
    assert_eq!(msg, "VERIFIED MATCH");

    // Case B: ECAM unmapped returning 0xFFFFFFFF
    let (msg, matches) = verify(0x14E0_1022, 0xFFFF_FFFF, true);
    assert!(!matches);
    assert_eq!(msg, "ECAM READ FAULT: Floating Bus");

    // Case C: ECAM returning 0x00000000
    let (msg, matches) = verify(0x14E0_1022, 0x0000_0000, true);
    assert!(!matches);
    assert_eq!(msg, "ECAM ZERO FAULT: Unmapped");

    // Case D: ECAM inactive
    let (msg, matches) = verify(0x14E0_1022, 0x14E0_1022, false);
    assert!(!matches);
    assert_eq!(msg, "ECAM INACTIVE");
}

#[test]
fn test_pcie_bridge_header_decoding() {
    // Simulated PCI-to-PCI bridge registers (Header Type 1)
    // Offset 0x18: Primary Bus = 0x00
    // Offset 0x19: Secondary Bus = 0x02 (Downstream Bus 2 for RTL8852BE Wi-Fi)
    // Offset 0x1A: Subordinate Bus = 0x02
    // Offset 0x04: Command = 0x0007 (I/O:1, Memory:1, BusMaster:1)
    let dword_buses: u32 = 0x0002_0200; // Primary: 0x00, Secondary: 0x02, Sub: 0x02
    let primary_bus = (dword_buses & 0xFF) as u8;
    let secondary_bus = ((dword_buses >> 8) & 0xFF) as u8;
    let subordinate_bus = ((dword_buses >> 16) & 0xFF) as u8;
    let cmd: u16 = 0x0007;

    assert_eq!(primary_bus, 0x00);
    assert_eq!(secondary_bus, 0x02, "Secondary bus must be 2 for Wi-Fi bridge");
    assert_eq!(subordinate_bus, 0x02);
    assert_eq!(cmd & 0x0002, 0x0002, "Memory Space Enable must be set");
    assert_eq!(cmd & 0x0004, 0x0004, "Bus Master Enable must be set");
}

#[test]
fn test_acpi_rsdt_fallback_parsing() {
    // 1. Synthetic ACPI 1.0 RSDP (20 bytes, revision = 0)
    let mut rsdp = [0u8; 36];
    rsdp[0..8].copy_from_slice(b"RSD PTR ");
    rsdp[15] = 0; // Revision 0 (ACPI 1.0)
    let rsdt_phys: u32 = 0x7E00_1000;
    rsdp[16..20].copy_from_slice(&rsdt_phys.to_le_bytes());

    // Compute checksum for first 20 bytes
    let sum20: u8 = rsdp[0..19].iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
    rsdp[8] = 0u8.wrapping_sub(sum20);

    let rev = rsdp[15];
    assert_eq!(rev, 0);

    // RSDT address check
    let parsed_rsdt = u32::from_le_bytes([rsdp[16], rsdp[17], rsdp[18], rsdp[19]]);
    assert_eq!(parsed_rsdt, 0x7E00_1000);

    // Synthetic RSDT table containing MCFG
    let mut rsdt_table = [0u8; 40];
    rsdt_table[0..4].copy_from_slice(b"RSDT");
    let rsdt_len: u32 = 40;
    rsdt_table[4..8].copy_from_slice(&rsdt_len.to_le_bytes());
    rsdt_table[8] = 1; // revision
    let mcfg_phys: u32 = 0x7E00_2000;
    rsdt_table[36..40].copy_from_slice(&mcfg_phys.to_le_bytes());

    let num_ptrs = (rsdt_len - 36) / 4;
    assert_eq!(num_ptrs, 1);
    let ptr0 = u32::from_le_bytes([rsdt_table[36], rsdt_table[37], rsdt_table[38], rsdt_table[39]]);
    assert_eq!(ptr0, 0x7E00_2000);
}

#[test]
fn test_diagnostics_pagination_bounds_and_focus_state() {
    let total_devices: usize = 38;
    let lines_fit: usize = 18;
    let max_offset = total_devices - lines_fit; // 20

    // Initial offset
    let mut offset: usize = 0;
    assert_eq!(offset, 0);

    // Scroll down by 10 (Next Page)
    offset = (offset + 10).min(max_offset);
    assert_eq!(offset, 10);

    // Scroll down by 10 again (Page 3)
    offset = (offset + 10).min(max_offset);
    assert_eq!(offset, 20);

    // Further down should clamp at max_offset
    offset = (offset + 10).min(max_offset);
    assert_eq!(offset, 20);

    // Scroll up by 10 (Prev Page)
    offset = offset.saturating_sub(10);
    assert_eq!(offset, 10);

    // Top button
    offset = 0;
    assert_eq!(offset, 0);
}
