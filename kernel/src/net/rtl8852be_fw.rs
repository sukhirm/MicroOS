//! Realtek RTL8852BE Firmware Acquisition, Parser, and DLFW Layout.
//!
//! Grounded in official Linux rtw89 firmware specifications (fw.h / fw.c).
//! Parses multi-firmware containers (MFW sig 0xFF), extracts sections,
//! verifies header boundaries, and prepares packet payloads for DMA DLFW.

#![allow(dead_code)]

use alloc::vec::Vec;
use core::convert::TryInto;
use crate::memory::mmio::MmioRegion;
use crate::net::rtl8852be_dma::{DmaRingError, Rtl8852beTxRing};

pub const RTW89_MFW_SIG: u8 = 0xFF;
pub const RTW89_FW_NORMAL: u8 = 1;
pub const RTW89_FW_WOWLAN: u8 = 3;
pub const RTW89_FW_NORMAL_CE: u8 = 5;
pub const RTW89_FW_NORMAL_B: u8 = 14;
pub const RTW89_FW_LOGFMT: u8 = 255;

pub const FWDL_SECTION_PER_PKT_LEN: usize = 2020;
pub const FWDL_SECTION_MAX_NUM: usize = 10;

pub const RTW89_FWDL_INITIAL_STATE: u8 = 0;
pub const RTW89_FWDL_CHECKSUM_FAIL: u8 = 2;
pub const RTW89_FWDL_SECURITY_FAIL: u8 = 3;
pub const RTW89_FWDL_CV_NOT_MATCH: u8 = 4;
pub const RTW89_FWDL_WCPU_FWDL_RDY: u8 = 6;
pub const RTW89_FWDL_WCPU_FW_INIT_RDY: u8 = 7;

// H2C Categories and Functions for Firmware Download
pub const H2C_CAT_MAC: u8 = 0;
pub const H2C_CL_MAC_FWDL: u8 = 1;
pub const H2C_FUNC_MAC_FWHDR_DL: u8 = 0;
pub const FWCMD_TYPE_H2C: u8 = 0;

// Core RX packet types for TXWD
pub const RTW89_CORE_RX_TYPE_H2C: u32 = 13;
pub const RTW89_CORE_RX_TYPE_FWDL: u32 = 14;

/// Embedded official Realtek RTL8852B firmware binary (v0.29.29.15).
/// Verified redistributable under LICENCE.rtlwifi_firmware.txt.
pub static RTW8852B_FW_BIN: &[u8] = include_bytes!("../../../firmware/rtw8852b_fw-1.bin");

/// Multi-Firmware Container Header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MfwHeader {
    pub sig: u8,
    pub fw_nr: u8,
    pub major: u8,
    pub minor: u8,
    pub sub: u8,
    pub idx: u8,
}

/// Entry in the MFW Container directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MfwEntry {
    pub cv: u8,
    pub fw_type: u8,
    pub mp: u8,
    pub shift: usize,
    pub size: usize,
}

/// Parsed Section Descriptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParsedSection<'a> {
    pub dl_addr: u32,
    pub size: usize,
    pub sec_type: u8,
    pub checksum: bool,
    pub redl: bool,
    pub data: &'a [u8],
}

/// Parsed Single Firmware Suit.
#[derive(Debug, Clone)]
pub struct ParsedFwSuit<'a> {
    pub chip_id: u32,
    pub cv: u8,
    pub fw_type: u8,
    pub major: u8,
    pub minor: u8,
    pub sub: u8,
    pub idx: u8,
    pub commit_id: u32,
    pub hdr_ver: u8,
    pub part_size: usize,
    pub raw_hdr: &'a [u8],
    pub sections: Vec<ParsedSection<'a>>,
}

/// Complete MFW container view.
pub struct MfwContainer<'a> {
    pub header: MfwHeader,
    pub entries: Vec<MfwEntry>,
    pub raw: &'a [u8],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FwParseError {
    BufferTooShort,
    InvalidSignature,
    NoFirmwareEntries,
    InvalidHeaderVersion,
    TooManySections,
    SectionOffsetOutOfBounds,
    SuitableFirmwareNotFound,
    SectionPayloadTruncated,
}

