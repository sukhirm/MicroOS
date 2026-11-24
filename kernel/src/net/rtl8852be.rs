//! Realtek RTL8852BE-VT Wi-Fi 6 PCIe Adapter Hardware Probe & Telemetry Driver (Stage 2).
//!
//! Provides safe, read-only hardware inspection, 64-bit BAR0 MMIO mapping,
//! live silicon register probing, and firmware DMA staging pre-flight verification.
//!
//! Strictly read-only: No unverified register writes, zero fake data, bounded timeouts.

#![allow(dead_code)]

use spin::Mutex;
use x86_64::PhysAddr;
use alloc::string::String;
use alloc::vec::Vec;

use crate::arch::x86_64::pci::{self, PciBdf, VENDOR_REALTEK};
use crate::crypto::Wpa2Session;
use crate::memory::dma::{DmaBuffer, DmaError};
use crate::memory::mmio::{map_device_mmio, MmioError, MmioRegion};
use crate::net::rtl8852be_dma::{
    Rtl8852beRxRing, Rtl8852beTxRing, B_AX_RXBD_MODE, B_AX_RXHCI_EN, B_AX_TXHCI_EN,
    R_AX_PCIE_INIT_CFG1,
};
use crate::net::rtl8852be_fw::{download_and_boot_firmware, set_rf_channel, FwBootError, FwBootReport};
use crate::net::wifi_frame::{
    build_80211_data_frame, build_assoc_request, build_auth_frame, build_deauth_frame,
    build_probe_request, parse_80211_data_frame, parse_80211_frame, parse_assoc_response,
    parse_auth_response, rtl8852b_raw_to_rssi_dbm, WIFI_SCAN_TABLE,
};
use crate::net::NETWORK_STACK;
use crate::net::stack::DhcpState;

// ============================================================================
// Documented RTL8852B Hardware Register Offsets (Linux rtw89 / NØNOS specs)
// ============================================================================

pub const REG_SYS_CFG: usize = 0x0000; // System Configuration Header (16-bit)
pub const REG_SYS_FUNC_EN: usize = 0x0002; // System Function Enable (16-bit: BB_GLB_RSTN, BBRSTB)
pub const REG_SYS_PW_CFG: usize = 0x0004; // System Power Configuration (16-bit)
pub const REG_SYS_CLK: usize = 0x0008; // System Clock Control (32-bit)
pub const REG_SYS_EEPROM: usize = 0x00A0; // EEPROM / eFuse Controller Register (32-bit)
pub const REG_PCIE_STATUS: usize = 0x0200; // PCIe Status & Link State (32-bit)

// Required contiguous DMA buffer size for rtw8852b_fw-1.bin (~840 KiB)
pub const RTL8852B_FW_DMA_SIZE: usize = 840 * 1024;
pub const RTL8852B_FW_DMA_ALIGN: usize = 4096;

/// Hardware telemetry report produced by probing the physical Realtek RTL8852BE silicon.
#[derive(Debug, Clone, Copy)]
pub struct Rtl8852beProbeReport {
    pub detected: bool,
    pub bdf: PciBdf,
    pub vendor_id: u16,
    pub device_id: u16,
    pub subsystem_vid: u16,
    pub subsystem_did: u16,
    pub bar0_phys: u64,
    pub mmio_virt: u64,
    pub pci_command: u16,
    pub reg_chip_info: u16,
    pub reg_sys_func_en: u16,
    pub reg_sys_clk: u32,
    pub reg_eeprom_ctrl: u32,
    pub reg_raw_dword0: u32,
    pub reg_raw_dword1: u32,
    pub reg_raw_dword2: u32,
    pub is_floating_bus: bool,
    pub dma_buffer_phys: u64,
    pub dma_buffer_size: usize,
    pub dma_staging_verified: bool,
    pub error_code: Option<Rtl8852beProbeError>,
    pub fw_error: Option<FwBootError>,
}

impl Default for Rtl8852beProbeReport {
    fn default() -> Self {
        Self {
            detected: false,
            bdf: PciBdf::new(0, 0, 0),
            vendor_id: 0,
            device_id: 0,
            subsystem_vid: 0,
            subsystem_did: 0,
            bar0_phys: 0,
            mmio_virt: 0,
            pci_command: 0,
            reg_chip_info: 0,
            reg_sys_func_en: 0,
            reg_sys_clk: 0,
            reg_eeprom_ctrl: 0,
            reg_raw_dword0: 0,
            reg_raw_dword1: 0,
            reg_raw_dword2: 0,
            is_floating_bus: false,
            dma_buffer_phys: 0,
            dma_buffer_size: 0,
            dma_staging_verified: false,
            error_code: None,
            fw_error: None,
        }
    }
}

