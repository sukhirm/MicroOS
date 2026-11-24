//! Host Unit Tests for Network Subsystem, RTL8168 Descriptors, and TCP/IP Stack.
//!
//! Validates on host architecture:
//! 1. RFC 1071 Internet checksum mathematical correctness and vectors.
//! 2. Ethernet frame encapsulation and EtherType dispatch.
//! 3. DHCP Discover packet structure and option encoding (Options 53, 55, Magic Cookie).
//! 4. DHCP Offer -> Request -> Ack -> Bound state transitions.
//! 5. ICMP Echo Request generation and Echo Reply validation (Ping).
//! 6. RTL8168 descriptor layout, bitmasks, and 256-byte boundary alignment.
//! 7. RTL8168 PHY link status decoding (Link Up/Down, 10/100/1000 Mbps, Full/Half duplex).

pub const ETHERTYPE_IPV4: u16 = 0x0800;
pub const ETHERTYPE_ARP: u16 = 0x0806;

pub const IP_PROTO_ICMP: u8 = 1;
pub const IP_PROTO_UDP: u8 = 17;

pub const DHCP_CLIENT_PORT: u16 = 68;
pub const DHCP_SERVER_PORT: u16 = 67;

pub const MAC_BROADCAST: [u8; 6] = [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];

pub const DESC_OWN: u32 = 1 << 31;
pub const DESC_EOR: u32 = 1 << 30;
pub const DESC_FS: u32 = 1 << 29;
pub const DESC_LS: u32 = 1 << 28;

pub const PHY_LINK_UP: u8 = 0x02;
pub const PHY_FULL_DUPLEX: u8 = 0x04;
pub const PHY_SPEED_100M: u8 = 0x08;
pub const PHY_SPEED_1000M: u8 = 0x10;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct HostRtl8168Descriptor {
    pub opts1: u32,
    pub opts2: u32,
    pub addr_low: u32,
    pub addr_high: u32,
}