/// Parses an MFW firmware container binary into an index of entries.
pub fn parse_mfw(bytes: &[u8]) -> Result<MfwContainer<'_>, FwParseError> {
    if bytes.len() < 16 {
        return Err(FwParseError::BufferTooShort);
    }

    let sig = bytes[0];
    if sig != RTW89_MFW_SIG {
        return Err(FwParseError::InvalidSignature);
    }

    let fw_nr = bytes[1];
    if fw_nr == 0 {
        return Err(FwParseError::NoFirmwareEntries);
    }

    let major = bytes[4];
    let minor = bytes[5];
    let sub = bytes[6];
    let idx = bytes[7];

    let header = MfwHeader {
        sig,
        fw_nr,
        major,
        minor,
        sub,
        idx,
    };

    let mut entries = Vec::new();
    let mut offset = 16usize;

    for _ in 0..fw_nr {
        if offset + 16 > bytes.len() {
            return Err(FwParseError::BufferTooShort);
        }

        let cv = bytes[offset];
        let fw_type = bytes[offset + 1];
        let mp = bytes[offset + 2];
        let shift = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap()) as usize;
        let size = u32::from_le_bytes(bytes[offset + 8..offset + 12].try_into().unwrap()) as usize;

        if shift + size > bytes.len() {
            return Err(FwParseError::SectionOffsetOutOfBounds);
        }

        let _ = entries.push(MfwEntry {
            cv,
            fw_type,
            mp,
            shift,
            size,
        });

        offset += 16;
    }

    Ok(MfwContainer {
        header,
        entries,
        raw: bytes,
    })
}

/// Selects and parses the appropriate firmware suit matching the hardware chip cut version (cv).
pub fn select_fw_suit<'a>(
    container: &'a MfwContainer<'a>,
    chip_cv: u8,
) -> Result<ParsedFwSuit<'a>, FwParseError> {
    let mut selected_entry: Option<&MfwEntry> = None;

    // Traverse all candidate entries for RTW89_FW_NORMAL_CE or RTW89_FW_NORMAL
    for entry in &container.entries {
        if entry.fw_type != RTW89_FW_NORMAL_CE && entry.fw_type != RTW89_FW_NORMAL {
            continue;
        }

        if entry.cv <= chip_cv && entry.mp == 0 {
            match selected_entry {
                None => selected_entry = Some(entry),
                Some(prev) => {
                    if prev.cv < entry.cv {
                        selected_entry = Some(entry);
                    }
                }
            }
        }
    }

    // Fallback: if no entry <= chip_cv exists, use the lowest non-MP normal entry
    if selected_entry.is_none() {
        selected_entry = container
            .entries
            .iter()
            .find(|e| (e.fw_type == RTW89_FW_NORMAL_CE || e.fw_type == RTW89_FW_NORMAL) && e.mp == 0);
    }

    let entry = selected_entry.ok_or(FwParseError::SuitableFirmwareNotFound)?;
    let sub_bytes = &container.raw[entry.shift..entry.shift + entry.size];

    parse_single_fw(sub_bytes, entry.cv, entry.fw_type)
}