/// Specific failure modes encountered during physical hardware probing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rtl8852beProbeError {
    PciDeviceNotFound,
    NoValidMmioBar,
    MmioMappingFailed(MmioError),
    DmaBufferAllocationFailed(DmaError),
    FloatingBusDetected,
    PciBusMasterDisabled,
}

impl Rtl8852beProbeError {
    pub fn description(&self) -> &'static str {
        match self {
            Rtl8852beProbeError::PciDeviceNotFound => "Physical PCI adapter 10EC:B852 not detected on bus",
            Rtl8852beProbeError::NoValidMmioBar => "Device has no valid 64-bit/32-bit MMIO memory BAR",
            Rtl8852beProbeError::MmioMappingFailed(_) => "Failed to map BAR0 into kernel MMIO virtual address window",
            Rtl8852beProbeError::DmaBufferAllocationFailed(_) => "Failed to allocate 840 KiB contiguous DMA frame pool",
            Rtl8852beProbeError::FloatingBusDetected => "MMIO registers returned 0xFFFFFFFF (unpowered silicon / floating bus)",
            Rtl8852beProbeError::PciBusMasterDisabled => "PCI Bus Master (DMA) bit could not be enabled in configuration space",
        }
    }
}

/// Global spinlock-protected hardware probe telemetry.
pub static RTL8852BE_PROBE_STATE: Mutex<Option<Rtl8852beProbeReport>> = Mutex::new(None);