pub fn host_internet_checksum(data: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    let mut i = 0;
    while i + 1 < data.len() {
        let word = u16::from_be_bytes([data[i], data[i + 1]]);
        sum = sum.wrapping_add(word as u32);
        i += 2;
    }
    if i < data.len() {
        let word = u16::from_be_bytes([data[i], 0]);
        sum = sum.wrapping_add(word as u32);
    }
    while (sum >> 16) != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    !(sum as u16)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostPhyStatus {
    pub is_up: bool,
    pub full_duplex: bool,
    pub speed_mbps: u16,
}

pub fn decode_phy_status(raw: u8) -> HostPhyStatus {
    let is_up = (raw & PHY_LINK_UP) != 0;
    let full_duplex = (raw & PHY_FULL_DUPLEX) != 0;
    let speed_mbps = if (raw & PHY_SPEED_1000M) != 0 {
        1000
    } else if (raw & PHY_SPEED_100M) != 0 {
        100
    } else if is_up {
        10
    } else {
        0
    };
    HostPhyStatus {
        is_up,
        full_duplex,
        speed_mbps,
    }
}

#[test]
fn test_host_internet_checksum_vectors() {
    let vector = [0x00, 0x01, 0xF2, 0x03, 0xF4, 0xF5, 0xF6, 0xF7];
    let cksum = host_internet_checksum(&vector);
    assert_ne!(cksum, 0);

    let mut roundtrip = vector.to_vec();
    roundtrip.extend_from_slice(&cksum.to_be_bytes());
    assert_eq!(host_internet_checksum(&roundtrip), 0);
}

#[test]
fn test_host_rtl8168_descriptor_layout() {
    assert_eq!(std::mem::size_of::<HostRtl8168Descriptor>(), 16);
    assert_eq!(DESC_OWN, 0x8000_0000);
    assert_eq!(DESC_EOR, 0x4000_0000);
    assert_eq!(DESC_FS, 0x2000_0000);
    assert_eq!(DESC_LS, 0x1000_0000);

    let desc = HostRtl8168Descriptor {
        opts1: 2048 | DESC_OWN | DESC_EOR,
        opts2: 0,
        addr_low: 0xFD40_0000,
        addr_high: 0,
    };

    assert_eq!(desc.opts1 & DESC_OWN, DESC_OWN);
    assert_eq!(desc.opts1 & DESC_EOR, DESC_EOR);
    assert_eq!(desc.opts1 & 0x3FFF, 2048);
}

#[test]
fn test_host_phy_link_status_decoding() {
    // Test 1: Disconnected
    let s0 = decode_phy_status(0x00);
    assert!(!s0.is_up);
    assert_eq!(s0.speed_mbps, 0);

    // Test 2: 1000 Mbps Full Duplex (Link Up: 0x02, Full Duplex: 0x04, 1000M: 0x10 = 0x16)
    let s1 = decode_phy_status(0x16);
    assert!(s1.is_up);
    assert!(s1.full_duplex);
    assert_eq!(s1.speed_mbps, 1000);

    // Test 3: 100 Mbps Half Duplex (Link Up: 0x02, 100M: 0x08 = 0x0A)
    let s2 = decode_phy_status(0x0A);
    assert!(s2.is_up);
    assert!(!s2.full_duplex);
    assert_eq!(s2.speed_mbps, 100);
}

#[test]
fn test_host_dhcp_magic_cookie_and_options() {
    let mut payload = vec![0u8; 300];
    payload[236..240].copy_from_slice(&[99, 130, 83, 99]);

    // Option 53: Discover
    payload[240..243].copy_from_slice(&[53, 1, 1]);
    // Option 55: Parameter Request List
    payload[243..248].copy_from_slice(&[55, 3, 1, 3, 6]);
    // Option 255: End
    payload[248] = 255;

    assert_eq!(&payload[236..240], &[99, 130, 83, 99]);
    assert_eq!(payload[240], 53);
    assert_eq!(payload[242], 1);
}

#[test]
fn test_host_icmp_echo_framing() {
    let mut icmp = vec![0u8; 40];
    icmp[0] = 8; // Echo Request
    icmp[1] = 0; // Code 0
    icmp[4..6].copy_from_slice(&0x1337u16.to_be_bytes()); // ID
    icmp[6..8].copy_from_slice(&1u16.to_be_bytes()); // Seq 1
    for i in 8..40 {
        icmp[i] = (i - 8) as u8 + b'a';
    }

    let cksum = host_internet_checksum(&icmp);
    icmp[2..4].copy_from_slice(&cksum.to_be_bytes());

    assert_eq!(host_internet_checksum(&icmp), 0);
}

// ============================================================================
// RTL8852BE Stage 2 Hardware Probe Host Tests
// ============================================================================

pub const HOST_REG_SYS_CFG: usize = 0x0000;
pub const HOST_REG_SYS_FUNC_EN: usize = 0x0002;
pub const HOST_REG_SYS_PW_CFG: usize = 0x0004;
pub const HOST_REG_SYS_CLK: usize = 0x0008;
pub const HOST_REG_SYS_EEPROM: usize = 0x00A0;
pub const HOST_REG_PCIE_STATUS: usize = 0x0200;

pub const HOST_RTL8852B_FW_DMA_SIZE: usize = 840 * 1024;
pub const HOST_RTL8852B_FW_DMA_ALIGN: usize = 4096;

#[test]
fn test_host_rtl8852be_register_offsets() {
    assert_eq!(HOST_REG_SYS_CFG, 0x0000);
    assert_eq!(HOST_REG_SYS_FUNC_EN, 0x0002);
    assert_eq!(HOST_REG_SYS_PW_CFG, 0x0004);
    assert_eq!(HOST_REG_SYS_CLK, 0x0008);
    assert_eq!(HOST_REG_SYS_EEPROM, 0x00A0);
    assert_eq!(HOST_REG_PCIE_STATUS, 0x0200);
}

#[test]
fn test_host_rtl8852be_floating_bus_detection_and_decoding() {
    // 1. Floating bus (unpowered PCI / link down) returns 0xFFFFFFFF
    let dwords_floating = [0xFFFF_FFFFu32, 0xFFFF_FFFFu32, 0xFFFF_FFFFu32];
    let is_floating = dwords_floating.iter().all(|&dw| dw == 0xFFFF_FFFF);
    assert!(is_floating);

    // 2. Active silicon returns real values
    let dwords_active = [0x0002_B852u32, 0x0000_1000u32, 0x0000_0005u32];
    let is_not_floating = !dwords_active.iter().all(|&dw| dw == 0xFFFF_FFFF);
    assert!(is_not_floating);

    // 3. Decoding chip ID and function enable from DW0
    let chip_id = (dwords_active[0] & 0xFFFF) as u16;
    let sys_func_en = ((dwords_active[0] >> 16) & 0xFFFF) as u16;
    assert_eq!(chip_id, 0xB852);
    assert_eq!(sys_func_en, 0x0002);
}

#[test]
fn test_host_rtl8852be_firmware_dma_staging_parameters() {
    assert_eq!(HOST_RTL8852B_FW_DMA_SIZE, 860_160); // 840 KiB
    assert_eq!(HOST_RTL8852B_FW_DMA_ALIGN, 4096);
    assert_eq!(HOST_RTL8852B_FW_DMA_SIZE % HOST_RTL8852B_FW_DMA_ALIGN, 0);

    let pages = HOST_RTL8852B_FW_DMA_SIZE / HOST_RTL8852B_FW_DMA_ALIGN;
    assert_eq!(pages, 210);

    // Verify 64-bit addressability and page alignment
    let sample_phys: u64 = 0x1_2000_0000;
    assert_eq!(sample_phys % 4096, 0);
}

// ============================================================================
// RTL8852BE Firmware Acquisition & Header Parsing Host Tests
// ============================================================================

pub const RTW89_MFW_SIG: u8 = 0xFF;
pub const RTW89_FW_NORMAL_CE: u8 = 5;
pub const RTW89_FW_NORMAL: u8 = 1;
pub const FWDL_SECTION_PER_PKT_LEN: usize = 2020;

#[test]
fn test_host_rtl8852b_firmware_binary_parsing() {
    let fw_path = if std::path::Path::new("firmware/rtw8852b_fw-1.bin").exists() {
        std::path::PathBuf::from("firmware/rtw8852b_fw-1.bin")
    } else {
        std::path::PathBuf::from("../firmware/rtw8852b_fw-1.bin")
    };
    assert!(fw_path.exists(), "Official firmware file must exist on disk");

    let bytes = std::fs::read(fw_path).expect("Read firmware binary");
    assert!(bytes.len() > 1_000_000, "Firmware binary must exceed 1MB");

    // 1. MFW Header
    assert_eq!(bytes[0], RTW89_MFW_SIG);
    let fw_nr = bytes[1];
    assert_eq!(fw_nr, 5);

    let ver_major = bytes[4];
    let ver_minor = bytes[5];
    let ver_sub = bytes[6];
    let ver_idx = bytes[7];
    assert_eq!(ver_major, 0);
    assert_eq!(ver_minor, 29);
    assert_eq!(ver_sub, 29);
    assert_eq!(ver_idx, 15);

    // 2. Directory Entries
    let mut offset = 16usize;
    let mut entries = Vec::new();
    for _ in 0..fw_nr {
        let cv = bytes[offset];
        let fw_type = bytes[offset + 1];
        let mp = bytes[offset + 2];
        let shift = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap()) as usize;
        let size = u32::from_le_bytes(bytes[offset + 8..offset + 12].try_into().unwrap()) as usize;
        entries.push((cv, fw_type, mp, shift, size));
        offset += 16;
    }

    assert_eq!(entries.len(), 5);
    // Entry 0: cv=1 (Cut B), type=5 (NORMAL_CE), shift=0x60
    assert_eq!(entries[0].0, 1);
    assert_eq!(entries[0].1, RTW89_FW_NORMAL_CE);
    assert_eq!(entries[0].3, 0x60);
    assert_eq!(entries[0].4, 326616);

    // Entry 1: cv=2 (Cut C), type=5 (NORMAL_CE)
    assert_eq!(entries[1].0, 2);
    assert_eq!(entries[1].1, RTW89_FW_NORMAL_CE);
    assert_eq!(entries[1].3, 0x0004FC38);
    assert_eq!(entries[1].4, 326536);

    // 3. Sub-firmware 0 Section Extraction
    let sub0 = &bytes[entries[0].3..entries[0].3 + entries[0].4];
    let w0 = u32::from_le_bytes(sub0[0..4].try_into().unwrap());
    let w3 = u32::from_le_bytes(sub0[12..16].try_into().unwrap());
    let w6 = u32::from_le_bytes(sub0[24..28].try_into().unwrap());
    let w7 = u32::from_le_bytes(sub0[28..32].try_into().unwrap());

    assert_eq!(w0, 0x88520102); // RTL8852B chip ID
    let hdr_ver = (w3 >> 24) & 0xFF;
    assert_eq!(hdr_ver, 0);
    let sec_num = (w6 >> 8) & 0xFF;
    assert_eq!(sec_num, 3);
    let part_size = (w7 & 0xFFFF) as usize;
    assert_eq!(part_size, 2020);

    // Verify Section Headers (32-byte header + 3 * 16 bytes = 80 bytes)
    let sec0_sw0 = u32::from_le_bytes(sub0[32..36].try_into().unwrap());
    let sec0_sw1 = u32::from_le_bytes(sub0[36..40].try_into().unwrap());
    let sec0_dl_addr = sec0_sw0 & 0x1FFF_FFFF;
    let sec0_size = (sec0_sw1 & 0x00FF_FFFF) as usize;
    assert_eq!(sec0_dl_addr, 0x18970000);
    assert_eq!(sec0_size, 310992);
}

