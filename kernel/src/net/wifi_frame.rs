//! IEEE 802.11 Management Frame Dissector and Scan Processing.
//!
//! Parses live Beacon and Probe Response frames received over the air by the
//! Realtek RTL8852BE radio. Extracts genuine SSID, BSSID, Channel, RSSI,
//! and RSN/WPA2/WPA3 cryptographic capability suites.
//!
//! STRICT RULE: Zero simulated networks, zero mock SSIDs. Only frames received
//! by physical hardware silicon are parsed and presented.

#![allow(dead_code)]

use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

pub const FC_TYPE_MGMT: u8 = 0;
pub const FC_TYPE_CTRL: u8 = 1;
pub const FC_TYPE_DATA: u8 = 2;

pub const FC_SUBTYPE_BEACON: u8 = 8;
pub const FC_SUBTYPE_PROBE_RESP: u8 = 5;
pub const FC_SUBTYPE_AUTH: u8 = 11;
pub const FC_SUBTYPE_ASSOC_RESP: u8 = 1;
pub const FC_SUBTYPE_DEAUTH: u8 = 12;

pub const IE_TAG_SSID: u8 = 0;
pub const IE_TAG_SUPPORTED_RATES: u8 = 1;
pub const IE_TAG_DS_PARAMS: u8 = 3;
pub const IE_TAG_RSN: u8 = 48; // WPA2 / 802.11i
pub const IE_TAG_HT_OPERATION: u8 = 61;
pub const IE_TAG_VENDOR_SPECIFIC: u8 = 221;

// OUI Constants for 802.11i / WPA
pub const OUI_WPA2: [u8; 3] = [0x00, 0x0F, 0xAC];
pub const OUI_WPA1: [u8; 4] = [0x00, 0x50, 0xF2, 0x01];

pub const CIPHER_CCMP: u8 = 4;
pub const CIPHER_TKIP: u8 = 2;
pub const AKM_PSK: u8 = 2;
pub const AKM_SAE_WPA3: u8 = 8;

/// Security protocol suite advertised by an Access Point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WifiSecurity {
    Open,
    Wpa2Personal,
    Wpa3Personal,
    WpaPersonalDual, // WPA2/WPA3 transition mode
    WpaEnterprise,
    Wep,
}

impl WifiSecurity {
    pub fn display_name(&self) -> &'static str {
        match self {
            WifiSecurity::Open => "Open (Unsecured)",
            WifiSecurity::Wpa2Personal => "WPA2-Personal (AES-CCMP)",
            WifiSecurity::Wpa3Personal => "WPA3-Personal (SAE)",
            WifiSecurity::WpaPersonalDual => "WPA2/WPA3 Personal",
            WifiSecurity::WpaEnterprise => "WPA2-Enterprise (802.1X)",
            WifiSecurity::Wep => "WEP (Insecure)",
        }
    }
}

/// Discovered physical Wi-Fi Access Point network telemetry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredNetwork {
    pub ssid: String,
    pub bssid: [u8; 6],
    pub channel: u8,
    pub rssi_dbm: i8,
    pub security: WifiSecurity,
    pub signal_bars: u8,
    pub is_5ghz: bool,
    pub beacon_interval: u16,
    pub capabilities: u16,
}

impl DiscoveredNetwork {
    pub fn signal_percentage(&self) -> u8 {
        if self.rssi_dbm >= -50 {
            100
        } else if self.rssi_dbm <= -100 {
            0
        } else {
            (2 * (self.rssi_dbm + 100)) as u8
        }
    }
}

/// Computes signal bar rating (1 to 4 bars) from dBm signal level.
pub fn calculate_signal_bars(rssi_dbm: i8) -> u8 {
    if rssi_dbm >= -60 {
        4
    } else if rssi_dbm >= -70 {
        3
    } else if rssi_dbm >= -80 {
        2
    } else {
        1
    }
}

/// Converts raw hardware RSSI byte from RTL8852B RX descriptor into dBm.
pub fn rtl8852b_raw_to_rssi_dbm(raw: u8) -> i8 {
    // rtw89 formula: dBm = (raw / 2) - 110
    let dbm = (raw as i16 / 2) - 110;
    dbm.clamp(-110, -20) as i8
}