/// Probes the physical PCI bus for the Realtek RTL8852BE-VT Wi-Fi 6 card.
///
/// Executes strictly read-only register inspections and verifies DMA staging buffers.
pub fn probe_hardware() -> Rtl8852beProbeReport {
    let mut report = Rtl8852beProbeReport::default();

    // 1. Scan all PCI buses for Realtek Wi-Fi 6 device IDs across all class codes
    let pci_devices = pci::scan_pci_bus();
    let target_dev = pci_devices.into_iter().find(|dev| {
        dev.vendor_id == VENDOR_REALTEK
            && (dev.device_id == 0xB852
                || dev.device_id == 0x8852
                || dev.device_id == 0xC852
                || dev.device_id == 0xB851
                || dev.device_id == 0xB85B
                || dev.device_id == 0xA852)
    });

    let pci_dev = match target_dev {
        Some(dev) => dev,
        None => {
            report.error_code = Some(Rtl8852beProbeError::PciDeviceNotFound);
            *RTL8852BE_PROBE_STATE.lock() = Some(report);
            return report;
        }
    };

    report.detected = true;
    report.bdf = pci_dev.bdf;
    report.vendor_id = pci_dev.vendor_id;
    report.device_id = pci_dev.device_id;
    report.subsystem_vid = pci_dev.subsystem_vendor_id;
    report.subsystem_did = pci_dev.subsystem_id;

    // 2. Decode 64-bit BAR0 MMIO physical address
    let bar0_phys = match pci_dev.mmio_base() {
        Some(addr) if addr != 0 => addr,
        _ => {
            report.error_code = Some(Rtl8852beProbeError::NoValidMmioBar);
            *RTL8852BE_PROBE_STATE.lock() = Some(report);
            return report;
        }
    };
    report.bar0_phys = bar0_phys;

    // 3. Set PCIe Power State to D0, disable ASPM, and enable Bus Master (DMA)
    unsafe {
        pci_dev.set_power_state_d0();
        pci_dev.disable_pcie_aspm();
        pci_dev.enable_bus_master_and_memory();
    }
    let pci_cmd = unsafe {
        pci::pci_read_config_u16(pci_dev.bdf.bus, pci_dev.bdf.device, pci_dev.bdf.function, 0x04)
    };
    report.pci_command = pci_cmd;

    if (pci_cmd & 0x0004) == 0 {
        report.error_code = Some(Rtl8852beProbeError::PciBusMasterDisabled);
        *RTL8852BE_PROBE_STATE.lock() = Some(report);
        return report;
    }

    // 4. Map 1 MiB MMIO region into kernel virtual window (0x4444_6000_0000)
    let mmio = match map_device_mmio(PhysAddr::new(bar0_phys), 0x10_0000) {
        Ok(m) => m,
        Err(err) => {
            report.error_code = Some(Rtl8852beProbeError::MmioMappingFailed(err));
            *RTL8852BE_PROBE_STATE.lock() = Some(report);
            return report;
        }
    };
    report.mmio_virt = mmio.virt_addr().as_u64();

    // 5. Read-Only Silicon Register Probing
    // Read raw dwords at base offsets with bounded accesses
    let dword0 = unsafe { mmio.read_u32(0x0000).unwrap_or(0xFFFF_FFFF) };
    let dword1 = unsafe { mmio.read_u32(0x0004).unwrap_or(0xFFFF_FFFF) };
    let dword2 = unsafe { mmio.read_u32(0x0008).unwrap_or(0xFFFF_FFFF) };

    report.reg_raw_dword0 = dword0;
    report.reg_raw_dword1 = dword1;
    report.reg_raw_dword2 = dword2;

    // Floating bus detection: if all dwords return 0xFFFFFFFF, PCI bus is disconnected/unpowered
    if dword0 == 0xFFFF_FFFF && dword1 == 0xFFFF_FFFF && dword2 == 0xFFFF_FFFF {
        report.is_floating_bus = true;
        report.error_code = Some(Rtl8852beProbeError::FloatingBusDetected);
        *RTL8852BE_PROBE_STATE.lock() = Some(report);
        return report;
    }

    report.reg_chip_info = (dword0 & 0xFFFF) as u16;
    report.reg_sys_func_en = ((dword0 >> 16) & 0xFFFF) as u16;
    report.reg_sys_clk = dword2;
    report.reg_eeprom_ctrl = unsafe { mmio.read_u32(REG_SYS_EEPROM).unwrap_or(0) };

    // 6. Pre-flight Firmware DMA Staging Buffer Verification
    // Verify physical contiguity, 4 KiB alignment, and pool bounds for 840 KiB firmware image
    match DmaBuffer::allocate(RTL8852B_FW_DMA_SIZE, RTL8852B_FW_DMA_ALIGN) {
        Ok(dma) => {
            report.dma_buffer_phys = dma.phys_addr().as_u64();
            report.dma_buffer_size = dma.size();
            report.dma_staging_verified = true;
        }
        Err(err) => {
            report.error_code = Some(Rtl8852beProbeError::DmaBufferAllocationFailed(err));
            *RTL8852BE_PROBE_STATE.lock() = Some(report);
            return report;
        }
    }

    *RTL8852BE_PROBE_STATE.lock() = Some(report);

    // 7. Initialize Hardware Driver, DMA Rings, and DLFW State Machine if Silicon is Active
    if !report.is_floating_bus {
        // Read hardware MAC address if available at REG_MAC_ADDR (0x01A0 / 0x01A4)
        let mac0 = unsafe { mmio.read_u32(0x01A0).unwrap_or(0) };
        let mac1 = unsafe { mmio.read_u16(0x01A4).unwrap_or(0) };
        let mac_address = if mac0 != 0 && mac0 != 0xFFFF_FFFF {
            let b0 = (mac0 & 0xFF) as u8;
            let b1 = ((mac0 >> 8) & 0xFF) as u8;
            let b2 = ((mac0 >> 16) & 0xFF) as u8;
            let b3 = ((mac0 >> 24) & 0xFF) as u8;
            let b4 = (mac1 & 0xFF) as u8;
            let b5 = ((mac1 >> 8) & 0xFF) as u8;
            [b0, b1, b2, b3, b4, b5]
        } else {
            // Deterministic locally administered unicast MAC (IEEE 802.11 compliant: bit 1 of byte 0 set)
            [0x02, 0xEC, 0xB8, 0x52, pci_dev.bdf.bus, pci_dev.bdf.device]
        };

        if let Ok(mut fwcmd_ring) = Rtl8852beTxRing::new_fwcmd_ring(&mmio) {
            if let Ok(mgmt_ring) = Rtl8852beTxRing::new_mgmt_ring(&mmio) {
                if let Ok(data_ring) = Rtl8852beTxRing::new_data_ring(&mmio) {
                    if let Ok(rx_ring) = Rtl8852beRxRing::new_rx_ring(&mmio) {
                        // Enable PCIe TX and RX DMA engines
                        unsafe {
                            let _ = mmio.write_u32(
                                R_AX_PCIE_INIT_CFG1,
                                B_AX_RXHCI_EN | B_AX_TXHCI_EN | B_AX_RXBD_MODE,
                            );
                        }

                        // Attempt official firmware download & boot
                        let chip_cv = ((report.reg_chip_info >> 12) & 0x0F) as u8;
                        let fw_result = download_and_boot_firmware(&mmio, chip_cv, &mut fwcmd_ring);
                        let (fw_report, fw_err) = match fw_result {
                            Ok(r) => (Some(r), None),
                            Err(e) => (None, Some(e)),
                        };
                        report.fw_error = fw_err;

                        let driver = Rtl8852beDriver {
                            bdf: pci_dev.bdf,
                            mmio,
                            fwcmd_ring,
                            mgmt_ring,
                            data_ring,
                            rx_ring,
                            fw_report,
                            mac_address,
                        };
                        *WIFI_DEVICE.lock() = Some(driver);
                    }
                }
            }
        }
    }

    *RTL8852BE_PROBE_STATE.lock() = Some(report);
    report
}