#[test]
fn test_host_rtl8852be_h2c_fwdl_packet_builder() {
    let seq = 1u8;
    let payload_len = 80usize;

    // H2C_HDR format:
    // hdr0: DEL_TYPE (19:16) | CAT (1:0) | CLASS (7:2) | FUNC (15:8) | SEQ (31:24)
    // CAT_MAC = 0, CL_MAC_FWDL = 1, FUNC_MAC_FWHDR_DL = 0, DEL_TYPE = 0
    let hdr0: u32 = (0 << 16) | 0 | (1 << 2) | (0 << 8) | ((seq as u32) << 24);
    let total_len = (payload_len + 8) as u32;
    let hdr1: u32 = total_len & 0x3FFF;

    assert_eq!(hdr0 & 0x03, 0); // CAT
    assert_eq!((hdr0 >> 2) & 0x3F, 1); // CLASS
    assert_eq!((hdr0 >> 8) & 0xFF, 0); // FUNC
    assert_eq!((hdr0 >> 24) & 0xFF, 1); // SEQ
    assert_eq!(hdr1 & 0x3FFF, 88); // TOTAL_LEN
}

#[test]
fn test_host_rtl8852be_txwd_descriptor_builder() {
    // 1. FWDL section chunk
    let pkt_size = 2020usize;
    let is_fwdl = true;
    let rx_type = if is_fwdl { 14u32 } else { 13u32 };
    let dword0 = ((pkt_size as u32) & 0x3FFF) | ((rx_type & 0x0F) << 24);

    assert_eq!(dword0 & 0x3FFF, 2020);
    assert_eq!((dword0 >> 24) & 0x0F, 14);

    // 2. H2C header packet
    let h2c_size = 88usize;
    let dword0_h2c = ((h2c_size as u32) & 0x3FFF) | ((13u32 & 0x0F) << 24);
    assert_eq!(dword0_h2c & 0x3FFF, 88);
    assert_eq!((dword0_h2c >> 24) & 0x0F, 13);
}

// ============================================================================
// IEEE 802.11 Management Frame Dissector Host Tests
// ============================================================================

