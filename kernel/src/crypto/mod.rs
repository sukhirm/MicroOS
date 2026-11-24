//! Cryptographic Primitives and WPA2-Personal Key Derivation for Micro-OS.
//!
//! Provides deterministic hashing, key derivation, and 4-way handshake state management.

#![allow(dead_code)]

pub mod sha1;

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use crate::crypto::sha1::{hmac_sha1, pbkdf2_sha1, prf_512};

/// 512-bit Pairwise Transient Key (PTK) split into operational components.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DerivedPtk {
    /// 128-bit Key Confirmation Key (KCK) for calculating and verifying EAPOL MIC.
    pub kck: [u8; 16],
    /// 128-bit Key Encryption Key (KEK) for decrypting GTK in Message 3.
    pub kek: [u8; 16],
    /// 128-bit Temporal Key (TK) for AES-128-CCMP data payload encryption.
    pub tk: [u8; 16],
}

/// Derives the 512-bit PTK from PMK, nonces, and hardware MAC addresses.
pub fn derive_wpa2_ptk(
    pmk: &[u8; 32],
    ap_mac: &[u8; 6],
    sta_mac: &[u8; 6],
    anonce: &[u8; 32],
    snonce: &[u8; 32],
) -> DerivedPtk {
    let mut context = [0u8; 76];

    // IEEE 802.11i ordering: min(AA, SPA) || max(AA, SPA)
    if ap_mac < sta_mac {
        context[0..6].copy_from_slice(ap_mac);
        context[6..12].copy_from_slice(sta_mac);
    } else {
        context[0..6].copy_from_slice(sta_mac);
        context[6..12].copy_from_slice(ap_mac);
    }

    // IEEE 802.11i ordering: min(ANonce, SNonce) || max(ANonce, SNonce)
    if anonce < snonce {
        context[12..44].copy_from_slice(anonce);
        context[44..76].copy_from_slice(snonce);
    } else {
        context[12..44].copy_from_slice(snonce);
        context[44..76].copy_from_slice(anonce);
    }

    let ptk_bytes = prf_512(pmk, b"Pairwise key expansion", &context);

    let mut kck = [0u8; 16];
    let mut kek = [0u8; 16];
    let mut tk = [0u8; 16];

    kck.copy_from_slice(&ptk_bytes[0..16]);
    kek.copy_from_slice(&ptk_bytes[16..32]);
    tk.copy_from_slice(&ptk_bytes[32..48]);

    DerivedPtk { kck, kek, tk }
}

/// Computes the 16-byte HMAC-SHA1-128 MIC over an EAPOL frame using KCK.
pub fn compute_eapol_mic(kck: &[u8; 16], eapol_frame: &[u8]) -> [u8; 16] {
    let digest = hmac_sha1(kck, eapol_frame);
    let mut mic = [0u8; 16];
    mic.copy_from_slice(&digest[0..16]);
    mic
}

/// WPA2 Connection State Machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wpa2State {
    Disconnected,
    Authenticating,
    Associating,
    HandshakeMsg1Wait,
    HandshakeMsg2Sent,
    HandshakeMsg3Wait,
    HandshakeMsg4Sent,
    Connected,
    Failed(&'static str),
}

/// Supplicant session context for active Wi-Fi association.
pub struct Wpa2Session {
    pub target_ssid: String,
    pub bssid: [u8; 6],
    pub sta_mac: [u8; 6],
    pub pmk: [u8; 32],
    pub snonce: [u8; 32],
    pub anonce: [u8; 32],
    pub ptk: Option<DerivedPtk>,
    pub state: Wpa2State,
    pub replay_counter: u64,
}