/// Helper to locate the 802.11 Management MPDU start within an RX buffer that may contain
/// a hardware rxdesc header of 0, 16, 24, or 32 bytes.
pub fn find_80211_mgmt_frame<'a>(buf: &'a [u8], expected_subtypes: &[u8]) -> Option<&'a [u8]> {
    let offsets = [0, 16, 24, 32];
    for &off in &offsets {
        if buf.len() < off + 24 {
            continue;
        }
        let slice = &buf[off..];
        let fc = u16::from_le_bytes([slice[0], slice[1]]);
        let version = fc & 0x03;
        let frame_type = ((fc >> 2) & 0x03) as u8;
        let subtype = ((fc >> 4) & 0x0F) as u8;

        if version == 0 && frame_type == FC_TYPE_MGMT && expected_subtypes.contains(&subtype) {
            return Some(slice);
        }
    }
    None
}

/// Helper to locate an 802.11 Data MPDU with LLC/SNAP header within an RX buffer.
pub fn find_80211_data_frame<'a>(buf: &'a [u8]) -> Option<&'a [u8]> {
    let offsets = [0, 16, 24, 32];
    for &off in &offsets {
        if buf.len() < off + 24 + 8 {
            continue;
        }
        let slice = &buf[off..];
        let fc = u16::from_le_bytes([slice[0], slice[1]]);
        let version = fc & 0x03;
        let frame_type = ((fc >> 2) & 0x03) as u8;

        if version == 0 && frame_type == FC_TYPE_DATA {
            // Standard LLC/SNAP header at offset 24
            if slice.len() >= 32 && slice[24..30] == [0xAA, 0xAA, 0x03, 0x00, 0x00, 0x00] {
                return Some(slice);
            }
        }
    }
    None
}

/// Parses an 802.11 MPDU (starting after any 0, 16, 24, or 32 byte rxdesc header).
pub fn parse_80211_frame(frame: &[u8], rx_rssi_dbm: i8) -> Option<DiscoveredNetwork> {
    let data = find_80211_mgmt_frame(frame, &[FC_SUBTYPE_BEACON, FC_SUBTYPE_PROBE_RESP])?;

    if data.len() < 24 + 12 {
        return None; // Minimum 802.11 Mgmt Header (24) + Fixed Params (12)
    }

    let fc = u16::from_le_bytes([data[0], data[1]]);
    let frame_type = ((fc >> 2) & 0x03) as u8;
    let subtype = ((fc >> 4) & 0x0F) as u8;

    if frame_type != FC_TYPE_MGMT {
        return None;
    }

    if subtype != FC_SUBTYPE_BEACON && subtype != FC_SUBTYPE_PROBE_RESP {
        return None;
    }

    let mut bssid = [0u8; 6];
    bssid.copy_from_slice(&data[16..22]); // Address 3 is BSSID

    // Fixed parameters: 8 bytes timestamp + 2 bytes beacon interval + 2 bytes capability info
    let beacon_interval = u16::from_le_bytes([data[32], data[33]]);
    let capabilities = u16::from_le_bytes([data[34], data[35]]);
    let privacy = (capabilities & (1 << 4)) != 0;

    // Parse Tagged Information Elements (starting at byte 36)
    let mut offset = 36usize;
    let mut ssid_str = String::new();
    let mut channel = 1u8;
    let mut has_rsn = false;
    let mut has_wpa1 = false;
    let mut has_wpa3 = false;

    while offset + 2 <= data.len() {
        let tag = data[offset];
        let len = data[offset + 1] as usize;
        let val_start = offset + 2;
        let val_end = val_start + len;

        if val_end > data.len() {
            break; // Truncated IE
        }

        let val = &data[val_start..val_end];

        match tag {
            IE_TAG_SSID => {
                if let Ok(s) = core::str::from_utf8(val) {
                    if !s.is_empty() {
                        ssid_str = String::from(s);
                    }
                }
            }
            IE_TAG_DS_PARAMS => {
                if !val.is_empty() {
                    channel = val[0];
                }
            }
            IE_TAG_HT_OPERATION => {
                if !val.is_empty() && channel == 1 {
                    channel = val[0];
                }
            }
            IE_TAG_RSN => {
                has_rsn = true;
                // Parse AKM suites inside RSN IE
                if val.len() >= 8 {
                    // Check for WPA3 SAE AKM suite (00-0F-AC-08)
                    for chunk in val.windows(4) {
                        if chunk == [0x00, 0x0F, 0xAC, 0x08] {
                            has_wpa3 = true;
                        }
                    }
                }
            }
            IE_TAG_VENDOR_SPECIFIC => {
                if val.len() >= 4 && val[0..4] == OUI_WPA1 {
                    has_wpa1 = true;
                }
            }
            _ => {}
        }

        offset = val_end;
    }

    if ssid_str.is_empty() {
        ssid_str = String::from("<Hidden Network>");
    }

    let security = if has_rsn && has_wpa3 {
        WifiSecurity::WpaPersonalDual
    } else if has_rsn {
        WifiSecurity::Wpa2Personal
    } else if has_wpa1 {
        WifiSecurity::Wpa2Personal // Treat legacy WPA1 as PSK
    } else if privacy {
        WifiSecurity::Wep
    } else {
        WifiSecurity::Open
    };

    let is_5ghz = channel > 14;
    let signal_bars = calculate_signal_bars(rx_rssi_dbm);

    Some(DiscoveredNetwork {
        ssid: ssid_str,
        bssid,
        channel,
        rssi_dbm: rx_rssi_dbm,
        security,
        signal_bars,
        is_5ghz,
        beacon_interval,
        capabilities,
    })
}