/// Parses an individual firmware suit binary (v0 or v1 format).
pub fn parse_single_fw<'a>(
    bytes: &'a [u8],
    cv: u8,
    fw_type: u8,
) -> Result<ParsedFwSuit<'a>, FwParseError> {
    if bytes.len() < 32 {
        return Err(FwParseError::BufferTooShort);
    }

    let w0 = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
    let w1 = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
    let w2 = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
    let w3 = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
    let _w4 = u32::from_le_bytes(bytes[16..20].try_into().unwrap());
    let _w5 = u32::from_le_bytes(bytes[20..24].try_into().unwrap());
    let w6 = u32::from_le_bytes(bytes[24..28].try_into().unwrap());
    let w7 = u32::from_le_bytes(bytes[28..32].try_into().unwrap());

    let major = (w1 & 0xFF) as u8;
    let minor = ((w1 >> 8) & 0xFF) as u8;
    let sub = ((w1 >> 16) & 0xFF) as u8;
    let idx = ((w1 >> 24) & 0xFF) as u8;
    let commit_id = w2;
    let hdr_ver = ((w3 >> 24) & 0xFF) as u8;
    let sec_num = ((w6 >> 8) & 0xFF) as usize;
    let mut part_size = (w7 & 0xFFFF) as usize;

    if part_size == 0 || part_size > FWDL_SECTION_PER_PKT_LEN {
        part_size = FWDL_SECTION_PER_PKT_LEN;
    }

    if sec_num == 0 || sec_num > FWDL_SECTION_MAX_NUM {
        return Err(FwParseError::TooManySections);
    }

    let base_hdr_len = if hdr_ver == 0 {
        32 + sec_num * 16
    } else if hdr_ver == 1 {
        48 + sec_num * 16
    } else {
        return Err(FwParseError::InvalidHeaderVersion);
    };

    if base_hdr_len > bytes.len() {
        return Err(FwParseError::BufferTooShort);
    }

    let raw_hdr = &bytes[0..base_hdr_len];
    let mut sections = Vec::new();
    let sec_hdr_offset = if hdr_ver == 0 { 32 } else { 48 };

    let mut payload_offset = base_hdr_len;

    for i in 0..sec_num {
        let sh_start = sec_hdr_offset + i * 16;
        let sw0 = u32::from_le_bytes(bytes[sh_start..sh_start + 4].try_into().unwrap());
        let sw1 = u32::from_le_bytes(bytes[sh_start + 4..sh_start + 8].try_into().unwrap());

        let dl_addr = sw0 & 0x1FFF_FFFF;
        let sec_size = (sw1 & 0x00FF_FFFF) as usize;
        let sec_type = ((sw1 >> 24) & 0x0F) as u8;
        let checksum = ((sw1 >> 28) & 1) != 0;
        let redl = ((sw1 >> 29) & 1) != 0;

        if payload_offset + sec_size > bytes.len() {
            return Err(FwParseError::SectionPayloadTruncated);
        }

        let data = &bytes[payload_offset..payload_offset + sec_size];
        payload_offset += sec_size;

        let _ = sections.push(ParsedSection {
            dl_addr,
            size: sec_size,
            sec_type,
            checksum,
            redl,
            data,
        });
    }

    Ok(ParsedFwSuit {
        chip_id: w0,
        cv,
        fw_type,
        major,
        minor,
        sub,
        idx,
        commit_id,
        hdr_ver,
        part_size,
        raw_hdr,
        sections,
    })
}

/// Builds the 8-byte H2C firmware header download command descriptor.
///
/// H2C_HDR format:
/// `hdr0`: DEL_TYPE (19:16) | CAT (1:0) | CLASS (7:2) | FUNC (15:8) | SEQ (31:24)
/// `hdr1`: TOTAL_LEN (13:0)
pub fn build_h2c_fwdl_hdr(seq: u8, payload_len: usize) -> [u8; 8] {
    let hdr0: u32 = ((FWCMD_TYPE_H2C as u32) << 16)
        | ((H2C_CAT_MAC as u32) & 0x03)
        | (((H2C_CL_MAC_FWDL as u32) & 0x3F) << 2)
        | (((H2C_FUNC_MAC_FWHDR_DL as u32) & 0xFF) << 8)
        | ((seq as u32) << 24);

    let total_len = (payload_len + 8) as u32;
    let hdr1: u32 = total_len & 0x3FFF;

    let mut buf = [0u8; 8];
    buf[0..4].copy_from_slice(&hdr0.to_le_bytes());
    buf[4..8].copy_from_slice(&hdr1.to_le_bytes());
    buf
}