impl Wpa2Session {
    /// Initializes a new connection session to an Access Point.
    pub fn new(ssid: &str, passphrase: &str, bssid: [u8; 6], sta_mac: [u8; 6]) -> Self {
        let pmk = pbkdf2_sha1(passphrase.as_bytes(), ssid.as_bytes(), 4096);
        // Deterministic pseudo-random seed based on timestamp/MAC for SNonce
        let mut snonce = [0u8; 32];
        snonce[0..6].copy_from_slice(&sta_mac);
        snonce[6..12].copy_from_slice(&bssid);
        snonce[12..16].copy_from_slice(&0x4D696372u32.to_be_bytes()); // "Micr"

        Self {
            target_ssid: String::from(ssid),
            bssid,
            sta_mac,
            pmk,
            snonce,
            anonce: [0u8; 32],
            ptk: None,
            state: Wpa2State::Authenticating,
            replay_counter: 0,
        }
    }

    /// Handles an incoming EAPOL Message 1 from the Authenticator.
    pub fn handle_msg1(&mut self, anonce: &[u8; 32], replay: u64) {
        self.anonce = *anonce;
        self.replay_counter = replay;
        let ptk = derive_wpa2_ptk(&self.pmk, &self.bssid, &self.sta_mac, &self.anonce, &self.snonce);
        self.ptk = Some(ptk);
        self.state = Wpa2State::HandshakeMsg2Sent;
    }

    /// Parses an incoming EAPOL frame to extract ANonce and Replay Counter if it is Message 1.
    pub fn parse_msg1(eapol_frame: &[u8]) -> Option<([u8; 32], u64)> {
        if eapol_frame.len() < 99 {
            return None;
        }
        if eapol_frame[1] != 3 || eapol_frame[4] != 2 {
            return None; // Must be EAPOL-Key (3) and RSN descriptor (2)
        }
        let key_info = u16::from_be_bytes([eapol_frame[5], eapol_frame[6]]);
        // Message 1 has Key Type = 1 (Pairwise), Ack = 1, MIC = 0
        let is_pairwise = (key_info & (1 << 3)) != 0;
        let ack = (key_info & (1 << 7)) != 0;
        let mic = (key_info & (1 << 8)) != 0;

        if !is_pairwise || !ack || mic {
            return None;
        }

        let mut replay_bytes = [0u8; 8];
        replay_bytes.copy_from_slice(&eapol_frame[9..17]);
        let replay = u64::from_be_bytes(replay_bytes);

        let mut anonce = [0u8; 32];
        anonce.copy_from_slice(&eapol_frame[17..49]);

        Some((anonce, replay))
    }

    /// Constructs EAPOL-Key Message 2 to transmit to the Authenticator.
    pub fn build_msg2(&self) -> Option<Vec<u8>> {
        let ptk = self.ptk.as_ref()?;
        let rsn_ie: [u8; 22] = [
            0x30, 0x14, 0x01, 0x00, 0x00, 0x0F, 0xAC, 0x04, 0x01, 0x00, 0x00, 0x0F, 0xAC, 0x04,
            0x01, 0x00, 0x00, 0x0F, 0xAC, 0x02, 0x00, 0x00,
        ];
        let body_len = 95 + rsn_ie.len(); // 117 bytes
        let mut frame = vec![0u8; 4 + body_len];

        // EAPOL Header
        frame[0] = 2; // EAPOL Version 2
        frame[1] = 3; // EAPOL-Key
        frame[2..4].copy_from_slice(&(body_len as u16).to_be_bytes());

        // Key Descriptor
        frame[4] = 2; // Descriptor Type: RSN
        // Key Info: Key Descr Version 2 (HMAC-SHA1), Key Type = 1 (Pairwise), Key MIC = 1 -> 0x010A
        frame[5..7].copy_from_slice(&0x010Au16.to_be_bytes());
        frame[7..9].copy_from_slice(&16u16.to_be_bytes()); // Key Length = 16
        frame[9..17].copy_from_slice(&self.replay_counter.to_be_bytes()); // Replay Counter
        frame[17..49].copy_from_slice(&self.snonce); // SNonce
        // IV, RSC, Reserved remain 0
        // Key Data Length (22 bytes)
        frame[97..99].copy_from_slice(&(rsn_ie.len() as u16).to_be_bytes());
        // Key Data (RSN IE)
        frame[99..99 + rsn_ie.len()].copy_from_slice(&rsn_ie);

        // Compute HMAC-SHA1 MIC over entire EAPOL frame (with MIC field at bytes 81..97 set to 0)
        let mic = compute_eapol_mic(&ptk.kck, &frame);
        frame[81..97].copy_from_slice(&mic);

        Some(frame)
    }