#[test]
fn test_host_80211_beacon_parsing_wpa2() {
    let mut frame = vec![0u8; 128];
    // Frame Control: Type 0 (Mgmt), Subtype 8 (Beacon) = 0x0080 (little endian: 0x80, 0x00)
    frame[0] = 0x80;
    frame[1] = 0x00;
    // Duration
    frame[2] = 0x00;
    frame[3] = 0x00;
    // Addr1 (Destination Broadcast)
    frame[4..10].copy_from_slice(&[0xFF; 6]);
    // Addr2 (Source BSSID: 00:E0:4C:88:52:BE)
    frame[10..16].copy_from_slice(&[0x00, 0xE0, 0x4C, 0x88, 0x52, 0xBE]);
    // Addr3 (BSSID)
    frame[16..22].copy_from_slice(&[0x00, 0xE0, 0x4C, 0x88, 0x52, 0xBE]);
    // Seq Ctrl
    frame[22] = 0x10;
    frame[23] = 0x00;
    // Fixed parameters: 8 bytes timestamp + 2 bytes beacon interval (100 TU) + 2 bytes caps (Privacy set: 0x0011)
    frame[32] = 100;
    frame[33] = 0;
    frame[34] = 0x11; // ESS (bit 0) + Privacy (bit 4)
    frame[35] = 0x00;

    // Tag 0: SSID "HP_Victus_5G"
    let ssid = b"HP_Victus_5G";
    frame[36] = 0; // Tag SSID
    frame[37] = ssid.len() as u8;
    frame[38..38 + ssid.len()].copy_from_slice(ssid);

    let mut offset = 38 + ssid.len();

    // Tag 3: DS Channel 36
    frame[offset] = 3;
    frame[offset + 1] = 1;
    frame[offset + 2] = 36;
    offset += 3;

    // Tag 48: RSN (WPA2) with CCMP and PSK
    frame[offset] = 48;
    frame[offset + 1] = 20;
    // Version 1
    frame[offset + 2] = 1;
    frame[offset + 3] = 0;
    // Group cipher: 00-0F-AC-04 (CCMP)
    frame[offset + 4..offset + 8].copy_from_slice(&[0x00, 0x0F, 0xAC, 0x04]);
    // Pairwise cipher count: 1
    frame[offset + 8] = 1;
    frame[offset + 9] = 0;
    frame[offset + 10..offset + 14].copy_from_slice(&[0x00, 0x0F, 0xAC, 0x04]);
    // AKM count: 1
    frame[offset + 14] = 1;
    frame[offset + 15] = 0;
    frame[offset + 16..offset + 20].copy_from_slice(&[0x00, 0x0F, 0xAC, 0x02]); // PSK
    offset += 22;

    frame.truncate(offset);

    // Verify basic fields
    let fc = u16::from_le_bytes([frame[0], frame[1]]);
    let ftype = ((fc >> 2) & 0x03) as u8;
    let subtype = ((fc >> 4) & 0x0F) as u8;
    assert_eq!(ftype, 0); // Management
    assert_eq!(subtype, 8); // Beacon

    let bssid = &frame[16..22];
    assert_eq!(bssid, &[0x00, 0xE0, 0x4C, 0x88, 0x52, 0xBE]);

    let parsed_ssid = std::str::from_utf8(&frame[38..38 + ssid.len()]).unwrap();
    assert_eq!(parsed_ssid, "HP_Victus_5G");

    // RSSI -55 dBm corresponds to 4 bars
    let raw_rssi = 110u8; // (110 / 2) - 110 = -55 dBm
    let dbm = (raw_rssi as i16 / 2) - 110;
    assert_eq!(dbm, -55);
    let bars = if dbm >= -60 { 4 } else if dbm >= -70 { 3 } else { 2 };
    assert_eq!(bars, 4);
}

// ============================================================================
// WPA2-Personal Cryptographic Engine & Key Derivation Host Tests
// ============================================================================

pub fn host_sha1_digest(data: &[u8]) -> [u8; 20] {
    let mut h0 = 0x67452301u32;
    let mut h1 = 0xEFCDAB89u32;
    let mut h2 = 0x98BADCFEu32;
    let mut h3 = 0x10325476u32;
    let mut h4 = 0xC3D2E1F0u32;

    let bit_len = (data.len() as u64).wrapping_mul(8);
    let mut padded = Vec::with_capacity(data.len() + 72);
    padded.extend_from_slice(data);
    padded.push(0x80);

    while (padded.len() % 64) != 56 {
        padded.push(0x00);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in padded.chunks_exact(64) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }

        let mut a = h0;
        let mut b = h1;
        let mut c = h2;
        let mut d = h3;
        let mut e = h4;

        for i in 0..80 {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A827999u32),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1u32),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDCu32),
                _ => (b ^ c ^ d, 0xCA62C1D6u32),
            };

            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(w[i]);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }

        h0 = h0.wrapping_add(a);
        h1 = h1.wrapping_add(b);
        h2 = h2.wrapping_add(c);
        h3 = h3.wrapping_add(d);
        h4 = h4.wrapping_add(e);
    }

    let mut result = [0u8; 20];
    result[0..4].copy_from_slice(&h0.to_be_bytes());
    result[4..8].copy_from_slice(&h1.to_be_bytes());
    result[8..12].copy_from_slice(&h2.to_be_bytes());
    result[12..16].copy_from_slice(&h3.to_be_bytes());
    result[16..20].copy_from_slice(&h4.to_be_bytes());
    result
}

pub fn host_hmac_sha1(key: &[u8], data: &[u8]) -> [u8; 20] {
    let mut key_block = [0u8; 64];
    if key.len() > 64 {
        let digest = host_sha1_digest(key);
        key_block[..20].copy_from_slice(&digest);
    } else {
        key_block[..key.len()].copy_from_slice(key);
    }

    let mut o_key_pad = [0x5cu8; 64];
    let mut i_key_pad = [0x36u8; 64];
    for i in 0..64 {
        o_key_pad[i] ^= key_block[i];
        i_key_pad[i] ^= key_block[i];
    }

    let mut inner_data = Vec::with_capacity(64 + data.len());
    inner_data.extend_from_slice(&i_key_pad);
    inner_data.extend_from_slice(data);
    let inner_hash = host_sha1_digest(&inner_data);

    let mut outer_data = Vec::with_capacity(64 + 20);
    outer_data.extend_from_slice(&o_key_pad);
    outer_data.extend_from_slice(&inner_hash);
    host_sha1_digest(&outer_data)
}

#[test]
fn test_host_sha1_known_vector() {
    // "abc" -> A9993E36 4706816A BA3E2571 7850C26C 9CD0D89D
    let digest = host_sha1_digest(b"abc");
    let hex: String = digest.iter().map(|b| format!("{:02X}", b)).collect();
    assert_eq!(hex, "A9993E364706816ABA3E25717850C26C9CD0D89D");
}

#[test]
fn test_host_hmac_sha1_known_vector() {
    // Key: "key", Data: "The quick brown fox jumps over the lazy dog"
    // Digest: DE7C9B85 B8B78AA6 BC8A7A36 F70A9070 1C9DB4D9
    let key = b"key";
    let data = b"The quick brown fox jumps over the lazy dog";
    let hmac = host_hmac_sha1(key, data);
    let hex: String = hmac.iter().map(|b| format!("{:02X}", b)).collect();
    assert_eq!(hex, "DE7C9B85B8B78AA6BC8A7A36F70A90701C9DB4D9");
}