/// Builds the 16-byte hardware TXWD descriptor prepended to every FWCMD/FWDL packet.
///
/// `dword0`: bits 13:0 = pkt_size, bits 27:24 = rx_type (14 for FWDL, 13 for H2C)
pub fn build_txwd_fwcmd(pkt_size: usize, is_fwdl: bool) -> [u8; 16] {
    let rx_type = if is_fwdl {
        RTW89_CORE_RX_TYPE_FWDL
    } else {
        RTW89_CORE_RX_TYPE_H2C
    };

    let dword0: u32 = ((pkt_size as u32) & 0x3FFF) | ((rx_type & 0x0F) << 24);

    let mut txwd = [0u8; 16];
    txwd[0..4].copy_from_slice(&dword0.to_le_bytes());
    txwd
}

// Hardware Register Definitions for Power, Clock, and CPU & FWDL Control
pub const R_AX_SYS_FUNC_EN: usize = 0x0002;
pub const B_AX_FEN_BBRSTB: u16 = 1 << 0;
pub const B_AX_FEN_BB_GLB_RSTN: u16 = 1 << 1;
pub const B_AX_FEN_PCIEA: u16 = 1 << 2;
pub const B_AX_FEN_MAC: u16 = 1 << 3;

pub const R_AX_SYS_PW_CTRL: usize = 0x0004;
pub const B_AX_APFN_ONMAC: u32 = 1 << 8;
pub const B_AX_APFM_SWLPS: u32 = 1 << 9;
pub const B_AX_AFSM_WLSUS_EN: u32 = 1 << 11;
pub const B_AX_AFSM_PCIE_SUS_EN: u32 = 1 << 12;
pub const B_AX_PSUS_OFF_CAPC_EN: u32 = 1 << 14;
pub const B_AX_APDM_HPDN: u32 = 1 << 15;
pub const B_AX_EN_WLON: u32 = 1 << 16;
pub const B_AX_RDY_SYSPWR: u32 = 1 << 17;

pub const R_AX_SYS_CLK_CTRL: usize = 0x0008;
pub const B_AX_MAC_CLK_EN: u32 = 1 << 12;
pub const B_AX_CPU_CLK_EN: u32 = 1 << 14;
pub const B_AX_BB_CLK_EN: u32 = 1 << 15;

pub const R_AX_PLATFORM_ENABLE: usize = 0x0088;
pub const B_AX_PLATFORM_EN: u32 = 1 << 0;
pub const B_AX_WCPU_EN: u32 = 1 << 1;

pub const R_AX_WCPU_FW_CTRL: usize = 0x01E0;
pub const B_AX_WCPU_FWDL_EN: u32 = 1 << 0;
pub const B_AX_H2C_PATH_RDY: u32 = 1 << 1;
pub const B_AX_FWDL_PATH_RDY: u32 = 1 << 2;
pub const B_AX_WCPU_FWDL_STS_MASK: u32 = 0x00E0; // bits 7:5

pub const R_AX_BOOT_REASON: usize = 0x01E6;
pub const R_AX_SEC_CTRL: usize = 0x0C00;
pub const R_AX_UDM1: usize = 0x01F4;
pub const R_AX_UDM2: usize = 0x01F8;
pub const R_AX_HALT_H2C_CTRL: usize = 0x0160;
pub const R_AX_HALT_C2H_CTRL: usize = 0x0164;

// Baseband & RF Control Registers
pub const R_AX_WLRF_CTRL: usize = 0x02F0;
pub const B_AX_AFC_AFEDIG: u32 = 1 << 17;

pub const R_AX_PHYREG_SET: usize = 0x8040;
pub const PHYREG_SET_ALL_CYCLE: u8 = 0x8;

// RX Packet Filter Registers
pub const R_AX_RX_FLTR_OPT: usize = 0xCE20;
pub const B_AX_SNIFFER_MODE: u32 = 1 << 0;
pub const R_AX_CTRL_FLTR: usize = 0xCE24;
pub const R_AX_MGNT_FLTR: usize = 0xCE28;
pub const R_AX_DATA_FLTR: usize = 0xCE2C;

// Direct RF MMIO Offsets (Path A and Path B)
pub const RF_PATH_A_BASE: usize = 0xE000;
pub const RF_PATH_B_BASE: usize = 0xF000;
pub const RR_CFGCH: usize = 0x18;