/// Global registry of verified over-the-air discovered Wi-Fi networks.
pub struct WifiScanTable {
    pub networks: Vec<DiscoveredNetwork>,
}

impl WifiScanTable {
    pub const fn new() -> Self {
        Self {
            networks: Vec::new(),
        }
    }

    /// Updates or inserts a discovered network based on its unique BSSID.
    pub fn update_network(&mut self, net: DiscoveredNetwork) {
        if let Some(existing) = self.networks.iter_mut().find(|n| n.bssid == net.bssid) {
            existing.rssi_dbm = net.rssi_dbm;
            existing.signal_bars = net.signal_bars;
            existing.channel = net.channel;
            if existing.ssid == "<Hidden Network>" && net.ssid != "<Hidden Network>" {
                existing.ssid = net.ssid;
            }
        } else {
            self.networks.push(net);
        }
    }

    /// Clears expired entries during a new active scan cycle.
    pub fn clear(&mut self) {
        self.networks.clear();
    }
}

/// Global spinlock-protected scanned Wi-Fi network table.
pub static WIFI_SCAN_TABLE: Mutex<WifiScanTable> = Mutex::new(WifiScanTable::new());

// ============================================================================
// 802.11 Frame Generation & Parsing Helpers
// ============================================================================

/// Constructs an active 802.11 Probe Request frame (wildcard SSID broadcast).
pub fn build_probe_request(sta_mac: [u8; 6], seq: u16) -> Vec<u8> {
    let mut frame = Vec::with_capacity(64);
    // Frame Control: Type = 0 (Mgmt), Subtype = 4 (Probe Req) -> 0x0040
    frame.extend_from_slice(&[0x40, 0x00]);
    frame.extend_from_slice(&[0x00, 0x00]); // Duration
    frame.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]); // RA: Broadcast
    frame.extend_from_slice(&sta_mac); // TA: Station MAC
    frame.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]); // BSSID: Broadcast
    let seq_ctrl = (seq & 0x0FFF) << 4;
    frame.extend_from_slice(&seq_ctrl.to_le_bytes());

    // IE 0: Wildcard SSID (len 0)
    frame.extend_from_slice(&[0x00, 0x00]);
    // IE 1: Supported Rates (1, 2, 5.5, 11, 6, 9, 12, 18 Mbps)
    frame.extend_from_slice(&[0x01, 0x08, 0x82, 0x84, 0x8B, 0x96, 0x0C, 0x12, 0x18, 0x24]);
    // IE 50: Extended Supported Rates (24, 36, 48, 54 Mbps)
    frame.extend_from_slice(&[0x32, 0x04, 0x30, 0x48, 0x60, 0x6C]);
    // IE 45: HT Capabilities (26 bytes)
    frame.extend_from_slice(&[
        0x2D, 0x1A, 0x6E, 0x01, 0x1B, 0xFF, 0xFF, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ]);

    frame
}

/// Constructs an 802.11 Open System Authentication frame (Seq 1).
pub fn build_auth_frame(sta_mac: [u8; 6], bssid: [u8; 6], seq: u16, trans_seq: u16) -> Vec<u8> {
    let mut frame = Vec::with_capacity(32);
    // Frame Control: Type = 0 (Mgmt), Subtype = 11 (Auth) -> 0x00B0
    frame.extend_from_slice(&[0xB0, 0x00]);
    frame.extend_from_slice(&[0x3A, 0x01]); // Duration (314 us)
    frame.extend_from_slice(&bssid); // RA: AP BSSID
    frame.extend_from_slice(&sta_mac); // TA: Station MAC
    frame.extend_from_slice(&bssid); // BSSID
    let seq_ctrl = (seq & 0x0FFF) << 4;
    frame.extend_from_slice(&seq_ctrl.to_le_bytes());

    // Fixed Parameters:
    frame.extend_from_slice(&[0x00, 0x00]); // Auth Algorithm: 0 (Open System)
    frame.extend_from_slice(&trans_seq.to_le_bytes()); // Auth Transaction Sequence
    frame.extend_from_slice(&[0x00, 0x00]); // Status Code: 0 (Success)

    frame
}