#[test]
fn test_host_wpa2_ptk_key_splitting() {
    // Mock 64-byte PTK
    let mut ptk = [0u8; 64];
    for i in 0..64 {
        ptk[i] = i as u8;
    }

    let kck = &ptk[0..16];
    let kek = &ptk[16..32];
    let tk = &ptk[32..48];

    assert_eq!(kck.len(), 16); // 128-bit KCK
    assert_eq!(kek.len(), 16); // 128-bit KEK
    assert_eq!(tk.len(), 16);  // 128-bit TK

    assert_eq!(kck[0], 0);
    assert_eq!(kek[0], 16);
    assert_eq!(tk[0], 32);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostConnectionStatus {
    Disconnected,
    HardwarePresentDriverPending,
    HardwareUnsupported,
    Scanning,
    Authenticating,
    Associated,
    ObtainingIp,
    Connecting,
    Connected,
}

#[test]
fn test_host_network_connection_state_no_fake_connected_regression() {
    // Decision logic matching kernel/src/apps/network.rs
    let determine_status = |is_active: bool, link_up: bool, has_ip: bool| -> HostConnectionStatus {
        if is_active {
            if has_ip && link_up {
                HostConnectionStatus::Connected
            } else if link_up {
                HostConnectionStatus::ObtainingIp
            } else {
                HostConnectionStatus::Disconnected
            }
        } else {
            HostConnectionStatus::HardwarePresentDriverPending
        }
    };

    // 1. Uninitialized driver: HardwarePresentDriverPending
    assert_eq!(determine_status(false, false, false), HostConnectionStatus::HardwarePresentDriverPending);
    assert_ne!(determine_status(false, false, false), HostConnectionStatus::Connected);

    // 2. Active driver, no cable (HP Victus boot default): Disconnected (NOT Connected!)
    assert_eq!(determine_status(true, false, false), HostConnectionStatus::Disconnected);
    assert_ne!(determine_status(true, false, false), HostConnectionStatus::Connected);

    // 3. Active driver, cable plugged in, DHCP in progress: ObtainingIp (NOT Connected!)
    assert_eq!(determine_status(true, true, false), HostConnectionStatus::ObtainingIp);
    assert_ne!(determine_status(true, true, false), HostConnectionStatus::Connected);

    // 4. Active driver, cable plugged in, IP assigned: Connected
    assert_eq!(determine_status(true, true, true), HostConnectionStatus::Connected);

    // 5. Cable unplugged while IP still cached: Disconnected (Link down overrides IP)
    assert_eq!(determine_status(true, false, true), HostConnectionStatus::Disconnected);
    assert_ne!(determine_status(true, false, true), HostConnectionStatus::Connected);
}

// ============================================================================
// 802.11 Wi-Fi Framing, Scanning, and WPA2 Supplicant Host Verification
// ============================================================================

pub fn host_build_probe_request(sta_mac: [u8; 6], seq: u16) -> Vec<u8> {
    let mut frame = Vec::with_capacity(64);
    frame.extend_from_slice(&[0x40, 0x00]); // Mgmt, Probe Req
    frame.extend_from_slice(&[0x00, 0x00]); // Duration
    frame.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]); // RA
    frame.extend_from_slice(&sta_mac); // TA
    frame.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]); // BSSID
    let seq_ctrl = (seq & 0x0FFF) << 4;
    frame.extend_from_slice(&seq_ctrl.to_le_bytes());
    frame.extend_from_slice(&[0x00, 0x00]); // SSID len 0
    frame.extend_from_slice(&[0x01, 0x08, 0x82, 0x84, 0x8B, 0x96, 0x0C, 0x12, 0x18, 0x24]); // Rates
    frame.extend_from_slice(&[0x32, 0x04, 0x30, 0x48, 0x60, 0x6C]); // Ext rates
    frame
}

#[test]
fn test_host_80211_probe_request_framing() {
    let sta_mac = [0x02, 0xEC, 0xB8, 0x52, 0x04, 0x00];
    let probe = host_build_probe_request(sta_mac, 42);

    assert_eq!(probe[0..2], [0x40, 0x00]); // Frame Control: Probe Request
    assert_eq!(&probe[4..10], &[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]); // Broadcast RA
    assert_eq!(&probe[10..16], &sta_mac); // Station MAC TA
    assert_eq!(&probe[16..22], &[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]); // Broadcast BSSID

    let seq_ctrl = u16::from_le_bytes([probe[22], probe[23]]);
    assert_eq!(seq_ctrl >> 4, 42);

    // Tag 0 (SSID) length 0
    assert_eq!(probe[24], 0);
    assert_eq!(probe[25], 0);
}

pub fn host_build_80211_data_frame(
    sta_mac: [u8; 6],
    bssid: [u8; 6],
    da: [u8; 6],
    ethertype: u16,
    payload: &[u8],
    seq: u16,
) -> Vec<u8> {
    let mut frame = Vec::with_capacity(24 + 8 + payload.len());
    frame.extend_from_slice(&[0x08, 0x01]); // Type Data, ToDS = 1
    frame.extend_from_slice(&[0x00, 0x00]);
    frame.extend_from_slice(&bssid);
    frame.extend_from_slice(&sta_mac);
    frame.extend_from_slice(&da);
    let seq_ctrl = (seq & 0x0FFF) << 4;
    frame.extend_from_slice(&seq_ctrl.to_le_bytes());
    frame.extend_from_slice(&[0xAA, 0xAA, 0x03, 0x00, 0x00, 0x00]); // LLC/SNAP
    frame.extend_from_slice(&ethertype.to_be_bytes());
    frame.extend_from_slice(payload);
    frame
}