/// Result of physical firmware loading and boot execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FwBootReport {
    pub loaded: bool,
    pub chip_cv: u8,
    pub major: u8,
    pub minor: u8,
    pub sub: u8,
    pub idx: u8,
    pub sections_downloaded: usize,
    pub bytes_downloaded: usize,
    pub final_wcpu_status: u8,
    pub error: Option<FwBootError>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FwBootError {
    MfwParseFailed(FwParseError),
    MacPowerOnTimeout,
    H2cPathNotReady,
    FwdlPathNotReady,
    DmaSubmissionFailed(DmaRingError),
    WcpuInitFailed(u8),
    MmioAccessError,
}

impl FwBootError {
    pub fn description(&self) -> &'static str {
        match self {
            FwBootError::MfwParseFailed(_) => "MFW binary parse failed",
            FwBootError::MacPowerOnTimeout => "MAC power-on sequence timeout (RDY_SYSPWR not asserted)",
            FwBootError::H2cPathNotReady => "H2C path not ready (WCPU in power-cut state)",
            FwBootError::FwdlPathNotReady => "FWDL path not ready",
            FwBootError::DmaSubmissionFailed(_) => "FWCMD DMA submission timeout",
            FwBootError::WcpuInitFailed(_) => "WCPU init handshake failed",
            FwBootError::MmioAccessError => "MMIO access error",
        }
    }
}

/// Powers on the Realtek RTL8852B MAC, analog rails, system clocks, and function domains.
///
/// Follows the official Linux rtw89 hardware sequence (rtw8852b_pwr_on_func / rtw89_mac_pwr_on).
pub fn mac_power_on(mmio: &MmioRegion) -> Result<(), FwBootError> {
    unsafe {
        // 1. Read current power control register and disable analog power-cut & suspend modes
        let mut pw_ctrl = mmio.read_u32(R_AX_SYS_PW_CTRL).map_err(|_| FwBootError::MmioAccessError)?;
        pw_ctrl &= !(B_AX_PSUS_OFF_CAPC_EN | B_AX_AFSM_WLSUS_EN | B_AX_AFSM_PCIE_SUS_EN | B_AX_APDM_HPDN | B_AX_APFM_SWLPS);
        mmio.write_u32(R_AX_SYS_PW_CTRL, pw_ctrl).map_err(|_| FwBootError::MmioAccessError)?;

        // 2. Bounded poll for system power ready (B_AX_RDY_SYSPWR == 1)
        let mut sys_pwr_ready = false;
        for _ in 0..100_000 {
            let val = mmio.read_u32(R_AX_SYS_PW_CTRL).unwrap_or(0);
            if (val & B_AX_RDY_SYSPWR) != 0 {
                sys_pwr_ready = true;
                break;
            }
            core::hint::spin_loop();
        }
        if !sys_pwr_ready {
            // Give an additional bounded settling window
            for _ in 0..200_000 {
                let val = mmio.read_u32(R_AX_SYS_PW_CTRL).unwrap_or(0);
                if (val & B_AX_RDY_SYSPWR) != 0 {
                    sys_pwr_ready = true;
                    break;
                }
                core::hint::spin_loop();
            }
        }
        if !sys_pwr_ready {
            return Err(FwBootError::MacPowerOnTimeout);
        }

        // 3. Set WLAN power-on and MAC domain power
        let mut pw_ctrl2 = mmio.read_u32(R_AX_SYS_PW_CTRL).unwrap_or(pw_ctrl);
        pw_ctrl2 |= B_AX_EN_WLON | B_AX_APFN_ONMAC;
        mmio.write_u32(R_AX_SYS_PW_CTRL, pw_ctrl2).map_err(|_| FwBootError::MmioAccessError)?;

        // 4. Enable System Function resets and domains (BB, PCIEA, MAC)
        let func = mmio.read_u16(R_AX_SYS_FUNC_EN).unwrap_or(0);
        let new_func = func | B_AX_FEN_BBRSTB | B_AX_FEN_BB_GLB_RSTN | B_AX_FEN_PCIEA | B_AX_FEN_MAC;
        mmio.write_u16(R_AX_SYS_FUNC_EN, new_func).map_err(|_| FwBootError::MmioAccessError)?;

        // 5. Enable System Clocks (CPU, MAC, BB)
        let clk = mmio.read_u32(R_AX_SYS_CLK_CTRL).unwrap_or(0);
        let new_clk = clk | B_AX_CPU_CLK_EN | B_AX_MAC_CLK_EN | B_AX_BB_CLK_EN;
        mmio.write_u32(R_AX_SYS_CLK_CTRL, new_clk).map_err(|_| FwBootError::MmioAccessError)?;
    }
    Ok(())
}