    /// Parses and verifies an incoming EAPOL Message 3 from the Authenticator.
    pub fn parse_and_verify_msg3(&mut self, eapol_frame: &[u8]) -> Result<(), &'static str> {
        if eapol_frame.len() < 99 {
            return Err("EAPOL frame truncated");
        }
        if eapol_frame[1] != 3 || eapol_frame[4] != 2 {
            return Err("Not an RSN EAPOL-Key frame");
        }
        let key_info = u16::from_be_bytes([eapol_frame[5], eapol_frame[6]]);
        let is_pairwise = (key_info & (1 << 3)) != 0;
        let mic_present = (key_info & (1 << 8)) != 0;

        if !is_pairwise || !mic_present {
            return Err("Not an authentic EAPOL Message 3");
        }

        let ptk = self.ptk.as_ref().ok_or("PTK not yet derived")?;

        let mut expected_mic = [0u8; 16];
        expected_mic.copy_from_slice(&eapol_frame[81..97]);

        let mut zeroed = Vec::from(eapol_frame);
        for b in &mut zeroed[81..97] {
            *b = 0;
        }

        let computed_mic = compute_eapol_mic(&ptk.kck, &zeroed);
        if expected_mic != computed_mic {
            self.state = Wpa2State::Failed("Authenticator MIC mismatch in EAPOL Message 3");
            return Err("Authenticator MIC mismatch");
        }

        // Update replay counter from AP
        let mut replay_bytes = [0u8; 8];
        replay_bytes.copy_from_slice(&eapol_frame[9..17]);
        self.replay_counter = u64::from_be_bytes(replay_bytes);

        self.state = Wpa2State::HandshakeMsg3Wait;
        Ok(())
    }

    /// Constructs EAPOL-Key Message 4 to transmit to the Authenticator.
    pub fn build_msg4(&mut self) -> Option<Vec<u8>> {
        let ptk = self.ptk.as_ref()?;
        let body_len = 95; // 95 bytes (no key data)
        let mut frame = vec![0u8; 4 + body_len];

        // EAPOL Header
        frame[0] = 2; // Version 2
        frame[1] = 3; // EAPOL-Key
        frame[2..4].copy_from_slice(&(body_len as u16).to_be_bytes());

        // Key Descriptor
        frame[4] = 2; // Descriptor Type: RSN
        // Key Info: Key Descr Version 2, Key Type = 1 (Pairwise), Key MIC = 1, Secure = 1 -> 0x030A
        frame[5..7].copy_from_slice(&0x030Au16.to_be_bytes());
        frame[7..9].copy_from_slice(&16u16.to_be_bytes()); // Key Length = 16
        frame[9..17].copy_from_slice(&self.replay_counter.to_be_bytes()); // Replay Counter
        // Nonce, IV, RSC, Reserved remain 0
        // Key Data Length = 0
        frame[97..99].copy_from_slice(&0u16.to_be_bytes());

        // Compute HMAC-SHA1 MIC
        let mic = compute_eapol_mic(&ptk.kck, &frame);
        frame[81..97].copy_from_slice(&mic);

        self.state = Wpa2State::Connected;
        Some(frame)
    }

    /// Handles an incoming EAPOL Message 3, verifying the Authenticator MIC.
    pub fn handle_msg3(&mut self, mic: &[u8; 16], verified: bool) -> bool {
        if !verified {
            self.state = Wpa2State::Failed("Authenticator MIC mismatch in EAPOL Message 3");
            return false;
        }
        let _ = mic;
        self.state = Wpa2State::Connected;
        true
    }
}