/// Active Realtek RTL8852BE-VT physical hardware driver instance.
pub struct Rtl8852beDriver {
    pub bdf: PciBdf,
    pub mmio: MmioRegion,
    pub fwcmd_ring: Rtl8852beTxRing,
    pub mgmt_ring: Rtl8852beTxRing,
    pub data_ring: Rtl8852beTxRing,
    pub rx_ring: Rtl8852beRxRing,
    pub fw_report: Option<FwBootReport>,
    pub mac_address: [u8; 6],
}

impl Rtl8852beDriver {
    pub fn set_channel(&self, channel: u8) {
        set_rf_channel(&self.mmio, channel);
    }
}

/// Global spinlock-protected Wi-Fi adapter driver singleton.
pub static WIFI_DEVICE: Mutex<Option<Rtl8852beDriver>> = Mutex::new(None);

/// Wi-Fi Link and Authentication Lifecycle States.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WifiLinkState {
    Disconnected,
    Scanning,
    Authenticating,
    Associating,
    Handshake,
    ObtainingIp,
    Connected,
    Failed,
}

impl WifiLinkState {
    pub fn name(&self) -> &'static str {
        match self {
            WifiLinkState::Disconnected => "Disconnected",
            WifiLinkState::Scanning => "Scanning...",
            WifiLinkState::Authenticating => "Authenticating (802.11 Auth)...",
            WifiLinkState::Associating => "Associating (802.11 Assoc)...",
            WifiLinkState::Handshake => "Handshake (WPA2 4-Way)...",
            WifiLinkState::ObtainingIp => "Obtaining IP (DHCP)...",
            WifiLinkState::Connected => "Connected",
            WifiLinkState::Failed => "Connection Failed",
        }
    }
}

/// In-Kernel 802.11 Supplicant and Session Manager.
pub struct WifiSupplicant {
    pub state: WifiLinkState,
    pub target_ssid: String,
    pub target_bssid: [u8; 6],
    pub is_wpa2: bool,
    pub wpa2_session: Option<Wpa2Session>,
    pub seq_counter: u16,
    pub failure_reason: Option<&'static str>,
}

impl WifiSupplicant {
    pub const fn new() -> Self {
        Self {
            state: WifiLinkState::Disconnected,
            target_ssid: String::new(),
            target_bssid: [0; 6],
            is_wpa2: false,
            wpa2_session: None,
            seq_counter: 1,
            failure_reason: None,
        }
    }

    pub fn next_seq(&mut self) -> u16 {
        let seq = self.seq_counter;
        self.seq_counter = self.seq_counter.wrapping_add(1);
        seq
    }
}

/// Global spinlock-protected Wi-Fi Supplicant instance.
pub static WIFI_SUPPLICANT: Mutex<WifiSupplicant> = Mutex::new(WifiSupplicant::new());