/// De-asserts baseband reset, initializes AFE digital clock, sets PHY cycles, and opens RX filters.
pub fn mac_enable_bb_rf(mmio: &MmioRegion) -> Result<(), FwBootError> {
    unsafe {
        // Release Baseband resets in SYS_FUNC_EN
        let func = mmio.read_u16(R_AX_SYS_FUNC_EN).unwrap_or(0);
        let _ = mmio.write_u16(R_AX_SYS_FUNC_EN, func | B_AX_FEN_BBRSTB | B_AX_FEN_BB_GLB_RSTN);

        // Pulse AFE digital clock in WLRF_CTRL (0x02F0)
        let wlrf = mmio.read_u32(R_AX_WLRF_CTRL).unwrap_or(0);
        let _ = mmio.write_u32(R_AX_WLRF_CTRL, wlrf | B_AX_AFC_AFEDIG);
        let _ = mmio.write_u32(R_AX_WLRF_CTRL, wlrf & !B_AX_AFC_AFEDIG);
        let _ = mmio.write_u32(R_AX_WLRF_CTRL, wlrf | B_AX_AFC_AFEDIG);

        // Configure PHY register cycle (0x8040 -> 0x8)
        let _ = mmio.write_u8(R_AX_PHYREG_SET, PHYREG_SET_ALL_CYCLE);

        // Enable RX Sniffer mode (promiscuous receive of all valid 802.11 frames)
        let _ = mmio.write_u32(R_AX_RX_FLTR_OPT, B_AX_SNIFFER_MODE);
        // Allow all 802.11 Management frames (Beacons, Probe Resp, Auth, Assoc) and Data frames
        let _ = mmio.write_u32(R_AX_MGNT_FLTR, 0xFFFF_FFFF);
        let _ = mmio.write_u32(R_AX_DATA_FLTR, 0xFFFF_FFFF);
    }
    Ok(())
}

/// Sets the physical RF synthesizer frequency to a specific 2.4 GHz channel (1..14).
///
/// Direct-programs RF registers via Path A (0xE000) and Path B (0xF000) MMIO windows.
pub fn set_rf_channel(mmio: &MmioRegion, channel: u8) {
    if channel == 0 || channel > 14 {
        return;
    }
    unsafe {
        let ch_reg_offset = RR_CFGCH << 2; // 0x18 * 4 = 0x60
        let addr_a = RF_PATH_A_BASE + ch_reg_offset; // 0xE060
        let addr_b = RF_PATH_B_BASE + ch_reg_offset; // 0xF060

        // Read current RF channel register on Path A
        let val_a = mmio.read_u32(addr_a).unwrap_or(0);
        let new_val_a = (val_a & !0x303FF) | (channel as u32);
        let _ = mmio.write_u32(addr_a, new_val_a);

        // Read current RF channel register on Path B
        let val_b = mmio.read_u32(addr_b).unwrap_or(0);
        let new_val_b = (val_b & !0x303FF) | (channel as u32);
        let _ = mmio.write_u32(addr_b, new_val_b);
    }
}