/// Standard WPA2 / 802.11i RSN Information Element for Association Request and EAPOL Message 2.
pub fn build_rsn_ie() -> [u8; 22] {
    [
        0x30, 0x14, // Tag 48 (RSN), length 20
        0x01, 0x00, // Version 1
        0x00, 0x0F, 0xAC, 0x04, // Group Cipher: AES-CCMP
        0x01, 0x00, // Pairwise Cipher Count: 1
        0x00, 0x0F, 0xAC, 0x04, // Pairwise Cipher: AES-CCMP
        0x01, 0x00, // AKM Suite Count: 1
        0x00, 0x0F, 0xAC, 0x02, // AKM Suite: PSK (00-0F-AC-02)
        0x00, 0x00, // RSN Capabilities
    ]
}

/// Constructs an 802.11 Association Request frame.
pub fn build_assoc_request(
    sta_mac: [u8; 6],
    bssid: [u8; 6],
    ssid: &str,
    is_wpa2: bool,
    seq: u16,
) -> Vec<u8> {
    let mut frame = Vec::with_capacity(128);
    // Frame Control: Type = 0 (Mgmt), Subtype = 0 (Assoc Req) -> 0x0000
    frame.extend_from_slice(&[0x00, 0x00]);
    frame.extend_from_slice(&[0x3A, 0x01]); // Duration (314 us)
    frame.extend_from_slice(&bssid); // RA: AP BSSID
    frame.extend_from_slice(&sta_mac); // TA: Station MAC
    frame.extend_from_slice(&bssid); // BSSID
    let seq_ctrl = (seq & 0x0FFF) << 4;
    frame.extend_from_slice(&seq_ctrl.to_le_bytes());

    // Fixed Parameters:
    frame.extend_from_slice(&[0x31, 0x04]); // Capability Info (ESS, Short Preamble, Short Slot Time)
    frame.extend_from_slice(&[0x0A, 0x00]); // Listen Interval: 10

    // Tagged Information Elements:
    // IE 0: SSID
    let ssid_bytes = ssid.as_bytes();
    frame.push(0);
    frame.push(ssid_bytes.len() as u8);
    frame.extend_from_slice(ssid_bytes);

    // IE 1: Supported Rates
    frame.extend_from_slice(&[0x01, 0x08, 0x82, 0x84, 0x8B, 0x96, 0x0C, 0x12, 0x18, 0x24]);
    // IE 50: Extended Supported Rates
    frame.extend_from_slice(&[0x32, 0x04, 0x30, 0x48, 0x60, 0x6C]);

    if is_wpa2 {
        let rsn_ie = build_rsn_ie();
        frame.extend_from_slice(&rsn_ie);
    }

    frame
}

/// Constructs an 802.11 Deauthentication frame.
pub fn build_deauth_frame(sta_mac: [u8; 6], bssid: [u8; 6], reason: u16, seq: u16) -> Vec<u8> {
    let mut frame = Vec::with_capacity(26);
    // Frame Control: Type = 0 (Mgmt), Subtype = 12 (Deauth) -> 0x00C0
    frame.extend_from_slice(&[0xC0, 0x00]);
    frame.extend_from_slice(&[0x00, 0x00]);
    frame.extend_from_slice(&bssid);
    frame.extend_from_slice(&sta_mac);
    frame.extend_from_slice(&bssid);
    let seq_ctrl = (seq & 0x0FFF) << 4;
    frame.extend_from_slice(&seq_ctrl.to_le_bytes());
    frame.extend_from_slice(&reason.to_le_bytes());
    frame
}

/// Constructs an 802.11 Data frame with standard LLC/SNAP header.
pub fn build_80211_data_frame(
    sta_mac: [u8; 6],
    bssid: [u8; 6],
    da: [u8; 6],
    ethertype: u16,
    payload: &[u8],
    seq: u16,
) -> Vec<u8> {
    let mut frame = Vec::with_capacity(24 + 8 + payload.len());
    // Frame Control: Type = 2 (Data), Subtype = 0, ToDS = 1, FromDS = 0 -> 0x0108
    frame.extend_from_slice(&[0x08, 0x01]);
    frame.extend_from_slice(&[0x00, 0x00]); // Duration
    frame.extend_from_slice(&bssid); // RA / BSSID
    frame.extend_from_slice(&sta_mac); // TA / SA
    frame.extend_from_slice(&da); // DA
    let seq_ctrl = (seq & 0x0FFF) << 4;
    frame.extend_from_slice(&seq_ctrl.to_le_bytes());

    // LLC/SNAP Header (8 bytes): AA AA 03 00 00 00 [EtherType]
    frame.extend_from_slice(&[0xAA, 0xAA, 0x03, 0x00, 0x00, 0x00]);
    frame.extend_from_slice(&ethertype.to_be_bytes());
    frame.extend_from_slice(payload);

    frame
}