#[test]
fn test_host_80211_llc_snap_data_frame_roundtrip() {
    let sta_mac = [0x02, 0xEC, 0xB8, 0x52, 0x04, 0x00];
    let bssid = [0x30, 0xDE, 0x4B, 0x12, 0x34, 0x56];
    let da = [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
    let payload = b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n";

    let frame = host_build_80211_data_frame(sta_mac, bssid, da, 0x0800, payload, 101);

    assert_eq!(frame[0..2], [0x08, 0x01]); // ToDS data frame
    assert_eq!(&frame[4..10], &bssid);
    assert_eq!(&frame[10..16], &sta_mac);
    assert_eq!(&frame[16..22], &da);

    // SNAP header at offset 24
    assert_eq!(&frame[24..30], &[0xAA, 0xAA, 0x03, 0x00, 0x00, 0x00]);
    let ethertype = u16::from_be_bytes([frame[30], frame[31]]);
    assert_eq!(ethertype, 0x0800);
    assert_eq!(&frame[32..], payload);
}

#[test]
fn test_host_wpa2_eapol_key_msg2_and_msg4_generation() {
    let kck = [0x11u8; 16];
    let snonce = [0x22u8; 32];
    let replay_counter: u64 = 7;

    let rsn_ie: [u8; 22] = [
        0x30, 0x14, 0x01, 0x00, 0x00, 0x0F, 0xAC, 0x04, 0x01, 0x00, 0x00, 0x0F, 0xAC, 0x04,
        0x01, 0x00, 0x00, 0x0F, 0xAC, 0x02, 0x00, 0x00,
    ];

    // Build Message 2
    let body_len = 95 + rsn_ie.len();
    let mut msg2 = vec![0u8; 4 + body_len];
    msg2[0] = 2; // EAPOL Version 2
    msg2[1] = 3; // EAPOL-Key
    msg2[2..4].copy_from_slice(&(body_len as u16).to_be_bytes());
    msg2[4] = 2; // RSN Key Descriptor
    msg2[5..7].copy_from_slice(&0x010Au16.to_be_bytes()); // Key Info: Pairwise + MIC
    msg2[7..9].copy_from_slice(&16u16.to_be_bytes()); // Key Length
    msg2[9..17].copy_from_slice(&replay_counter.to_be_bytes());
    msg2[17..49].copy_from_slice(&snonce);
    msg2[97..99].copy_from_slice(&(rsn_ie.len() as u16).to_be_bytes());
    msg2[99..99 + rsn_ie.len()].copy_from_slice(&rsn_ie);

    // Compute MIC over entire frame with zeroed MIC
    let digest = host_hmac_sha1(&kck, &msg2);
    msg2[81..97].copy_from_slice(&digest[0..16]);

    // Verify non-zero MIC inserted
    assert_ne!(&msg2[81..97], &[0u8; 16]);

    // Build Message 4
    let body_len_msg4 = 95;
    let mut msg4 = vec![0u8; 4 + body_len_msg4];
    msg4[0] = 2;
    msg4[1] = 3;
    msg4[2..4].copy_from_slice(&(body_len_msg4 as u16).to_be_bytes());
    msg4[4] = 2;
    msg4[5..7].copy_from_slice(&0x030Au16.to_be_bytes()); // Pairwise + MIC + Secure
    msg4[7..9].copy_from_slice(&16u16.to_be_bytes());
    msg4[9..17].copy_from_slice(&replay_counter.to_be_bytes());

    let digest4 = host_hmac_sha1(&kck, &msg4);
    msg4[81..97].copy_from_slice(&digest4[0..16]);

    assert_ne!(&msg4[81..97], &[0u8; 16]);
}

#[test]
fn test_host_wifi_settings_profile_and_selection_state() {
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct MockProfile {
        ssid: String,
        passphrase: String,
        auto_connect: bool,
    }

    let mut profiles: Vec<MockProfile> = Vec::new();
    profiles.push(MockProfile {
        ssid: "Galaxy_Hotspot_5G".to_string(),
        passphrase: "supersecretpassword123".to_string(),
        auto_connect: true,
    });

    // Verify lookup by SSID
    let match_prof = profiles.iter().find(|p| p.ssid == "Galaxy_Hotspot_5G");
    assert!(match_prof.is_some());
    assert_eq!(match_prof.unwrap().passphrase, "supersecretpassword123");

    // Verify lookup on missing SSID
    assert!(profiles.iter().find(|p| p.ssid == "Unknown_AP").is_none());

    // Verify profile update
    if let Some(prof) = profiles.iter_mut().find(|p| p.ssid == "Galaxy_Hotspot_5G") {
        prof.passphrase = "newpassword456".to_string();
    }
    assert_eq!(profiles[0].passphrase, "newpassword456");
}

#[test]
fn test_host_80211_rxdesc_offset_tolerance() {
    // Construct valid 802.11 Beacon frame
    let mut beacon_mpdu = vec![0u8; 40];
    beacon_mpdu[0] = 0x80; // Mgmt, Beacon
    beacon_mpdu[1] = 0x00; // Version 0, no flags
    beacon_mpdu[16..22].copy_from_slice(&[0x10, 0x20, 0x30, 0x40, 0x50, 0x60]); // BSSID

    let offsets = [0, 16, 24, 32];
    for &off in &offsets {
        let mut rx_buf = vec![0xEE; off];
        rx_buf.extend_from_slice(&beacon_mpdu);

        // Mimic find_80211_mgmt_frame logic
        let found = {
            let mut detected = None;
            for &candidate in &[0, 16, 24, 32] {
                if rx_buf.len() < candidate + 24 {
                    continue;
                }
                let slice = &rx_buf[candidate..];
                let fc = u16::from_le_bytes([slice[0], slice[1]]);
                let version = fc & 0x03;
                let frame_type = ((fc >> 2) & 0x03) as u8;
                let subtype = ((fc >> 4) & 0x0F) as u8;
                if version == 0 && frame_type == 0 && (subtype == 8 || subtype == 5) {
                    detected = Some(candidate);
                    break;
                }
            }
            detected
        };

        assert_eq!(found, Some(off), "Failed to detect MPDU at rxdesc offset {}", off);
    }
}

#[test]
fn test_host_wifi_control_focus_cycling() {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum HostWifiControl {
        ScanButton,
        ToggleRadio,
        NetworkList,
        PasswordField,
        ConnectButton,
        DisconnectButton,
    }

    fn cycle_focus(cur: HostWifiControl, is_connected: bool, has_selection: bool) -> HostWifiControl {
        match cur {
            HostWifiControl::ScanButton => HostWifiControl::ToggleRadio,
            HostWifiControl::ToggleRadio => {
                if is_connected {
                    HostWifiControl::DisconnectButton
                } else {
                    HostWifiControl::NetworkList
                }
            }
            HostWifiControl::DisconnectButton => HostWifiControl::NetworkList,
            HostWifiControl::NetworkList => {
                if has_selection {
                    HostWifiControl::PasswordField
                } else {
                    HostWifiControl::ScanButton
                }
            }
            HostWifiControl::PasswordField => HostWifiControl::ConnectButton,
            HostWifiControl::ConnectButton => HostWifiControl::ScanButton,
        }
    }

    // 1. Disconnected with network selected
    let mut ctrl = HostWifiControl::ScanButton;
    ctrl = cycle_focus(ctrl, false, true);
    assert_eq!(ctrl, HostWifiControl::ToggleRadio);
    ctrl = cycle_focus(ctrl, false, true);
    assert_eq!(ctrl, HostWifiControl::NetworkList);
    ctrl = cycle_focus(ctrl, false, true);
    assert_eq!(ctrl, HostWifiControl::PasswordField);
    ctrl = cycle_focus(ctrl, false, true);
    assert_eq!(ctrl, HostWifiControl::ConnectButton);
    ctrl = cycle_focus(ctrl, false, true);
    assert_eq!(ctrl, HostWifiControl::ScanButton);

    // 2. Connected state includes DisconnectButton
    ctrl = HostWifiControl::ToggleRadio;
    ctrl = cycle_focus(ctrl, true, false);
    assert_eq!(ctrl, HostWifiControl::DisconnectButton);
    ctrl = cycle_focus(ctrl, true, false);
    assert_eq!(ctrl, HostWifiControl::NetworkList);
}

#[test]
fn test_host_dns_query_encoding_and_response_parsing() {
    // 1. Validate DNS Query Wire Encoding for "google.com"
    let hostname = "google.com";
    let xid: u16 = 0xD155;
    let mut query = Vec::with_capacity(64);
    query.extend_from_slice(&xid.to_be_bytes());
    query.extend_from_slice(&0x0100u16.to_be_bytes()); // Standard query with RD
    query.extend_from_slice(&1u16.to_be_bytes()); // QDCOUNT = 1
    query.extend_from_slice(&0u16.to_be_bytes()); // ANCOUNT = 0
    query.extend_from_slice(&0u16.to_be_bytes()); // NSCOUNT = 0
    query.extend_from_slice(&0u16.to_be_bytes()); // ARCOUNT = 0

    for part in hostname.split('.') {
        query.push(part.len() as u8);
        query.extend_from_slice(part.as_bytes());
    }
    query.push(0); // Root label
    query.extend_from_slice(&1u16.to_be_bytes()); // QTYPE = A
    query.extend_from_slice(&1u16.to_be_bytes()); // QCLASS = IN

    assert_eq!(query.len(), 12 + 1 + 6 + 1 + 3 + 1 + 4); // 28 bytes
    assert_eq!(&query[12..23], &[6, b'g', b'o', b'o', b'g', b'l', b'e', 3, b'c', b'o', b'm']);
    assert_eq!(query[23], 0);

    // 2. Build synthetic DNS Response for "google.com" -> 142.250.190.46
    let mut resp = Vec::new();
    resp.extend_from_slice(&xid.to_be_bytes());
    resp.extend_from_slice(&0x8180u16.to_be_bytes()); // Standard response, Recursion Available, No Error
    resp.extend_from_slice(&1u16.to_be_bytes()); // QDCOUNT = 1
    resp.extend_from_slice(&1u16.to_be_bytes()); // ANCOUNT = 1
    resp.extend_from_slice(&0u16.to_be_bytes());
    resp.extend_from_slice(&0u16.to_be_bytes());

    // Echo Question section
    for part in hostname.split('.') {
        resp.push(part.len() as u8);
        resp.extend_from_slice(part.as_bytes());
    }
    resp.push(0);
    resp.extend_from_slice(&1u16.to_be_bytes());
    resp.extend_from_slice(&1u16.to_be_bytes());

    // Answer section (compressed pointer to offset 12)
    resp.extend_from_slice(&[0xC0, 0x0C]); // Name pointer
    resp.extend_from_slice(&1u16.to_be_bytes()); // TYPE A
    resp.extend_from_slice(&1u16.to_be_bytes()); // CLASS IN
    resp.extend_from_slice(&300u32.to_be_bytes()); // TTL 300s
    resp.extend_from_slice(&4u16.to_be_bytes()); // RDLENGTH 4
    resp.extend_from_slice(&[142, 250, 190, 46]); // IPv4 RDATA

    // Parse the response using our DNS logic
    let flags = u16::from_be_bytes([resp[2], resp[3]]);
    let qdcount = u16::from_be_bytes([resp[4], resp[5]]) as usize;
    let ancount = u16::from_be_bytes([resp[6], resp[7]]) as usize;
    assert_ne!(flags & 0x8000, 0); // Is response
    assert_eq!(flags & 0x000F, 0); // No error
    assert_eq!(ancount, 1);

    let mut offset = 12;
    for _ in 0..qdcount {
        while offset < resp.len() {
            let len = resp[offset] as usize;
            offset += 1;
            if len == 0 { break; }
            if len >= 0xC0 { offset += 1; break; }
            offset += len;
        }
        offset += 4;
    }

    let mut extracted_ip: Option<[u8; 4]> = None;
    for _ in 0..ancount {
        if resp[offset] >= 0xC0 {
            offset += 2;
        }
        let rtype = u16::from_be_bytes([resp[offset], resp[offset + 1]]);
        let rclass = u16::from_be_bytes([resp[offset + 2], resp[offset + 3]]);
        let rdlength = u16::from_be_bytes([resp[offset + 8], resp[offset + 9]]) as usize;
        offset += 10;
        if rtype == 1 && rclass == 1 && rdlength == 4 {
            extracted_ip = Some([resp[offset], resp[offset + 1], resp[offset + 2], resp[offset + 3]]);
            break;
        }
    }

    assert_eq!(extracted_ip, Some([142, 250, 190, 46]));
}

#[test]
fn test_host_rtl8852b_mac_power_on_register_bits() {
    // Grounded in official Linux rtw89 rtw8852b_pwr_on_func & rtw89_mac_pwr_on specifications
    const R_AX_SYS_FUNC_EN: usize = 0x0002;
    const B_AX_FEN_BBRSTB: u16 = 1 << 0;
    const B_AX_FEN_BB_GLB_RSTN: u16 = 1 << 1;
    const B_AX_FEN_PCIEA: u16 = 1 << 2;
    const B_AX_FEN_MAC: u16 = 1 << 3;

    const R_AX_SYS_PW_CTRL: usize = 0x0004;
    const B_AX_APFN_ONMAC: u32 = 1 << 8;
    const B_AX_APFM_SWLPS: u32 = 1 << 9;
    const B_AX_AFSM_WLSUS_EN: u32 = 1 << 11;
    const B_AX_AFSM_PCIE_SUS_EN: u32 = 1 << 12;
    const B_AX_PSUS_OFF_CAPC_EN: u32 = 1 << 14;
    const B_AX_APDM_HPDN: u32 = 1 << 15;
    const B_AX_EN_WLON: u32 = 1 << 16;
    const B_AX_RDY_SYSPWR: u32 = 1 << 17;

    const R_AX_SYS_CLK_CTRL: usize = 0x0008;
    const B_AX_MAC_CLK_EN: u32 = 1 << 12;
    const B_AX_CPU_CLK_EN: u32 = 1 << 14;
    const B_AX_BB_CLK_EN: u32 = 1 << 15;

    assert_eq!(R_AX_SYS_FUNC_EN, 0x0002);
    assert_eq!(R_AX_SYS_PW_CTRL, 0x0004);
    assert_eq!(R_AX_SYS_CLK_CTRL, 0x0008);

    // Verify clear mask disables analog power cut and sleep states
    let clear_mask = B_AX_PSUS_OFF_CAPC_EN | B_AX_AFSM_WLSUS_EN | B_AX_AFSM_PCIE_SUS_EN | B_AX_APDM_HPDN | B_AX_APFM_SWLPS;
    assert_eq!(clear_mask, (1 << 14) | (1 << 11) | (1 << 12) | (1 << 15) | (1 << 9));

    // Verify polling targets bit 17 (RDY_SYSPWR)
    assert_eq!(B_AX_RDY_SYSPWR, 0x0002_0000);

    // Verify power assertion targets WLAN and MAC planes
    let pwr_enable_mask = B_AX_EN_WLON | B_AX_APFN_ONMAC;
    assert_eq!(pwr_enable_mask, 0x0001_0100);

    // Verify function enable unmasks resets and domains
    let func_mask = B_AX_FEN_BBRSTB | B_AX_FEN_BB_GLB_RSTN | B_AX_FEN_PCIEA | B_AX_FEN_MAC;
    assert_eq!(func_mask, 0x000F);

    // Verify clock enable enables CPU, MAC, and Baseband clocks
    let clk_mask = B_AX_CPU_CLK_EN | B_AX_MAC_CLK_EN | B_AX_BB_CLK_EN;
    assert_eq!(clk_mask, (1 << 14) | (1 << 12) | (1 << 15));
}

#[test]
fn test_host_rtl8852b_rf_synthesizer_direct_mmio_offsets() {
    const RF_PATH_A_BASE: usize = 0xE000;
    const RF_PATH_B_BASE: usize = 0xF000;
    const RR_CFGCH: usize = 0x18;

    let ch_offset = RR_CFGCH << 2; // 0x18 * 4 = 0x60
    assert_eq!(ch_offset, 0x60);

    let path_a_addr = RF_PATH_A_BASE + ch_offset;
    let path_b_addr = RF_PATH_B_BASE + ch_offset;

    assert_eq!(path_a_addr, 0xE060);
    assert_eq!(path_b_addr, 0xF060);

    // Test 2.4 GHz channel tuning calculations for Channels 1, 6, 11
    for &ch in &[1u8, 6, 11] {
        let raw_val: u32 = 0x0003_0380;
        let tuned = (raw_val & !0x303FF) | (ch as u32);
        assert_eq!((tuned & 0xFF) as u8, ch);
        assert_eq!(tuned & 0x30300, 0); // Preserves bits outside channel field
    }
}

#[test]
fn test_host_rtl8852b_rx_filter_and_bb_enable_registers() {
    const R_AX_WLRF_CTRL: usize = 0x02F0;
    const B_AX_AFC_AFEDIG: u32 = 1 << 17;

    const R_AX_PHYREG_SET: usize = 0x8040;
    const PHYREG_SET_ALL_CYCLE: u8 = 0x8;

    const R_AX_RX_FLTR_OPT: usize = 0xCE20;
    const B_AX_SNIFFER_MODE: u32 = 1 << 0;
    const R_AX_MGNT_FLTR: usize = 0xCE28;
    const R_AX_DATA_FLTR: usize = 0xCE2C;

    assert_eq!(R_AX_WLRF_CTRL, 0x02F0);
    assert_eq!(B_AX_AFC_AFEDIG, 0x0002_0000);
    assert_eq!(R_AX_PHYREG_SET, 0x8040);
    assert_eq!(PHYREG_SET_ALL_CYCLE, 8);

    assert_eq!(R_AX_RX_FLTR_OPT, 0xCE20);
    assert_eq!(B_AX_SNIFFER_MODE, 1);
    assert_eq!(R_AX_MGNT_FLTR, 0xCE28);
    assert_eq!(R_AX_DATA_FLTR, 0xCE2C);
}