/// Disables the Wireless CPU (WCPU) before initiating firmware download.
pub fn disable_wcpu(mmio: &MmioRegion) -> Result<(), FwBootError> {
    unsafe {
        // Clear WCPU enable
        let plt = mmio.read_u32(R_AX_PLATFORM_ENABLE).map_err(|_| FwBootError::MmioAccessError)?;
        mmio.write_u32(R_AX_PLATFORM_ENABLE, plt & !B_AX_WCPU_EN).map_err(|_| FwBootError::MmioAccessError)?;

        // Clear FWDL and path bits
        let fw_ctrl = mmio.read_u32(R_AX_WCPU_FW_CTRL).map_err(|_| FwBootError::MmioAccessError)?;
        mmio.write_u32(R_AX_WCPU_FW_CTRL, fw_ctrl & !(B_AX_WCPU_FWDL_EN | B_AX_H2C_PATH_RDY | B_AX_FWDL_PATH_RDY))
            .map_err(|_| FwBootError::MmioAccessError)?;

        // Clear CPU clock
        let clk = mmio.read_u32(R_AX_SYS_CLK_CTRL).map_err(|_| FwBootError::MmioAccessError)?;
        mmio.write_u32(R_AX_SYS_CLK_CTRL, clk & !B_AX_CPU_CLK_EN).map_err(|_| FwBootError::MmioAccessError)?;

        // Toggle platform enable to reset MCU state
        mmio.write_u32(R_AX_PLATFORM_ENABLE, plt & !B_AX_PLATFORM_EN).map_err(|_| FwBootError::MmioAccessError)?;
        mmio.write_u32(R_AX_PLATFORM_ENABLE, plt | B_AX_PLATFORM_EN).map_err(|_| FwBootError::MmioAccessError)?;
    }
    Ok(())
}

/// Enables the WCPU in DLFW (Firmware Download) mode.
pub fn enable_wcpu_dlfw(mmio: &MmioRegion) -> Result<(), FwBootError> {
    unsafe {
        // Clear UDM registers and halt control
        mmio.write_u32(R_AX_UDM1, 0).map_err(|_| FwBootError::MmioAccessError)?;
        mmio.write_u32(R_AX_UDM2, 0).map_err(|_| FwBootError::MmioAccessError)?;
        mmio.write_u32(R_AX_HALT_H2C_CTRL, 0).map_err(|_| FwBootError::MmioAccessError)?;
        mmio.write_u32(R_AX_HALT_C2H_CTRL, 0).map_err(|_| FwBootError::MmioAccessError)?;

        // Enable CPU clock
        let clk = mmio.read_u32(R_AX_SYS_CLK_CTRL).map_err(|_| FwBootError::MmioAccessError)?;
        mmio.write_u32(R_AX_SYS_CLK_CTRL, clk | B_AX_CPU_CLK_EN).map_err(|_| FwBootError::MmioAccessError)?;

        // Configure WCPU FW CTRL: clear status, enable FWDL
        let mut fw_ctrl = mmio.read_u32(R_AX_WCPU_FW_CTRL).map_err(|_| FwBootError::MmioAccessError)?;
        fw_ctrl &= !(B_AX_WCPU_FWDL_EN | B_AX_H2C_PATH_RDY | B_AX_FWDL_PATH_RDY | B_AX_WCPU_FWDL_STS_MASK);
        fw_ctrl |= B_AX_WCPU_FWDL_EN;
        mmio.write_u32(R_AX_WCPU_FW_CTRL, fw_ctrl).map_err(|_| FwBootError::MmioAccessError)?;

        // Configure security IDMEM size for RTL8852B
        let sec = mmio.read_u32(R_AX_SEC_CTRL).unwrap_or(0);
        let _ = mmio.write_u32(R_AX_SEC_CTRL, (sec & !(3 << 16)) | (2 << 16));

        // Set boot reason to normal (0)
        let _ = mmio.write_u16(R_AX_BOOT_REASON, 0);

        // Start WCPU
        let plt = mmio.read_u32(R_AX_PLATFORM_ENABLE).map_err(|_| FwBootError::MmioAccessError)?;
        mmio.write_u32(R_AX_PLATFORM_ENABLE, plt | B_AX_WCPU_EN).map_err(|_| FwBootError::MmioAccessError)?;
    }
    Ok(())
}