/// Parsed fields of an incoming 802.11 Data frame.
pub struct Parsed80211DataFrame<'a> {
    pub da: [u8; 6],
    pub sa: [u8; 6],
    pub bssid: [u8; 6],
    pub ethertype: u16,
    pub payload: &'a [u8],
}

/// Parses an incoming 802.11 Data frame and extracts DA, SA, EtherType, and payload.
pub fn parse_80211_data_frame(frame: &[u8]) -> Option<Parsed80211DataFrame<'_>> {
    let data = find_80211_data_frame(frame)?;

    if data.len() < 24 + 8 {
        return None;
    }

    let fc = u16::from_le_bytes([data[0], data[1]]);
    let frame_type = ((fc >> 2) & 0x03) as u8;
    if frame_type != FC_TYPE_DATA {
        return None;
    }

    let to_ds = (fc & 0x0100) != 0;
    let from_ds = (fc & 0x0200) != 0;

    let (da, sa, bssid) = match (to_ds, from_ds) {
        (false, true) => {
            // From AP to Station: Addr1 = DA (STA), Addr2 = BSSID (AP), Addr3 = SA (Source)
            let mut da = [0u8; 6];
            let mut bssid = [0u8; 6];
            let mut sa = [0u8; 6];
            da.copy_from_slice(&data[4..10]);
            bssid.copy_from_slice(&data[10..16]);
            sa.copy_from_slice(&data[16..22]);
            (da, sa, bssid)
        }
        (true, false) => {
            // From Station to AP: Addr1 = BSSID, Addr2 = SA (STA), Addr3 = DA
            let mut bssid = [0u8; 6];
            let mut sa = [0u8; 6];
            let mut da = [0u8; 6];
            bssid.copy_from_slice(&data[4..10]);
            sa.copy_from_slice(&data[10..16]);
            da.copy_from_slice(&data[16..22]);
            (da, sa, bssid)
        }
        _ => return None,
    };

    let snap_offset = 24;
    if data[snap_offset..snap_offset + 6] != [0xAA, 0xAA, 0x03, 0x00, 0x00, 0x00] {
        return None;
    }

    let ethertype = u16::from_be_bytes([data[snap_offset + 6], data[snap_offset + 7]]);
    let payload = &data[snap_offset + 8..];

    Some(Parsed80211DataFrame {
        da,
        sa,
        bssid,
        ethertype,
        payload,
    })
}

/// Parses an 802.11 Authentication response frame.
pub fn parse_auth_response(frame: &[u8]) -> Option<(u16, u16)> {
    let data = find_80211_mgmt_frame(frame, &[FC_SUBTYPE_AUTH])?;

    if data.len() < 24 + 6 {
        return None;
    }
    let fc = u16::from_le_bytes([data[0], data[1]]);
    let frame_type = ((fc >> 2) & 0x03) as u8;
    let subtype = ((fc >> 4) & 0x0F) as u8;
    if frame_type != FC_TYPE_MGMT || subtype != FC_SUBTYPE_AUTH {
        return None;
    }

    let trans_seq = u16::from_le_bytes([data[26], data[27]]);
    let status = u16::from_le_bytes([data[28], data[29]]);
    Some((trans_seq, status))
}

/// Parses an 802.11 Association response frame.
pub fn parse_assoc_response(frame: &[u8]) -> Option<(u16, u16)> {
    let data = find_80211_mgmt_frame(frame, &[FC_SUBTYPE_ASSOC_RESP])?;

    if data.len() < 24 + 6 {
        return None;
    }
    let fc = u16::from_le_bytes([data[0], data[1]]);
    let frame_type = ((fc >> 2) & 0x03) as u8;
    let subtype = ((fc >> 4) & 0x0F) as u8;
    if frame_type != FC_TYPE_MGMT || subtype != FC_SUBTYPE_ASSOC_RESP {
        return None;
    }

    let status = u16::from_le_bytes([data[26], data[27]]);
    let aid = u16::from_le_bytes([data[28], data[29]]) & 0x3FFF;
    Some((status, aid))
}