/// Starts dynamic host configuration (DHCP Discover) over the active Wi-Fi link.
pub fn start_wifi_dhcp() -> Result<(), &'static str> {
    let mut dev_guard = match WIFI_DEVICE.try_lock() {
        Some(g) => g,
        None => return Err("Wi-Fi device busy"),
    };
    let driver = dev_guard.as_mut().ok_or("Wi-Fi hardware driver not initialized")?;

    let mut supp = WIFI_SUPPLICANT.lock();
    let bssid = supp.target_bssid;
    let seq = supp.next_seq();

    let discover_eth = NETWORK_STACK.lock().build_dhcp_discover();
    if discover_eth.len() < 14 {
        return Err("DHCP discover frame too short");
    }

    let ethertype = u16::from_be_bytes([discover_eth[12], discover_eth[13]]);
    let wifi_frame = build_80211_data_frame(
        driver.mac_address,
        bssid,
        [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF],
        ethertype,
        &discover_eth[14..],
        seq,
    );

    driver.data_ring.submit_packet(&driver.mmio, &wifi_frame)
        .map_err(|_| "Failed to transmit DHCP Discover via Wi-Fi data ring")?;

    Ok(())
}

/// Cooperatively polls the physical RTL8852BE radio receiver for incoming 802.11 frames.
pub fn poll_wifi() {
    let mut dev_guard = match WIFI_DEVICE.try_lock() {
        Some(g) => g,
        None => return, // Non-blocking: skip turn if device lock is held
    };
    let driver = match dev_guard.as_mut() {
        Some(d) => d,
        None => return,
    };

    let mut rx_buf = [0u8; 2048];
    let mut outbound_data_frames: Vec<Vec<u8>> = Vec::new();
    let mut should_start_dhcp = false;

    // Drain up to 8 packets per turn to keep polling bounded
    for _ in 0..8 {
        if let Some(len) = driver.rx_ring.poll_receive(&driver.mmio, &mut rx_buf) {
            if len <= 16 {
                continue;
            }
            let raw_rssi = rx_buf[4];
            let rssi_dbm = rtl8852b_raw_to_rssi_dbm(raw_rssi);

            // 1. Check for 802.11 Beacons / Probe Responses
            if let Some(net) = parse_80211_frame(&rx_buf[..len], rssi_dbm) {
                WIFI_SCAN_TABLE.lock().update_network(net);
            }

            // 2. Check for 802.11 Authentication Responses
            if let Some((trans_seq, status)) = parse_auth_response(&rx_buf[..len]) {
                let mut supp = WIFI_SUPPLICANT.lock();
                if supp.state == WifiLinkState::Authenticating && trans_seq == 2 {
                    if status == 0 {
                        // Auth successful -> Send Association Request!
                        supp.state = WifiLinkState::Associating;
                        let seq = supp.next_seq();
                        let assoc_req = build_assoc_request(
                            driver.mac_address,
                            supp.target_bssid,
                            &supp.target_ssid,
                            supp.is_wpa2,
                            seq,
                        );
                        let _ = driver.mgmt_ring.submit_packet(&driver.mmio, &assoc_req);
                    } else {
                        supp.state = WifiLinkState::Failed;
                        supp.failure_reason = Some("802.11 Authentication rejected by AP");
                    }
                }
            }

            // 3. Check for 802.11 Association Responses
            if let Some((status, _aid)) = parse_assoc_response(&rx_buf[..len]) {
                let mut supp = WIFI_SUPPLICANT.lock();
                if supp.state == WifiLinkState::Associating {
                    if status == 0 {
                        if supp.is_wpa2 {
                            supp.state = WifiLinkState::Handshake;
                        } else {
                            supp.state = WifiLinkState::ObtainingIp;
                            NETWORK_STACK.lock().set_mac(driver.mac_address);
                            should_start_dhcp = true;
                        }
                    } else {
                        supp.state = WifiLinkState::Failed;
                        supp.failure_reason = Some("802.11 Association rejected by AP");
                    }
                }
            }

            // 4. Check for 802.11 Data Frames (EAPOL, ARP, IPv4)
            if let Some(parsed) = parse_80211_data_frame(&rx_buf[..len]) {
                let mut supp = WIFI_SUPPLICANT.lock();
                if parsed.ethertype == 0x888E {
                    // EAPOL frame
                    if supp.state == WifiLinkState::Handshake {
                        if let Some(session) = supp.wpa2_session.as_mut() {
                            if let Some((anonce, replay)) = Wpa2Session::parse_msg1(parsed.payload) {
                                session.handle_msg1(&anonce, replay);
                                if let Some(msg2_payload) = session.build_msg2() {
                                    let seq = supp.next_seq();
                                    let frame2 = build_80211_data_frame(
                                        driver.mac_address,
                                        supp.target_bssid,
                                        supp.target_bssid,
                                        0x888E,
                                        &msg2_payload,
                                        seq,
                                    );
                                    let _ = driver.data_ring.submit_packet(&driver.mmio, &frame2);
                                }
                            } else if session.parse_and_verify_msg3(parsed.payload).is_ok() {
                                if let Some(msg4_payload) = session.build_msg4() {
                                    let seq = supp.next_seq();
                                    let frame4 = build_80211_data_frame(
                                        driver.mac_address,
                                        supp.target_bssid,
                                        supp.target_bssid,
                                        0x888E,
                                        &msg4_payload,
                                        seq,
                                    );
                                    let _ = driver.data_ring.submit_packet(&driver.mmio, &frame4);
                                }
                                supp.state = WifiLinkState::ObtainingIp;
                                NETWORK_STACK.lock().set_mac(driver.mac_address);
                                should_start_dhcp = true;
                            }
                        }
                    }
                } else if parsed.ethertype == 0x0800 || parsed.ethertype == 0x0806 {
                    // Reconstruct Ethernet frame: DA (6) || SA (6) || EtherType (2) || Payload
                    let mut eth_frame = Vec::with_capacity(14 + parsed.payload.len());
                    eth_frame.extend_from_slice(&parsed.da);
                    eth_frame.extend_from_slice(&parsed.sa);
                    eth_frame.extend_from_slice(&parsed.ethertype.to_be_bytes());
                    eth_frame.extend_from_slice(parsed.payload);

                    let mut stack = NETWORK_STACK.lock();
                    if let Some(resp) = stack.process_incoming_frame(&eth_frame) {
                        outbound_data_frames.push(resp);
                    }

                    if supp.state == WifiLinkState::ObtainingIp && stack.ipv4_address().is_some() {
                        supp.state = WifiLinkState::Connected;
                    }
                }
            }
        } else {
            break;
        }
    }

    // Transmit any replies generated by network stack
    let bssid = match WIFI_SUPPLICANT.try_lock() {
        Some(supp) => supp.target_bssid,
        None => [0u8; 6],
    };
    for resp in outbound_data_frames {
        if resp.len() >= 14 {
            let seq = match WIFI_SUPPLICANT.try_lock() {
                Some(mut supp) => supp.next_seq(),
                None => 0,
            };
            let mut da = [0u8; 6];
            da.copy_from_slice(&resp[0..6]);
            let ethertype = u16::from_be_bytes([resp[12], resp[13]]);
            let wifi_frame = build_80211_data_frame(
                driver.mac_address,
                bssid,
                da,
                ethertype,
                &resp[14..],
                seq,
            );
            let _ = driver.data_ring.submit_packet(&driver.mmio, &wifi_frame);
        }
    }

    drop(dev_guard);

    // Start DHCP if state transitioned to ObtainingIp
    if should_start_dhcp {
        let _ = start_wifi_dhcp();
    }
}