/// Orchestrates complete Realtek RTL8852BE firmware download and verifies execution.
pub fn download_and_boot_firmware(
    mmio: &MmioRegion,
    chip_cv: u8,
    fwcmd_ring: &mut Rtl8852beTxRing,
) -> Result<FwBootReport, FwBootError> {
    // 0. Power on MAC, system clocks, and functional domains
    mac_power_on(mmio)?;

    // 1. Parse Embedded MFW Binary
    let mfw = parse_mfw(RTW8852B_FW_BIN).map_err(FwBootError::MfwParseFailed)?;
    let suit = select_fw_suit(&mfw, chip_cv).map_err(FwBootError::MfwParseFailed)?;

    // 2. Disable WCPU and put silicon into DLFW mode
    disable_wcpu(mmio)?;
    enable_wcpu_dlfw(mmio)?;

    // 3. Wait for H2C Path Ready
    let mut h2c_ready = false;
    for _ in 0..100_000 {
        let val = unsafe { mmio.read_u32(R_AX_WCPU_FW_CTRL).unwrap_or(0) };
        if (val & B_AX_H2C_PATH_RDY) != 0 {
            h2c_ready = true;
            break;
        }
        core::hint::spin_loop();
    }
    if !h2c_ready {
        return Err(FwBootError::H2cPathNotReady);
    }

    // 4. Download FW Header via H2C
    let mut h2c_payload = Vec::new();
    let h2c_hdr = build_h2c_fwdl_hdr(0, suit.raw_hdr.len());
    h2c_payload.extend_from_slice(&h2c_hdr);
    h2c_payload.extend_from_slice(suit.raw_hdr);

    fwcmd_ring
        .submit_fwcmd_sync(mmio, &h2c_payload, false)
        .map_err(FwBootError::DmaSubmissionFailed)?;

    // 5. Wait for FWDL Path Ready
    let mut fwdl_ready = false;
    for _ in 0..100_000 {
        let val = unsafe { mmio.read_u32(R_AX_WCPU_FW_CTRL).unwrap_or(0) };
        if (val & B_AX_FWDL_PATH_RDY) != 0 {
            fwdl_ready = true;
            break;
        }
        core::hint::spin_loop();
    }
    if !fwdl_ready {
        return Err(FwBootError::FwdlPathNotReady);
    }

    // 6. Download Sections (Chunked to suit.part_size)
    let mut total_bytes = 0usize;
    for section in &suit.sections {
        let mut residue = section.data;
        while !residue.is_empty() {
            let chunk_len = residue.len().min(suit.part_size);
            let chunk = &residue[..chunk_len];

            fwcmd_ring
                .submit_fwcmd_sync(mmio, chunk, true)
                .map_err(FwBootError::DmaSubmissionFailed)?;

            total_bytes += chunk_len;
            residue = &residue[chunk_len..];
        }
    }

    // 7. Settle and Verify FreeRTOS Boot Status
    // Delay 500,000 iterations for MCU PLL stabilization
    for _ in 0..500_000 {
        core::hint::spin_loop();
    }

    let mut wcpu_status = 0u8;
    for _ in 0..100_000 {
        let val = unsafe { mmio.read_u32(R_AX_WCPU_FW_CTRL).unwrap_or(0) };
        wcpu_status = ((val >> 5) & 0x07) as u8;
        if wcpu_status == RTW89_FWDL_WCPU_FW_INIT_RDY {
            break;
        }
        core::hint::spin_loop();
    }

    if wcpu_status != RTW89_FWDL_WCPU_FW_INIT_RDY {
        return Err(FwBootError::WcpuInitFailed(wcpu_status));
    }

    // 8. Enable Baseband modem, AFE clocks, and promiscuous RX filter
    let _ = mac_enable_bb_rf(mmio);

    // 9. Tune RF synthesizers to default Channel 1 (2412 MHz)
    set_rf_channel(mmio, 1);

    Ok(FwBootReport {
        loaded: true,
        chip_cv: suit.cv,
        major: suit.major,
        minor: suit.minor,
        sub: suit.sub,
        idx: suit.idx,
        sections_downloaded: suit.sections.len(),
        bytes_downloaded: total_bytes,
        final_wcpu_status: wcpu_status,
        error: None,
    })
}