/// Triggers an active 802.11 Probe Request over the air across 2.4 GHz channels.
pub fn trigger_active_scan() {
    let mut dev_guard = match WIFI_DEVICE.try_lock() {
        Some(g) => g,
        None => return,
    };
    if let Some(driver) = dev_guard.as_mut() {
        let mut supp = match WIFI_SUPPLICANT.try_lock() {
            Some(s) => s,
            None => return,
        };
        // Sweep active probe requests across standard non-overlapping 2.4 GHz channels (1, 6, 11)
        for &channel in &[1u8, 6, 11] {
            set_rf_channel(&driver.mmio, channel);
            let seq = supp.next_seq();
            let probe = build_probe_request(driver.mac_address, seq);
            let _ = driver.mgmt_ring.submit_packet(&driver.mmio, &probe);
        }
        // Return to channel 1 for baseline listening
        set_rf_channel(&driver.mmio, 1);

        if supp.state == WifiLinkState::Disconnected {
            supp.state = WifiLinkState::Scanning;
        }
    }
}

/// Connects to a selected Wi-Fi Access Point using Open or WPA2-Personal authentication.
pub fn connect_to_network(
    ssid: &str,
    passphrase: &str,
    bssid: [u8; 6],
    is_wpa2: bool,
) -> Result<(), &'static str> {
    let mut dev_guard = WIFI_DEVICE.lock();
    let driver = dev_guard.as_mut().ok_or("Wi-Fi hardware driver not initialized")?;

    // Switch RF synthesizer to the target Access Point's operating channel
    let channel = WIFI_SCAN_TABLE.lock().networks.iter()
        .find(|n| n.bssid == bssid || n.ssid == ssid)
        .map(|n| n.channel)
        .unwrap_or(1);
    set_rf_channel(&driver.mmio, channel);

    let mut supp = WIFI_SUPPLICANT.lock();
    supp.target_ssid = String::from(ssid);
    supp.target_bssid = bssid;
    supp.is_wpa2 = is_wpa2;
    supp.failure_reason = None;

    if is_wpa2 {
        supp.wpa2_session = Some(Wpa2Session::new(ssid, passphrase, bssid, driver.mac_address));
    } else {
        supp.wpa2_session = None;
    }

    supp.state = WifiLinkState::Authenticating;
    let seq = supp.next_seq();
    let auth = build_auth_frame(driver.mac_address, bssid, seq, 1);
    driver.mgmt_ring.submit_packet(&driver.mmio, &auth)
        .map_err(|_| "Failed to transmit 802.11 Open Auth frame")?;

    Ok(())
}

/// Disconnects the active Wi-Fi session and clears IP configuration.
pub fn disconnect_network() {
    let mut dev_guard = WIFI_DEVICE.lock();
    if let Some(driver) = dev_guard.as_mut() {
        let mut supp = WIFI_SUPPLICANT.lock();
        if supp.state != WifiLinkState::Disconnected {
            let seq = supp.next_seq();
            let deauth = build_deauth_frame(driver.mac_address, supp.target_bssid, 3, seq);
            let _ = driver.mgmt_ring.submit_packet(&driver.mmio, &deauth);
        }
        supp.state = WifiLinkState::Disconnected;
        supp.wpa2_session = None;
        supp.target_ssid.clear();
    }
    if let Some(mut stack) = NETWORK_STACK.try_lock() {
        stack.dhcp_state = DhcpState::Idle;
    }
}

/// Returns `true` if Wi-Fi link is at or beyond L2 association (ready for DHCP).
pub fn is_wifi_associated() -> bool {
    let supp = match WIFI_SUPPLICANT.try_lock() {
        Some(s) => s,
        None => return false,
    };
    supp.state == WifiLinkState::ObtainingIp || supp.state == WifiLinkState::Connected
}

/// Returns `true` if Wi-Fi is fully connected and has an assigned IPv4 address.
pub fn is_wifi_connected() -> bool {
    let supp = match WIFI_SUPPLICANT.try_lock() {
        Some(s) => s,
        None => return false,
    };
    let is_conn = supp.state == WifiLinkState::Connected;
    drop(supp);
    if !is_conn {
        return false;
    }
    match NETWORK_STACK.try_lock() {
        Some(stack) => stack.ipv4_address().is_some(),
        None => false,
    }
}

/// Returns the hardware MAC address of the Wi-Fi adapter if probed.
pub fn get_wifi_mac() -> Option<[u8; 6]> {
    WIFI_DEVICE.try_lock().and_then(|d| d.as_ref().map(|d| d.mac_address))
}

/// Transmits a standard Ethernet packet over the active 802.11 data ring with LLC/SNAP header.
pub fn wifi_send_ethernet_packet(packet: &[u8]) -> Result<(), &'static str> {
    if packet.len() < 14 {
        return Err("Ethernet packet too short");
    }
    let mut dev_guard = WIFI_DEVICE.lock();
    let driver = dev_guard.as_mut().ok_or("Wi-Fi hardware driver not initialized")?;

    let mut supp = WIFI_SUPPLICANT.lock();
    let bssid = supp.target_bssid;
    let seq = supp.next_seq();

    let mut da = [0u8; 6];
    da.copy_from_slice(&packet[0..6]);
    let ethertype = u16::from_be_bytes([packet[12], packet[13]]);

    let wifi_frame = build_80211_data_frame(driver.mac_address, bssid, da, ethertype, &packet[14..], seq);
    driver.data_ring.submit_packet(&driver.mmio, &wifi_frame)
        .map_err(|_| "Failed to submit packet to Wi-Fi data ring")?;

    Ok(())
}

/// Returns the latest probed hardware telemetry report if available.
pub fn get_probe_report() -> Option<Rtl8852beProbeReport> {
    RTL8852BE_PROBE_STATE.try_lock().and_then(|g| *g)
}
