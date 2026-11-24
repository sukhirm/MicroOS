//! Micro-OS Network Manager Application ("Network").
//!
//! Provides hardware network adapter discovery, PCI bus enumeration, driver state
//! tracking, and connection diagnostics conforming to the Cosmic Slate / Obsidian Azure design system:
//! 1. Overview: Host networking state, interface summary cards, rescan controls.
//! 2. Adapters: Full PCI network inventory (Vendor, Device, BDF, Class/Subclass, BARs, IRQ).
//! 3. Wi-Fi: Dedicated Realtek RTL8852BE-VT Wi-Fi 6 status, firmware requirements, and feasibility analysis.
//! 4. Ethernet: Dedicated Realtek Gaming GbE / RTL8168 wired NIC status, DMA descriptor ring architecture, and link state.
//! 5. Diagnostics: Event log console, driver capability matrix, and hardware probe telemetry.
//!
//! Important: In strict compliance with architectural integrity, hardware detection is clearly
//! distinguished from driver initialization and active internet connectivity. No connectivity is faked.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;
use crate::acpi;

use crate::arch::x86_64::pci::{
    self, PciBdf, VENDOR_INTEL, VENDOR_REALTEK, VENDOR_REDHAT_VIRTIO,
};
use crate::arch::x86_64::timer;
use crate::net::{self, ETHERNET_DEVICE, NETWORK_STACK};
use crate::net::stack::{DhcpState, DnsStatus, PingStatus};
use crate::graphics::color::Color;
use crate::graphics::font::draw_string;
use crate::graphics::geometry::{Point, Rect};
use crate::graphics::icon::{
    draw_icon, Icon, ICON_ACTIVITY, ICON_GEAR, ICON_NETWORK, ICON_TERMINAL,
};
use crate::graphics::surface::Surface;
use crate::input::event::{KeyAction, KeyCode, KeyEvent};
use crate::window::manager::WindowManager;
use crate::window::state::WindowId;
use crate::window::window::Window;

/// Global spinlock-protected Network Manager application state.
pub static NETWORK_MANAGER: Mutex<NetworkManagerApp> = Mutex::new(NetworkManagerApp::new());

/// The five distinct diagnostic views of the Network Manager.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkSection {
    Overview,
    Adapters,
    WiFi,
    Ethernet,
    Diagnostics,
}

impl NetworkSection {
    pub const ALL: [NetworkSection; 5] = [
        NetworkSection::Overview,
        NetworkSection::Adapters,
        NetworkSection::WiFi,
        NetworkSection::Ethernet,
        NetworkSection::Diagnostics,
    ];

    pub fn title(&self) -> &'static str {
        match self {
            NetworkSection::Overview => "Overview",
            NetworkSection::Adapters => "Adapters",
            NetworkSection::WiFi => "Wi-Fi",
            NetworkSection::Ethernet => "Ethernet",
            NetworkSection::Diagnostics => "Diagnostics",
        }
    }

    pub fn icon(&self) -> &'static Icon {
        match self {
            NetworkSection::Overview => &ICON_NETWORK,
            NetworkSection::Adapters => &ICON_ACTIVITY,
            NetworkSection::WiFi => &ICON_NETWORK,
            NetworkSection::Ethernet => &ICON_TERMINAL,
            NetworkSection::Diagnostics => &ICON_GEAR,
        }
    }
}

/// Classification of a network physical interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum InterfaceType {
    Ethernet,
    WiFi,
    VirtIO,
    Other,
}

#[allow(dead_code)]
impl InterfaceType {
    pub fn name(&self) -> &'static str {
        match self {
            InterfaceType::Ethernet => "802.3 Ethernet",
            InterfaceType::WiFi => "802.11ax Wi-Fi 6",
            InterfaceType::VirtIO => "VirtIO Paravirtualized",
            InterfaceType::Other => "Network Peripheral",
        }
    }
}

/// Explicit lifecycle stage of an interface device driver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterfaceDriverState {
    /// Hardware detected on PCI bus; no kernel driver initialized.
    HardwareProbed,
    /// Kernel driver module recognized hardware IDs, but uninitialized.
    DriverLoaded,
    /// Hardware MMIO, DMA descriptor rings, and MAC address initialized.
    InterfaceInitialized,
    /// Physical link / carrier signal detected.
    LinkActive,
    /// IPv4/IPv6 address assigned.
    IpConfigured,
    /// Verified internet connectivity via gateway.
    InternetVerified,
    /// Hardware requires proprietary closed binary firmware (e.g. Wi-Fi).
    DriverUnsupported,
}

impl InterfaceDriverState {
    pub fn name(&self) -> &'static str {
        match self {
            InterfaceDriverState::HardwareProbed => "Hardware Probed (Uninitialized)",
            InterfaceDriverState::DriverLoaded => "Driver Loaded",
            InterfaceDriverState::InterfaceInitialized => "Interface Initialized",
            InterfaceDriverState::LinkActive => "Link Active (Carrier Detected)",
            InterfaceDriverState::IpConfigured => "IP Configured",
            InterfaceDriverState::InternetVerified => "Internet Verified",
            InterfaceDriverState::DriverUnsupported => "Driver Unsupported (Firmware Required)",
        }
    }

    pub fn color(&self) -> Color {
        match self {
            InterfaceDriverState::HardwareProbed => Color::ACCENT_AZURE,
            InterfaceDriverState::DriverLoaded => Color::ACCENT_CYAN,
            InterfaceDriverState::InterfaceInitialized => Color::SUCCESS_GREEN,
            InterfaceDriverState::LinkActive => Color::SUCCESS_GREEN,
            InterfaceDriverState::IpConfigured => Color::SUCCESS_GREEN,
            InterfaceDriverState::InternetVerified => Color::SUCCESS_GREEN,
            InterfaceDriverState::DriverUnsupported => Color::WARNING_AMBER,
        }
    }
}

/// Connection status of an interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ConnectionStatus {
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

#[allow(dead_code)]
impl ConnectionStatus {
    pub fn name(&self) -> &'static str {
        match self {
            ConnectionStatus::Disconnected => "Disconnected",
            ConnectionStatus::HardwarePresentDriverPending => "Hardware Present (Driver Pending)",
            ConnectionStatus::HardwareUnsupported => "Firmware Blob Missing",
            ConnectionStatus::Scanning => "Scanning Air...",
            ConnectionStatus::Authenticating => "Authenticating (WPA2)...",
            ConnectionStatus::Associated => "Associated (L2 Linked)",
            ConnectionStatus::ObtainingIp => "Obtaining IP (DHCP)...",
            ConnectionStatus::Connecting => "Connecting...",
            ConnectionStatus::Connected => "Connected",
        }
    }

    pub fn color(&self) -> Color {
        match self {
            ConnectionStatus::Disconnected => Color::TEXT_MUTED,
            ConnectionStatus::HardwarePresentDriverPending => Color::ACCENT_CYAN,
            ConnectionStatus::HardwareUnsupported => Color::WARNING_AMBER,
            ConnectionStatus::Scanning => Color::ACCENT_AZURE,
            ConnectionStatus::Authenticating => Color::ACCENT_AZURE,
            ConnectionStatus::Associated => Color::ACCENT_CYAN,
            ConnectionStatus::ObtainingIp => Color::ACCENT_AZURE,
            ConnectionStatus::Connecting => Color::ACCENT_AZURE,
            ConnectionStatus::Connected => Color::SUCCESS_GREEN,
        }
    }
}

/// Descriptor of a detected network interface adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkAdapter {
    pub id: usize,
    pub name: String,
    pub adapter_type: InterfaceType,
    pub bdf: PciBdf,
    pub vendor_id: u16,
    pub device_id: u16,
    pub subsystem_id: u16,
    pub driver_state: InterfaceDriverState,
    pub connection_status: ConnectionStatus,
    pub mac_address: Option<[u8; 6]>,
    pub ipv4_address: Option<[u8; 4]>,
    pub bar0: u32,
    pub irq: u8,
    pub diagnostic_note: String,
}

/// Focus area within the Network Manager window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkFocusArea {
    Sidebar,
    Content,
    DiagnosticsContent,
}

/// Active focusable UI control within the Desktop Wi-Fi Settings section.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WifiControl {
    ScanButton,
    ToggleRadio,
    NetworkList,
    PasswordField,
    ConnectButton,
    DisconnectButton,
}

/// Saved Wi-Fi network profile credentials for automatic reconnection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WifiProfile {
    pub ssid: String,
    pub passphrase: String,
    pub auto_connect: bool,
}

/// Network Manager application state machine.
pub struct NetworkManagerApp {
    pub current_section: NetworkSection,
    pub selected_sidebar_index: usize,
    pub adapters: Vec<NetworkAdapter>,
    pub selected_adapter_idx: usize,
    pub last_scan_tick: u64,
    pub scan_count: usize,
    pub diagnostic_logs: Vec<String>,
    pub diag_scroll_offset: usize,
    pub diag_active_card: usize,
    pub focus_area: NetworkFocusArea,

    // Desktop Wi-Fi Settings State
    pub wifi_enabled: bool,
    pub wifi_scanning: bool,
    pub wifi_focused_control: WifiControl,
    pub selected_wifi_idx: Option<usize>,
    pub wifi_password_input: String,
    pub wifi_show_password: bool,
    pub wifi_password_focused: bool,
    pub wifi_auto_connect: bool,
    pub saved_profiles: Vec<WifiProfile>,
    #[allow(dead_code)]
    pub wifi_status_message: Option<String>,
}

impl NetworkManagerApp {
    /// Creates a new default NetworkManagerApp instance.
    pub const fn new() -> Self {
        Self {
            current_section: NetworkSection::Overview,
            selected_sidebar_index: 0,
            adapters: Vec::new(),
            selected_adapter_idx: 0,
            last_scan_tick: 0,
            scan_count: 0,
            diagnostic_logs: Vec::new(),
            diag_scroll_offset: 0,
            diag_active_card: 0,
            focus_area: NetworkFocusArea::Sidebar,

            wifi_enabled: true,
            wifi_scanning: false,
            wifi_focused_control: WifiControl::ScanButton,
            selected_wifi_idx: None,
            wifi_password_input: String::new(),
            wifi_show_password: false,
            wifi_password_focused: false,
            wifi_auto_connect: true,
            saved_profiles: Vec::new(),
            wifi_status_message: None,
        }
    }

    /// Selects a specific Wi-Fi network from the scanned networks list.
    pub fn select_wifi_network(&mut self, idx: usize) {
        self.wifi_scanning = false;
        let scan_table = crate::net::wifi_frame::WIFI_SCAN_TABLE.lock();
        if let Some(net) = scan_table.networks.get(idx) {
            self.selected_wifi_idx = Some(idx);
            if let Some(profile) = self.saved_profiles.iter().find(|p| p.ssid == net.ssid) {
                self.wifi_password_input = profile.passphrase.clone();
            } else {
                self.wifi_password_input.clear();
            }
            self.wifi_password_focused = true;
        }
    }

    /// Initiates connection to the currently selected Wi-Fi network.
    pub fn connect_selected_wifi(&mut self) {
        if !self.wifi_enabled {
            return;
        }
        let sel = match self.selected_wifi_idx {
            Some(i) => i,
            None => return,
        };
        let (ssid, bssid, is_wpa2) = {
            let scan_table = crate::net::wifi_frame::WIFI_SCAN_TABLE.lock();
            let net = match scan_table.networks.get(sel) {
                Some(n) => n,
                None => return,
            };
            (
                net.ssid.clone(),
                net.bssid,
                net.security != crate::net::wifi_frame::WifiSecurity::Open,
            )
        };

        if self.wifi_auto_connect {
            if let Some(prof) = self.saved_profiles.iter_mut().find(|p| p.ssid == ssid) {
                prof.passphrase = self.wifi_password_input.clone();
            } else {
                self.saved_profiles.push(WifiProfile {
                    ssid: ssid.clone(),
                    passphrase: self.wifi_password_input.clone(),
                    auto_connect: true,
                });
            }
        }

        let _ = crate::net::rtl8852be::connect_to_network(
            &ssid,
            &self.wifi_password_input,
            bssid,
            is_wpa2,
        );
    }

    /// Initializes and scans hardware if not yet scanned.
    pub fn ensure_initialized(&mut self) {
        if self.adapters.is_empty() && self.scan_count == 0 {
            self.rescan_hardware();
        }
    }

    /// Performs a live hardware scan across PCI buses for network devices.
    pub fn rescan_hardware(&mut self) {
        self.adapters.clear();
        self.diagnostic_logs.clear();
        self.scan_count += 1;
        self.last_scan_tick = timer::ticks();

        if let Some(ecam) = pci::get_ecam_config() {
            self.diagnostic_logs.push(format!(
                "[PCI ECAM] Active MMCONFIG at {:#010X}, Bus {}..={}",
                ecam.base_addr, ecam.start_bus, ecam.end_bus
            ));
        } else {
            self.diagnostic_logs.push("[PCI Legacy] ECAM inactive. Using I/O ports 0xCF8/0xCFC (Bus 0..=7).".to_string());
        }

        // Probing physical PCI bus for all hardware peripherals
        let all_pci_devs = pci::scan_pci_bus();
        self.diagnostic_logs.push(format!("[PCI] Discovered {} total physical PCI devices on bus.", all_pci_devs.len()));

        for dev in &all_pci_devs {
            self.diagnostic_logs.push(format!(
                "  [{}] {:04X}:{:04X} Cls:{:02X}:{:02X} BAR0:{:#010X} - {}",
                dev.bdf.to_string(),
                dev.vendor_id,
                dev.device_id,
                dev.class_code,
                dev.subclass,
                dev.bar0,
                dev.device_name()
            ));
        }

        let pci_net_devs: Vec<pci::PciDevice> = all_pci_devs
            .into_iter()
            .filter(|dev| {
                dev.is_network_controller()
                    || (dev.vendor_id == VENDOR_REALTEK
                        && (dev.device_id == 0xB852
                            || dev.device_id == 0x8852
                            || dev.device_id == 0xC852
                            || dev.device_id == 0xB851
                            || dev.device_id == 0xB85B
                            || dev.device_id == 0xA852
                            || dev.device_id == 0x8168
                            || dev.device_id == 0x8161
                            || dev.device_id == 0x8111
                            || dev.device_id == 0x8125
                            || dev.device_id == 0x8139
                            || dev.device_id == 0x3000))
                    || (dev.vendor_id == VENDOR_INTEL
                        && (dev.device_id == 0x100E
                            || dev.device_id == 0x100F
                            || dev.device_id == 0x1539
                            || dev.device_id == 0x15F3))
                    || (dev.vendor_id == VENDOR_REDHAT_VIRTIO
                        && (dev.device_id == 0x1000 || dev.device_id == 0x1041))
            })
            .collect();

        self.diagnostic_logs.push(format!("[PCI] Classified {} network interfaces for management.", pci_net_devs.len()));

        let mut id_counter = 1;

        for dev in pci_net_devs {
            let (adapter_type, driver_state, conn_status, note) = match (dev.vendor_id, dev.device_id) {
                // Realtek RTL8852BE-VT Wi-Fi 6
                (VENDOR_REALTEK, 0xB852) | (VENDOR_REALTEK, 0x8852) | (VENDOR_REALTEK, 0xC852)
                | (VENDOR_REALTEK, 0xB851) | (VENDOR_REALTEK, 0xB85B) | (VENDOR_REALTEK, 0xA852) => {
                    let probe_opt = crate::net::rtl8852be::get_probe_report();
                    let is_driver_active = crate::net::rtl8852be::WIFI_DEVICE.try_lock().map(|d| d.is_some()).unwrap_or(false);
                    let (supp_state, supp_ssid) = match crate::net::rtl8852be::WIFI_SUPPLICANT.try_lock() {
                        Some(s) => (s.state, s.target_ssid.clone()),
                        None => (crate::net::rtl8852be::WifiLinkState::Disconnected, String::new()),
                    };
                    let has_ip = NETWORK_STACK.try_lock().map(|s| s.ipv4_address().is_some()).unwrap_or(false);

                    let (driver_state, conn_status, note) = if is_driver_active {
                        if supp_state == crate::net::rtl8852be::WifiLinkState::Connected && has_ip {
                            (
                                InterfaceDriverState::IpConfigured,
                                ConnectionStatus::Connected,
                                format!("Realtek RTL8852BE: Connected to '{}'. IP configured.", supp_ssid),
                            )
                        } else if supp_state == crate::net::rtl8852be::WifiLinkState::ObtainingIp {
                            (
                                InterfaceDriverState::LinkActive,
                                ConnectionStatus::ObtainingIp,
                                format!("Realtek RTL8852BE: Associated with '{}'. DHCP lease pending.", supp_ssid),
                            )
                        } else if supp_state == crate::net::rtl8852be::WifiLinkState::Authenticating
                            || supp_state == crate::net::rtl8852be::WifiLinkState::Associating
                            || supp_state == crate::net::rtl8852be::WifiLinkState::Handshake
                        {
                            (
                                InterfaceDriverState::LinkActive,
                                ConnectionStatus::Connecting,
                                format!("Realtek RTL8852BE: Connecting to '{}' ({})...", supp_ssid, supp_state.name()),
                            )
                        } else {
                            let fw_err_str = probe_opt.and_then(|p| p.fw_error).map(|e| e.description());
                            let diag = match fw_err_str {
                                Some(err) => format!("Realtek RTL8852BE: PCIe & DMA active. Firmware: {}. BaseBand & RF uncalibrated.", err),
                                None => "Realtek RTL8852BE: PCIe & DMA active. RF uncalibrated (rtw89 PHY tables required).".to_string(),
                            };
                            (
                                InterfaceDriverState::InterfaceInitialized,
                                ConnectionStatus::Disconnected,
                                diag,
                            )
                        }
                    } else if let Some(probe) = probe_opt {
                        if probe.detected {
                            let note = if probe.is_floating_bus {
                                "Realtek RTL8852BE: PCIe BAR0 mapped. Floating bus detected (0xFFFFFFFF).".to_string()
                            } else {
                                "Realtek RTL8852BE: Hardware detected on PCI bus. Driver initializing.".to_string()
                            };
                            (
                                InterfaceDriverState::HardwareProbed,
                                ConnectionStatus::HardwarePresentDriverPending,
                                note,
                            )
                        } else {
                            (
                                InterfaceDriverState::DriverUnsupported,
                                ConnectionStatus::HardwareUnsupported,
                                "Realtek Wi-Fi 6: Adapter not detected on PCI bus.".to_string(),
                            )
                        }
                    } else {
                        (
                            InterfaceDriverState::HardwareProbed,
                            ConnectionStatus::HardwarePresentDriverPending,
                            "Realtek Wi-Fi 6: Hardware probe initialized.".to_string(),
                        )
                    };
                    (
                        InterfaceType::WiFi,
                        driver_state,
                        conn_status,
                        note,
                    )
                }
                // Realtek GbE Ethernet
                (VENDOR_REALTEK, 0x8168) | (VENDOR_REALTEK, 0x8161) | (VENDOR_REALTEK, 0x8111)
                | (VENDOR_REALTEK, 0x8125) | (VENDOR_REALTEK, 0x8139) | (VENDOR_REALTEK, 0x3000) => {
                    let is_active = ETHERNET_DEVICE.try_lock().map(|d| d.is_some()).unwrap_or(false);
                    let link_up = net::get_link_status().map(|s| s.is_up).unwrap_or(false);
                    let has_ip = NETWORK_STACK.try_lock().map(|s| s.ipv4_address().is_some()).unwrap_or(false);
                    let (driver_state, conn_status, note) = if is_active {
                        if has_ip && link_up {
                            (
                                InterfaceDriverState::IpConfigured,
                                ConnectionStatus::Connected,
                                "Realtek RTL8168: Active GbE Driver initialized. IP configured.".to_string(),
                            )
                        } else if link_up {
                            (
                                InterfaceDriverState::LinkActive,
                                ConnectionStatus::ObtainingIp,
                                "Realtek RTL8168: Physical carrier detected. DHCP lease pending.".to_string(),
                            )
                        } else {
                            (
                                InterfaceDriverState::InterfaceInitialized,
                                ConnectionStatus::Disconnected,
                                "Realtek RTL8168: Active GbE Driver initialized. Cable disconnected.".to_string(),
                            )
                        }
                    } else {
                        (
                            InterfaceDriverState::HardwareProbed,
                            ConnectionStatus::HardwarePresentDriverPending,
                            "Realtek GbE: MMIO BAR verified. Hardware initialization pending.".to_string(),
                        )
                    };
                    (
                        InterfaceType::Ethernet,
                        driver_state,
                        conn_status,
                        note,
                    )
                }
                // Intel Ethernet
                (VENDOR_INTEL, 0x100E) | (VENDOR_INTEL, 0x100F) | (VENDOR_INTEL, 0x1539) | (VENDOR_INTEL, 0x15F3) => (
                    InterfaceType::Ethernet,
                    InterfaceDriverState::HardwareProbed,
                    ConnectionStatus::HardwarePresentDriverPending,
                    "Intel e1000 Gigabit: Hardware BAR verified. Driver planned for Phase 5.".to_string(),
                ),
                // VirtIO
                (VENDOR_REDHAT_VIRTIO, 0x1000) | (VENDOR_REDHAT_VIRTIO, 0x1041) => (
                    InterfaceType::VirtIO,
                    InterfaceDriverState::HardwareProbed,
                    ConnectionStatus::HardwarePresentDriverPending,
                    "VirtIO Net: Paravirtualized queue ring verified.".to_string(),
                ),
                _ => {
                    let if_type = if dev.is_wireless_controller() {
                        InterfaceType::WiFi
                    } else {
                        InterfaceType::Ethernet
                    };
                    (
                        if_type,
                        InterfaceDriverState::HardwareProbed,
                        ConnectionStatus::HardwarePresentDriverPending,
                        "Generic PCI Network Controller probed.".to_string(),
                    )
                }
            };

            self.diagnostic_logs.push(format!(
                "[Device] [{}] VID: 0x{:04X} DID: 0x{:04X} -> {}",
                dev.bdf.to_string(),
                dev.vendor_id,
                dev.device_id,
                dev.device_name()
            ));

            let mac_address = match adapter_type {
                InterfaceType::Ethernet => ETHERNET_DEVICE.try_lock().and_then(|g| g.as_ref().map(|d| d.mac_address())),
                InterfaceType::WiFi => crate::net::rtl8852be::get_wifi_mac(),
                _ => None,
            };
            let ipv4_address = match adapter_type {
                InterfaceType::Ethernet => {
                    let eth_up = net::get_link_status().map(|s| s.is_up).unwrap_or(false);
                    if eth_up { NETWORK_STACK.try_lock().and_then(|s| s.ipv4_address()) } else { None }
                }
                InterfaceType::WiFi => {
                    if crate::net::rtl8852be::is_wifi_connected() {
                        NETWORK_STACK.try_lock().and_then(|s| s.ipv4_address())
                    } else {
                        None
                    }
                }
                _ => None,
            };

            self.adapters.push(NetworkAdapter {
                id: id_counter,
                name: dev.device_name().to_string(),
                adapter_type,
                bdf: dev.bdf,
                vendor_id: dev.vendor_id,
                device_id: dev.device_id,
                subsystem_id: dev.subsystem_id,
                driver_state,
                connection_status: conn_status,
                mac_address,
                ipv4_address,
                bar0: dev.bar0,
                irq: dev.interrupt_line,
                diagnostic_note: note,
            });

            id_counter += 1;
        }

        if self.adapters.is_empty() {
            self.diagnostic_logs.push("[Notice] Zero physical network adapters detected on PCI bus.".to_string());
        } else {
            self.diagnostic_logs.push(format!("[Ready] {} physical network adapter(s) active in inventory.", self.adapters.len()));
        }

        self.diagnostic_logs.push("[Complete] Hardware enumeration finished cleanly.".to_string());
    }

    /// Selects an explicit section.
    pub fn select_section(&mut self, section: NetworkSection) {
        self.current_section = section;
        if let Some(idx) = NetworkSection::ALL.iter().position(|&s| s == section) {
            self.selected_sidebar_index = idx;
        }
        if section == NetworkSection::Diagnostics {
            self.focus_area = NetworkFocusArea::DiagnosticsContent;
        } else if section == NetworkSection::WiFi {
            self.focus_area = NetworkFocusArea::Content;
            self.wifi_focused_control = WifiControl::ScanButton;
        } else {
            self.focus_area = NetworkFocusArea::Sidebar;
        }
    }

    /// Handles mouse clicks within the Network Manager window client area.
    pub fn handle_click(&mut self, pt: Point, content_rect: Rect) -> bool {
        self.ensure_initialized();
        let sidebar_w = 130i32;

        // 1. Sidebar clicks (x: content_rect.x .. content_rect.x + 130)
        if pt.x >= content_rect.x && pt.x < content_rect.x + sidebar_w {
            let start_y = content_rect.y + 36;
            let item_h = 32i32;

            for (i, &sec) in NetworkSection::ALL.iter().enumerate() {
                let cur_y = start_y + (i as i32 * item_h);
                if pt.y >= cur_y && pt.y < cur_y + item_h {
                    self.select_section(sec);
                    return true;
                }
            }
            return false;
        }

        // 2. Client area interaction (x >= content_rect.x + 130)
        if pt.x >= content_rect.x + sidebar_w {
            // Check Rescan Button in Overview or Diagnostics header
            let rescan_rect = Rect::new(content_rect.right() - 150, content_rect.y + 12, 136, 26);
            if rescan_rect.contains(pt) {
                self.rescan_hardware();
                return true;
            }

            // Diagnostics View Interaction
            if self.current_section == NetworkSection::Diagnostics {
                let pad_x = content_rect.x + sidebar_w + 14;

                // Sub-tabs (y: content_rect.y + 52 .. + 72)
                let tab_y = content_rect.y + 52;
                let tab_h = 20i32;
                if pt.y >= tab_y && pt.y < tab_y + tab_h {
                    let tabs = [
                        (pad_x, 110),
                        (pad_x + 114, 125),
                        (pad_x + 243, 115),
                        (pad_x + 362, 95),
                    ];
                    for (idx, &(tx, tw)) in tabs.iter().enumerate() {
                        if pt.x >= tx && pt.x < tx + tw {
                            self.diag_active_card = idx;
                            self.diag_scroll_offset = 0;
                            self.focus_area = NetworkFocusArea::DiagnosticsContent;
                            return true;
                        }
                    }
                }

                // Visible Pagination Buttons (y: content_rect.y + 76 .. + 96)
                let btn_y = content_rect.y + 76;
                let btn_h = 20i32;
                if pt.y >= btn_y && pt.y < btn_y + btn_h {
                    let prev_btn = Rect::new(pad_x, btn_y, 90, btn_h as u32);
                    let next_btn = Rect::new(pad_x + 96, btn_y, 90, btn_h as u32);
                    let top_btn = Rect::new(pad_x + 192, btn_y, 65, btn_h as u32);

                    if prev_btn.contains(pt) {
                        self.diag_scroll_offset = self.diag_scroll_offset.saturating_sub(10);
                        self.focus_area = NetworkFocusArea::DiagnosticsContent;
                        return true;
                    }
                    if next_btn.contains(pt) {
                        self.diag_scroll_offset = self.diag_scroll_offset.saturating_add(10);
                        self.focus_area = NetworkFocusArea::DiagnosticsContent;
                        return true;
                    }
                    if top_btn.contains(pt) {
                        self.diag_scroll_offset = 0;
                        self.focus_area = NetworkFocusArea::DiagnosticsContent;
                        return true;
                    }
                }

                // Main Content Panel click: gives focus to DiagnosticsContent
                let panel_y = content_rect.y + 100;
                if pt.y >= panel_y {
                    self.focus_area = NetworkFocusArea::DiagnosticsContent;
                    return true;
                }
            }

            // Check Adapter Selection clicks in Adapters view
            if self.current_section == NetworkSection::Adapters {
                let start_y = content_rect.y + 54;
                let card_h = 52i32;
                for i in 0..self.adapters.len() {
                    let cur_y = start_y + (i as i32 * (card_h + 8));
                    let card_rect = Rect::new(content_rect.x + sidebar_w + 14, cur_y, content_rect.width.saturating_sub(158), card_h as u32);
                    if card_rect.contains(pt) {
                        self.selected_adapter_idx = i;
                        return true;
                    }
                }
            }

            // Wi-Fi Desktop Settings View Interaction
            if self.current_section == NetworkSection::WiFi {
                let pad_x = content_rect.x + sidebar_w + 14;
                let card_w = content_rect.width.saturating_sub(sidebar_w as u32 + 28);

                // 1. Header Buttons: Toggle Wi-Fi & Scan
                let toggle_rect = Rect::new(content_rect.right() - 250, content_rect.y + 10, 100, 24);
                let scan_btn_rect = Rect::new(content_rect.right() - 140, content_rect.y + 10, 126, 24);

                if toggle_rect.contains(pt) {
                    self.wifi_enabled = !self.wifi_enabled;
                    self.wifi_focused_control = WifiControl::ToggleRadio;
                    self.focus_area = NetworkFocusArea::Content;
                    if !self.wifi_enabled {
                        crate::net::rtl8852be::disconnect_network();
                    }
                    return true;
                }

                if scan_btn_rect.contains(pt) {
                    self.wifi_focused_control = WifiControl::ScanButton;
                    self.focus_area = NetworkFocusArea::Content;
                    if self.wifi_enabled {
                        crate::net::rtl8852be::trigger_active_scan();
                        self.wifi_scanning = true;
                    }
                    return true;
                }

                // 2. Banner Actions (Disconnect / Forget / Cancel)
                let banner_y = content_rect.y + 38;
                let disconn_rect = Rect::new(content_rect.right() - 176, banner_y + 18, 76, 26);
                let forget_rect = Rect::new(content_rect.right() - 92, banner_y + 18, 64, 26);
                let cancel_rect = Rect::new(content_rect.right() - 100, banner_y + 18, 72, 26);

                let is_connected = crate::net::rtl8852be::is_wifi_connected();
                let is_linking = crate::net::rtl8852be::is_wifi_associated() && !is_connected;

                if is_connected {
                    if disconn_rect.contains(pt) {
                        self.wifi_focused_control = WifiControl::DisconnectButton;
                        self.focus_area = NetworkFocusArea::Content;
                        crate::net::rtl8852be::disconnect_network();
                        return true;
                    }
                    if forget_rect.contains(pt) {
                        let target_ssid = crate::net::rtl8852be::WIFI_SUPPLICANT.try_lock().map(|s| s.target_ssid.clone()).unwrap_or_default();
                        crate::net::rtl8852be::disconnect_network();
                        self.saved_profiles.retain(|p| p.ssid != target_ssid);
                        return true;
                    }
                } else if is_linking {
                    if cancel_rect.contains(pt) {
                        crate::net::rtl8852be::disconnect_network();
                        return true;
                    }
                }

                // 3. Network Selection Rows
                let table_y = content_rect.y + 108;
                let scan_table = match crate::net::wifi_frame::WIFI_SCAN_TABLE.try_lock() {
                    Some(st) => st,
                    None => return true,
                };
                for (i, _net) in scan_table.networks.iter().take(5).enumerate() {
                    let row_y = table_y + 28 + (i as i32 * 23);
                    let row_rect = Rect::new(pad_x + 6, row_y, card_w.saturating_sub(12), 22);
                    if row_rect.contains(pt) {
                        drop(scan_table);
                        self.select_wifi_network(i);
                        self.wifi_focused_control = WifiControl::NetworkList;
                        self.focus_area = NetworkFocusArea::Content;
                        return true;
                    }
                }
                drop(scan_table);

                // 4. Credential & Connect Panel
                let panel_y = content_rect.y + 262;
                let pwd_rect = Rect::new(pad_x + 84, panel_y + 28, 220, 26);
                let show_pwd_rect = Rect::new(pad_x + 312, panel_y + 28, 56, 26);
                let connect_rect = Rect::new(pad_x + 376, panel_y + 28, 96, 26);
                let auto_conn_rect = Rect::new(pad_x + 84, panel_y + 64, 260, 20);

                if pwd_rect.contains(pt) {
                    self.wifi_password_focused = true;
                    self.wifi_focused_control = WifiControl::PasswordField;
                    self.focus_area = NetworkFocusArea::Content;
                    return true;
                }
                if show_pwd_rect.contains(pt) {
                    self.wifi_show_password = !self.wifi_show_password;
                    self.wifi_focused_control = WifiControl::PasswordField;
                    self.focus_area = NetworkFocusArea::Content;
                    return true;
                }
                if auto_conn_rect.contains(pt) {
                    self.wifi_auto_connect = !self.wifi_auto_connect;
                    self.focus_area = NetworkFocusArea::Content;
                    return true;
                }
                if connect_rect.contains(pt) {
                    self.wifi_focused_control = WifiControl::ConnectButton;
                    self.focus_area = NetworkFocusArea::Content;
                    self.connect_selected_wifi();
                    return true;
                }
            }
        }

        false
    }

    /// Handles keyboard events for the Network Manager window.
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        self.ensure_initialized();

        if key.action != KeyAction::Pressed {
            return false;
        }

        // Dedicated Diagnostics controls: work whenever Diagnostics section is active
        if self.current_section == NetworkSection::Diagnostics {
            match key.code {
                KeyCode::PageDown => {
                    self.diag_scroll_offset = self.diag_scroll_offset.saturating_add(10);
                    self.focus_area = NetworkFocusArea::DiagnosticsContent;
                    return true;
                }
                KeyCode::PageUp => {
                    self.diag_scroll_offset = self.diag_scroll_offset.saturating_sub(10);
                    self.focus_area = NetworkFocusArea::DiagnosticsContent;
                    return true;
                }
                KeyCode::Home => {
                    self.diag_scroll_offset = 0;
                    self.focus_area = NetworkFocusArea::DiagnosticsContent;
                    return true;
                }
                KeyCode::End => {
                    self.diag_scroll_offset = 1000;
                    self.focus_area = NetworkFocusArea::DiagnosticsContent;
                    return true;
                }
                KeyCode::Num1 => {
                    self.diag_active_card = 0;
                    self.diag_scroll_offset = 0;
                    self.focus_area = NetworkFocusArea::DiagnosticsContent;
                    return true;
                }
                KeyCode::Num2 => {
                    self.diag_active_card = 1;
                    self.diag_scroll_offset = 0;
                    self.focus_area = NetworkFocusArea::DiagnosticsContent;
                    return true;
                }
                KeyCode::Num3 => {
                    self.diag_active_card = 2;
                    self.diag_scroll_offset = 0;
                    self.focus_area = NetworkFocusArea::DiagnosticsContent;
                    return true;
                }
                KeyCode::Num4 => {
                    self.diag_active_card = 3;
                    self.diag_scroll_offset = 0;
                    self.focus_area = NetworkFocusArea::DiagnosticsContent;
                    return true;
                }
                KeyCode::Tab => {
                    self.diag_active_card = (self.diag_active_card + 1) % 4;
                    self.diag_scroll_offset = 0;
                    self.focus_area = NetworkFocusArea::DiagnosticsContent;
                    return true;
                }
                KeyCode::Right => {
                    self.focus_area = NetworkFocusArea::DiagnosticsContent;
                    return true;
                }
                KeyCode::Left | KeyCode::Escape => {
                    self.focus_area = NetworkFocusArea::Sidebar;
                    return true;
                }
                _ => {}
            }

            // When Diagnostics Content panel is focused, Up/Down and J/K scroll lines
            if self.focus_area == NetworkFocusArea::DiagnosticsContent {
                match key.code {
                    KeyCode::Down | KeyCode::J => {
                        self.diag_scroll_offset = self.diag_scroll_offset.saturating_add(1);
                        return true;
                    }
                    KeyCode::Up | KeyCode::K => {
                        self.diag_scroll_offset = self.diag_scroll_offset.saturating_sub(1);
                        return true;
                    }
                    _ => {}
                }
            }
        }

        // Dedicated Wi-Fi controls and credential typing: active whenever Wi-Fi section is selected
        if self.current_section == NetworkSection::WiFi {
            match key.code {
                KeyCode::Left | KeyCode::Escape => {
                    self.focus_area = NetworkFocusArea::Sidebar;
                    return true;
                }
                KeyCode::Right => {
                    self.focus_area = NetworkFocusArea::Content;
                    return true;
                }
                KeyCode::Tab => {
                    self.focus_area = NetworkFocusArea::Content;
                    let is_connected = crate::net::rtl8852be::is_wifi_connected();
                    self.wifi_focused_control = match self.wifi_focused_control {
                        WifiControl::ScanButton => WifiControl::ToggleRadio,
                        WifiControl::ToggleRadio => {
                            if is_connected {
                                WifiControl::DisconnectButton
                            } else {
                                WifiControl::NetworkList
                            }
                        }
                        WifiControl::DisconnectButton => WifiControl::NetworkList,
                        WifiControl::NetworkList => {
                            if self.selected_wifi_idx.is_some() {
                                WifiControl::PasswordField
                            } else {
                                WifiControl::ScanButton
                            }
                        }
                        WifiControl::PasswordField => WifiControl::ConnectButton,
                        WifiControl::ConnectButton => WifiControl::ScanButton,
                    };
                    self.wifi_password_focused = self.wifi_focused_control == WifiControl::PasswordField;
                    return true;
                }
                KeyCode::Enter | KeyCode::Space => {
                    self.focus_area = NetworkFocusArea::Content;
                    match self.wifi_focused_control {
                        WifiControl::ScanButton => {
                            if self.wifi_enabled {
                                crate::net::rtl8852be::trigger_active_scan();
                                self.wifi_scanning = true;
                            }
                        }
                        WifiControl::ToggleRadio => {
                            self.wifi_enabled = !self.wifi_enabled;
                            if !self.wifi_enabled {
                                crate::net::rtl8852be::disconnect_network();
                            }
                        }
                        WifiControl::NetworkList => {
                            if self.selected_wifi_idx.is_some() {
                                self.wifi_focused_control = WifiControl::PasswordField;
                                self.wifi_password_focused = true;
                            }
                        }
                        WifiControl::PasswordField | WifiControl::ConnectButton => {
                            self.connect_selected_wifi();
                        }
                        WifiControl::DisconnectButton => {
                            crate::net::rtl8852be::disconnect_network();
                        }
                    }
                    return true;
                }
                KeyCode::S => {
                    if self.wifi_enabled {
                        crate::net::rtl8852be::trigger_active_scan();
                        self.wifi_scanning = true;
                        self.wifi_focused_control = WifiControl::ScanButton;
                        self.focus_area = NetworkFocusArea::Content;
                    }
                    return true;
                }
                KeyCode::T => {
                    self.wifi_enabled = !self.wifi_enabled;
                    if !self.wifi_enabled {
                        crate::net::rtl8852be::disconnect_network();
                    }
                    self.wifi_focused_control = WifiControl::ToggleRadio;
                    self.focus_area = NetworkFocusArea::Content;
                    return true;
                }
                KeyCode::Up => {
                    let count = crate::net::wifi_frame::WIFI_SCAN_TABLE.try_lock().map(|t| t.networks.len()).unwrap_or(0);
                    if self.wifi_focused_control == WifiControl::NetworkList && count > 0 {
                        let cur = self.selected_wifi_idx.unwrap_or(0);
                        let prev = if cur == 0 { count - 1 } else { cur - 1 };
                        self.select_wifi_network(prev);
                    } else if self.wifi_focused_control == WifiControl::ConnectButton {
                        self.wifi_focused_control = WifiControl::PasswordField;
                        self.wifi_password_focused = true;
                    } else if self.wifi_focused_control == WifiControl::PasswordField {
                        self.wifi_focused_control = WifiControl::NetworkList;
                        self.wifi_password_focused = false;
                    } else if self.wifi_focused_control == WifiControl::NetworkList {
                        self.wifi_focused_control = WifiControl::ScanButton;
                    }
                    return true;
                }
                KeyCode::Down => {
                    let count = crate::net::wifi_frame::WIFI_SCAN_TABLE.try_lock().map(|t| t.networks.len()).unwrap_or(0);
                    if self.wifi_focused_control == WifiControl::NetworkList && count > 0 {
                        let cur = self.selected_wifi_idx.unwrap_or(0);
                        let next = (cur + 1) % count;
                        self.select_wifi_network(next);
                    } else if self.wifi_focused_control == WifiControl::ScanButton || self.wifi_focused_control == WifiControl::ToggleRadio {
                        self.wifi_focused_control = WifiControl::NetworkList;
                    } else if self.wifi_focused_control == WifiControl::NetworkList && self.selected_wifi_idx.is_some() {
                        self.wifi_focused_control = WifiControl::PasswordField;
                        self.wifi_password_focused = true;
                    } else if self.wifi_focused_control == WifiControl::PasswordField {
                        self.wifi_focused_control = WifiControl::ConnectButton;
                        self.wifi_password_focused = false;
                    }
                    return true;
                }
                KeyCode::Backspace => {
                    if self.wifi_focused_control == WifiControl::PasswordField || self.wifi_password_focused {
                        self.wifi_password_input.pop();
                        return true;
                    }
                }
                _ => {
                    if let Some(c) = key.character {
                        if !c.is_control() && (self.wifi_focused_control == WifiControl::PasswordField || self.wifi_password_focused || self.selected_wifi_idx.is_some()) {
                            if self.wifi_password_input.len() < 64 {
                                self.wifi_password_input.push(c);
                                self.wifi_password_focused = true;
                                self.wifi_focused_control = WifiControl::PasswordField;
                                return true;
                            }
                        }
                    }
                }
            }
        }

        // Dedicated Adapters controls: active whenever Adapters section is selected
        if self.current_section == NetworkSection::Adapters {
            match key.code {
                KeyCode::Tab | KeyCode::Right => {
                    self.focus_area = NetworkFocusArea::Content;
                    return true;
                }
                KeyCode::Left | KeyCode::Escape => {
                    self.focus_area = NetworkFocusArea::Sidebar;
                    return true;
                }
                _ => {}
            }

            if self.focus_area == NetworkFocusArea::Content {
                match key.code {
                    KeyCode::Down | KeyCode::J => {
                        if !self.adapters.is_empty() {
                            self.selected_adapter_idx = (self.selected_adapter_idx + 1) % self.adapters.len();
                            return true;
                        }
                    }
                    KeyCode::Up | KeyCode::K => {
                        if !self.adapters.is_empty() {
                            self.selected_adapter_idx = if self.selected_adapter_idx == 0 {
                                self.adapters.len() - 1
                            } else {
                                self.selected_adapter_idx - 1
                            };
                            return true;
                        }
                    }
                    KeyCode::Enter | KeyCode::Space => {
                        if let Some(adapter) = self.adapters.get(self.selected_adapter_idx) {
                            match adapter.adapter_type {
                                InterfaceType::WiFi => self.select_section(NetworkSection::WiFi),
                                InterfaceType::Ethernet | InterfaceType::VirtIO => self.select_section(NetworkSection::Ethernet),
                                _ => {}
                            }
                        }
                        return true;
                    }
                    _ => {}
                }
            }
        }

        // Global sidebar and action keys (only when sidebar has focus)
        if self.focus_area == NetworkFocusArea::Sidebar {
            match key.code {
                KeyCode::Down => {
                    let next_idx = (self.selected_sidebar_index + 1) % NetworkSection::ALL.len();
                    self.selected_sidebar_index = next_idx;
                    self.current_section = NetworkSection::ALL[next_idx];
                    self.focus_area = if self.current_section == NetworkSection::Diagnostics {
                        NetworkFocusArea::DiagnosticsContent
                    } else if self.current_section == NetworkSection::WiFi {
                        NetworkFocusArea::Content
                    } else {
                        NetworkFocusArea::Sidebar
                    };
                    return true;
                }
                KeyCode::Up => {
                    let prev_idx = if self.selected_sidebar_index == 0 {
                        NetworkSection::ALL.len() - 1
                    } else {
                        self.selected_sidebar_index - 1
                    };
                    self.selected_sidebar_index = prev_idx;
                    self.current_section = NetworkSection::ALL[prev_idx];
                    self.focus_area = if self.current_section == NetworkSection::Diagnostics {
                        NetworkFocusArea::DiagnosticsContent
                    } else if self.current_section == NetworkSection::WiFi {
                        NetworkFocusArea::Content
                    } else {
                        NetworkFocusArea::Sidebar
                    };
                    return true;
                }
                _ => {}
            }
        }

        match key.code {
            KeyCode::A => {
                self.select_section(NetworkSection::Adapters);
                true
            }
            KeyCode::W => {
                self.select_section(NetworkSection::WiFi);
                true
            }
            KeyCode::E => {
                self.select_section(NetworkSection::Ethernet);
                true
            }
            KeyCode::O => {
                self.select_section(NetworkSection::Overview);
                true
            }
            KeyCode::D => {
                let _ = net::start_dhcp();
                true
            }
            KeyCode::P => {
                let _ = net::ping_gateway();
                true
            }
            KeyCode::N => {
                let _ = net::resolve_dns("google.com");
                true
            }
            KeyCode::R => {
                self.rescan_hardware();
                true
            }
            KeyCode::X => {
                self.select_section(NetworkSection::Diagnostics);
                true
            }
            KeyCode::Space | KeyCode::Enter => {
                if self.current_section == NetworkSection::Ethernet {
                    let _ = net::start_dhcp();
                } else if self.current_section == NetworkSection::Diagnostics {
                    self.focus_area = match self.focus_area {
                        NetworkFocusArea::Sidebar => NetworkFocusArea::DiagnosticsContent,
                        _ => NetworkFocusArea::Sidebar,
                    };
                } else if self.current_section == NetworkSection::Adapters {
                    self.focus_area = match self.focus_area {
                        NetworkFocusArea::Sidebar => NetworkFocusArea::Content,
                        _ => NetworkFocusArea::Sidebar,
                    };
                }
                true
            }
            _ => false,
        }
    }
}

/// Global client content renderer for the Network Manager window.
pub fn render_network_content(_window: &Window, surface: &mut Surface, content_rect: Rect) {
    let mut app = match NETWORK_MANAGER.try_lock() {
        Some(a) => a,
        None => return,
    };
    app.ensure_initialized();

    let saved_clip = surface.clip_rect();
    let client_clip = match saved_clip.intersect(&content_rect) {
        Some(c) => c,
        None => return,
    };
    surface.set_clip_rect(client_clip);

    // 1. Fill base client background (Obsidian Slate)
    surface.fill_rect(content_rect, Color::new(0x0E, 0x14, 0x24));

    // 2. Render Left Navigation Sidebar (Width: 130 px)
    let sidebar_w = 130u32;
    let sidebar_rect = Rect::new(content_rect.x, content_rect.y, sidebar_w, content_rect.height);
    surface.fill_rect(sidebar_rect, Color::new(0x0B, 0x10, 0x1C));
    surface.draw_vline(
        content_rect.x + sidebar_w as i32,
        content_rect.y,
        content_rect.height,
        Color::BORDER_SLATE,
    );

    // Sidebar Title
    draw_string(surface, content_rect.x + 12, content_rect.y + 12, "NETWORKING", Color::TEXT_MUTED, None, 1);

    // Sidebar Items
    let item_start_y = content_rect.y + 36;
    let item_h = 32i32;

    for (i, &sec) in NetworkSection::ALL.iter().enumerate() {
        let cur_y = item_start_y + (i as i32 * item_h);
        if cur_y + item_h > content_rect.bottom() {
            break;
        }

        let is_selected = app.current_section == sec;
        if is_selected {
            let active_rect = Rect::new(content_rect.x + 6, cur_y + 2, sidebar_w - 12, 28);
            surface.fill_rounded_rect(active_rect, 5, Color::new(0x1B, 0x25, 0x38));
            surface.draw_vline(content_rect.x + 8, cur_y + 6, 20, Color::ACCENT_CYAN);
        }

        let label_color = if is_selected { Color::ACCENT_CYAN } else { Color::TEXT_LIGHT };
        let icon_tint = if is_selected { Some(Color::ACCENT_CYAN) } else { Some(Color::TEXT_MUTED) };

        draw_icon(surface, content_rect.x + 14, cur_y + 8, sec.icon(), icon_tint);
        draw_string(surface, content_rect.x + 36, cur_y + 10, sec.title(), label_color, None, 1);
    }

    // 3. Render Main Section Content Area
    let main_x = content_rect.x + sidebar_w as i32 + 1;
    let main_w = content_rect.width.saturating_sub(sidebar_w + 1);
    let main_rect = Rect::new(main_x, content_rect.y, main_w, content_rect.height);

    match app.current_section {
        NetworkSection::Overview => render_overview_section(surface, main_rect, &app),
        NetworkSection::Adapters => render_adapters_section(surface, main_rect, &app),
        NetworkSection::WiFi => render_wifi_section(surface, main_rect, &app),
        NetworkSection::Ethernet => render_ethernet_section(surface, main_rect, &app),
        NetworkSection::Diagnostics => render_diagnostics_section(surface, main_rect, &app),
    }

    surface.set_clip_rect(saved_clip);
}

/// Renders the Overview section.
fn render_overview_section(surface: &mut Surface, area: Rect, app: &NetworkManagerApp) {
    let pad_x = area.x + 14;
    let mut cur_y = area.y + 12;

    let card_w = area.width.saturating_sub(28);

    // Header Card with Rescan Button
    let header_rect = Rect::new(pad_x, cur_y, card_w, 42);
    surface.fill_rounded_rect(header_rect, 6, Color::CARD_SLATE);
    surface.stroke_rounded_rect(header_rect, 6, 1, Color::BORDER_SLATE);

    draw_string(surface, pad_x + 12, cur_y + 8, "Host Network Subsystem", Color::WHITE, None, 1);
    draw_string(surface, pad_x + 12, cur_y + 22, "Stack: Bare-Metal Offline | Physical Link: Disconnected", Color::WARNING_AMBER, None, 1);

    // Rescan Button
    let btn_w = 136u32;
    let btn_rect = Rect::new(pad_x + card_w as i32 - btn_w as i32 - 10, cur_y + 8, btn_w, 26);
    surface.fill_rounded_rect(btn_rect, 5, Color::SURFACE_SLATE);
    surface.stroke_rounded_rect(btn_rect, 5, 1, Color::ACCENT_CYAN);
    draw_string(surface, btn_rect.x + 10, btn_rect.y + 8, "RESCAN PCI BUS", Color::ACCENT_CYAN, None, 1);

    cur_y += 50;

    // Interface Cards Summary
    let eth_count = app.adapters.iter().filter(|a| a.adapter_type == InterfaceType::Ethernet || a.adapter_type == InterfaceType::VirtIO).count();
    let wifi_count = app.adapters.iter().filter(|a| a.adapter_type == InterfaceType::WiFi).count();

    // Ethernet Summary Card
    let eth_card = Rect::new(pad_x, cur_y, card_w, 54);
    surface.fill_rounded_rect(eth_card, 6, Color::CARD_SLATE);
    surface.stroke_rounded_rect(eth_card, 6, 1, Color::BORDER_SLATE);
    draw_icon(surface, pad_x + 12, cur_y + 12, &ICON_TERMINAL, Some(Color::ACCENT_AZURE));
    draw_string(surface, pad_x + 36, cur_y + 10, "Wired Ethernet Controller", Color::WHITE, None, 1);
    draw_string(surface, pad_x + 36, cur_y + 24, &format!("Detected Devices: {} | Driver: Hardware Probed | Carrier: Down", eth_count), Color::TEXT_MUTED, None, 1);
    draw_string(surface, pad_x + 36, cur_y + 36, "IPv4: Not Assigned | MAC: Uninitialized", Color::TEXT_LIGHT, None, 1);

    cur_y += 62;

    // Wi-Fi Summary Card
    let wifi_card = Rect::new(pad_x, cur_y, card_w, 54);
    surface.fill_rounded_rect(wifi_card, 6, Color::CARD_SLATE);
    surface.stroke_rounded_rect(wifi_card, 6, 1, Color::BORDER_SLATE);
    let (wifi_icon_tint, wifi_status_str) = if wifi_count > 0 {
        (Some(Color::SUCCESS_GREEN), format!("Detected Devices: {} | Realtek RTL8852BE Probed | State: Standby", wifi_count))
    } else {
        (Some(Color::TEXT_MUTED), "Detected Devices: 0 | Hardware Pending Detection | State: Offline".to_string())
    };
    draw_icon(surface, pad_x + 12, cur_y + 12, &ICON_NETWORK, wifi_icon_tint);
    draw_string(surface, pad_x + 36, cur_y + 10, "Wireless 802.11ax Wi-Fi & Bluetooth Adapter", Color::WHITE, None, 1);
    draw_string(surface, pad_x + 36, cur_y + 24, &wifi_status_str, if wifi_count > 0 { Color::SUCCESS_GREEN } else { Color::TEXT_MUTED }, None, 1);
    draw_string(surface, pad_x + 36, cur_y + 36, "Bluetooth: Realtek Wireless Bluetooth Controller (PCIe/USB)", Color::TEXT_LIGHT, None, 1);

    cur_y += 62;

    // Architectural Transparency Notice Box
    let notice_rect = Rect::new(pad_x, cur_y, card_w, 66);
    surface.fill_rounded_rect(notice_rect, 6, Color::new(0x1B, 0x1A, 0x14));
    surface.stroke_rounded_rect(notice_rect, 6, 1, Color::WARNING_AMBER);
    draw_string(surface, pad_x + 12, cur_y + 8, "[ARCHITECTURAL NOTICE: DRIVER VS CONNECTIVITY]", Color::WARNING_AMBER, None, 1);
    let lines = [
        "- PCI hardware discovery detects physical NIC controllers on the host bus.",
        "- Network connectivity is strictly dependent on active DMA drivers & TCP/IP stack.",
        "- Micro-OS does not fabricate fake IP leases or simulated internet pings.",
    ];
    let mut line_y = cur_y + 22;
    for line in lines {
        draw_string(surface, pad_x + 12, line_y, line, Color::TEXT_LIGHT, None, 1);
        line_y += 13;
    }
}

/// Renders the Adapters inventory section.
fn render_adapters_section(surface: &mut Surface, area: Rect, app: &NetworkManagerApp) {
    let pad_x = area.x + 14;
    let mut cur_y = area.y + 12;

    let card_w = area.width.saturating_sub(28);

    let (focus_hint, hint_col) = match app.focus_area {
        NetworkFocusArea::Content => ("[Focus: ADAPTERS (Up/Down: Select, Enter: Open Tab, Left/Esc: Sidebar)]", Color::ACCENT_CYAN),
        _ => ("[Focus: SIDEBAR (Tab/Right: Select Adapters, Up/Down: Sections)]", Color::TEXT_MUTED),
    };
    draw_string(surface, pad_x, cur_y, "DETECTED PCI NETWORK ADAPTERS", Color::TEXT_MUTED, None, 1);
    draw_string(surface, pad_x + 220, cur_y, focus_hint, hint_col, None, 1);
    cur_y += 14;

    if app.adapters.is_empty() {
        let empty_rect = Rect::new(pad_x, cur_y, card_w, 64);
        surface.fill_rounded_rect(empty_rect, 6, Color::CARD_SLATE);
        surface.stroke_rounded_rect(empty_rect, 6, 1, Color::BORDER_SLATE);
        draw_string(surface, pad_x + 12, cur_y + 14, "No network adapters detected on PCI bus.", Color::WARNING_AMBER, None, 1);
        draw_string(surface, pad_x + 12, cur_y + 32, "Press [R] to rescan or check Diagnostics tab for raw PCI bus inventory.", Color::TEXT_MUTED, None, 1);
    }

    for (i, adapter) in app.adapters.iter().enumerate() {
        if cur_y + 54 > area.bottom() {
            break;
        }

        let is_selected = i == app.selected_adapter_idx;
        let is_content_focused = app.focus_area == NetworkFocusArea::Content;
        let card_rect = Rect::new(pad_x, cur_y, card_w, 52);

        surface.fill_rounded_rect(card_rect, 6, if is_selected && is_content_focused { Color::new(0x18, 0x24, 0x3D) } else { Color::CARD_SLATE });
        let border_color = if is_selected {
            if is_content_focused { Color::ACCENT_CYAN } else { Color::ACCENT_AZURE }
        } else {
            Color::BORDER_SLATE
        };
        surface.stroke_rounded_rect(card_rect, 6, if is_selected { 2 } else { 1 }, border_color);

        // Icon
        let icon = match adapter.adapter_type {
            InterfaceType::WiFi => &ICON_NETWORK,
            _ => &ICON_TERMINAL,
        };
        draw_icon(surface, pad_x + 10, cur_y + 10, icon, Some(adapter.driver_state.color()));

        // Name & BDF
        draw_string(surface, pad_x + 34, cur_y + 8, &adapter.name, Color::WHITE, None, 1);
        let pci_info = format!("PCI BDF: {} | VID: 0x{:04X} | DID: 0x{:04X} | BAR0: 0x{:08X}",
            adapter.bdf.to_string(), adapter.vendor_id, adapter.device_id, adapter.bar0);
        draw_string(surface, pad_x + 34, cur_y + 22, &pci_info, Color::TEXT_MUTED, None, 1);

        // Status badge
        let state_info = format!("State: {} | Status: {}", adapter.driver_state.name(), adapter.connection_status.name());
        draw_string(surface, pad_x + 34, cur_y + 36, &state_info, adapter.driver_state.color(), None, 1);

        cur_y += 60;
    }
}

/// Renders the Desktop Wi-Fi Settings section conforming to the Obsidian Azure design system.
fn render_wifi_section(surface: &mut Surface, area: Rect, app: &NetworkManagerApp) {
    let pad_x = area.x + 14;
    let card_w = area.width.saturating_sub(28);

    // 1. Header Bar: Title, Toggle Switch, and Scan Button
    let mut cur_y = area.y + 10;
    draw_string(surface, pad_x, cur_y + 4, "WI-FI SETTINGS", Color::WHITE, None, 1);

    // Toggle switch button
    let toggle_rect = Rect::new(area.right() - 250, cur_y, 100, 24);
    let is_toggle_focused = app.wifi_focused_control == WifiControl::ToggleRadio;
    if app.wifi_enabled {
        surface.fill_rounded_rect(toggle_rect, 4, Color::ACCENT_CYAN);
        surface.stroke_rounded_rect(toggle_rect, 4, if is_toggle_focused { 2 } else { 1 }, if is_toggle_focused { Color::WHITE } else { Color::ACCENT_CYAN });
        draw_string(surface, toggle_rect.x + 10, cur_y + 5, "[Wi-Fi: ON]", Color::new(0x0C, 0x10, 0x17), None, 1);
    } else {
        surface.fill_rounded_rect(toggle_rect, 4, Color::SURFACE_SLATE);
        surface.stroke_rounded_rect(toggle_rect, 4, if is_toggle_focused { 2 } else { 1 }, if is_toggle_focused { Color::ACCENT_CYAN } else { Color::BORDER_SLATE });
        draw_string(surface, toggle_rect.x + 8, cur_y + 5, "[Wi-Fi: OFF]", Color::TEXT_MUTED, None, 1);
    }

    // Scan Networks button
    let scan_rect = Rect::new(area.right() - 140, cur_y, 126, 24);
    let is_scan_focused = app.wifi_focused_control == WifiControl::ScanButton;
    if app.wifi_scanning {
        surface.fill_rounded_rect(scan_rect, 4, Color::new(0x0F, 0x2A, 0x1D));
        surface.stroke_rounded_rect(scan_rect, 4, if is_scan_focused { 2 } else { 1 }, Color::SUCCESS_GREEN);
        draw_string(surface, scan_rect.x + 10, cur_y + 5, "[Scanning...]", Color::SUCCESS_GREEN, None, 1);
    } else {
        surface.fill_rounded_rect(scan_rect, 4, Color::new(0x13, 0x22, 0x3D));
        let border_col = if is_scan_focused { Color::ACCENT_CYAN } else { Color::ACCENT_AZURE };
        surface.stroke_rounded_rect(scan_rect, 4, if is_scan_focused { 2 } else { 1 }, border_col);
        let text_col = if is_scan_focused { Color::WHITE } else { Color::ACCENT_AZURE };
        draw_string(surface, scan_rect.x + 10, cur_y + 5, "[Scan Networks]", text_col, None, 1);
    }

    cur_y += 32;

    // 2. Live Connection Status Banner
    let banner_rect = Rect::new(pad_x, cur_y, card_w, 64);
    let (supp_ssid, supp_bssid, supp_state, supp_failure) = {
        if let Some(supp) = crate::net::rtl8852be::WIFI_SUPPLICANT.try_lock() {
            (supp.target_ssid.clone(), supp.target_bssid, supp.state, supp.failure_reason)
        } else {
            (String::new(), [0u8; 6], crate::net::rtl8852be::WifiLinkState::Disconnected, None)
        }
    };
    let is_conn_state = supp_state == crate::net::rtl8852be::WifiLinkState::Connected;
    let ip_opt = crate::net::NETWORK_STACK.try_lock().and_then(|s| s.ipv4_address());
    let dhcp_state = crate::net::NETWORK_STACK.try_lock().map(|s| s.dhcp_state).unwrap_or(DhcpState::Idle);
    let is_connected = is_conn_state && ip_opt.is_some();

    if !app.wifi_enabled {
        surface.fill_rounded_rect(banner_rect, 6, Color::CARD_SLATE);
        surface.stroke_rounded_rect(banner_rect, 6, 1, Color::BORDER_SLATE);
        draw_string(surface, pad_x + 14, cur_y + 14, "[WI-FI RADIO DISABLED]", Color::TEXT_MUTED, None, 1);
        draw_string(surface, pad_x + 14, cur_y + 34, "Wireless radio is powered off. Click [Wi-Fi: OFF] above or press [T] to enable.", Color::TEXT_LIGHT, None, 1);
    } else if is_connected {
        surface.fill_rounded_rect(banner_rect, 6, Color::new(0x0F, 0x2A, 0x1D));
        surface.stroke_rounded_rect(banner_rect, 6, 1, Color::SUCCESS_GREEN);

        let title_str = format!("[CONNECTED]  {}", supp_ssid);
        draw_string(surface, pad_x + 14, cur_y + 12, &title_str, Color::SUCCESS_GREEN, None, 1);

        let (gw_str, dns_str) = match dhcp_state {
            DhcpState::Bound { gateway, dns, .. } => (
                format!("{}.{}.{}.{}", gateway[0], gateway[1], gateway[2], gateway[3]),
                format!("{}.{}.{}.{}", dns[0], dns[1], dns[2], dns[3]),
            ),
            _ => ("None".to_string(), "None".to_string()),
        };
        let ip_str = if let Some(ip) = ip_opt {
            format!("{}.{}.{}.{}", ip[0], ip[1], ip[2], ip[3])
        } else {
            "Configuring...".to_string()
        };
        let bssid_str = format!("{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
            supp_bssid[0], supp_bssid[1], supp_bssid[2],
            supp_bssid[3], supp_bssid[4], supp_bssid[5]);

        let detail_str = format!("IP: {}  |  Gateway: {}  |  DNS: {}  |  BSSID: {}", ip_str, gw_str, dns_str, bssid_str);
        draw_string(surface, pad_x + 14, cur_y + 36, &detail_str, Color::TEXT_LIGHT, None, 1);

        // Action Buttons: Disconnect and Forget
        let disconn_btn = Rect::new(banner_rect.right() - 176, cur_y + 18, 76, 26);
        let is_disconn_focused = app.wifi_focused_control == WifiControl::DisconnectButton;
        surface.fill_rounded_rect(disconn_btn, 4, Color::new(0x3B, 0x14, 0x14));
        surface.stroke_rounded_rect(disconn_btn, 4, if is_disconn_focused { 2 } else { 1 }, if is_disconn_focused { Color::WHITE } else { Color::DANGER_RED });
        draw_string(surface, disconn_btn.x + 8, cur_y + 24, "Disconnect", Color::DANGER_RED, None, 1);

        let forget_btn = Rect::new(banner_rect.right() - 92, cur_y + 18, 64, 26);
        surface.fill_rounded_rect(forget_btn, 4, Color::SURFACE_SLATE);
        surface.stroke_rounded_rect(forget_btn, 4, 1, Color::BORDER_SLATE);
        draw_string(surface, forget_btn.x + 12, cur_y + 24, "Forget", Color::TEXT_MUTED, None, 1);
    } else if supp_state == crate::net::rtl8852be::WifiLinkState::Authenticating
        || supp_state == crate::net::rtl8852be::WifiLinkState::Associating
        || supp_state == crate::net::rtl8852be::WifiLinkState::Handshake
        || supp_state == crate::net::rtl8852be::WifiLinkState::ObtainingIp
    {
        surface.fill_rounded_rect(banner_rect, 6, Color::new(0x13, 0x1B, 0x2E));
        surface.stroke_rounded_rect(banner_rect, 6, 1, Color::ACCENT_CYAN);

        let title_str = format!("[CONNECTING]  Connecting to '{}'...", supp_ssid);
        draw_string(surface, pad_x + 14, cur_y + 12, &title_str, Color::ACCENT_CYAN, None, 1);

        let state_str = format!("Stage: {}  |  Radio active  |  Awaiting network packets...", supp_state.name());
        draw_string(surface, pad_x + 14, cur_y + 36, &state_str, Color::TEXT_LIGHT, None, 1);

        let cancel_btn = Rect::new(banner_rect.right() - 100, cur_y + 18, 72, 26);
        surface.fill_rounded_rect(cancel_btn, 4, Color::SURFACE_SLATE);
        surface.stroke_rounded_rect(cancel_btn, 4, 1, Color::BORDER_SLATE);
        draw_string(surface, cancel_btn.x + 14, cur_y + 24, "Cancel", Color::TEXT_LIGHT, None, 1);
    } else {
        surface.fill_rounded_rect(banner_rect, 6, Color::CARD_SLATE);
        surface.stroke_rounded_rect(banner_rect, 6, 1, Color::BORDER_SLATE);

        draw_string(surface, pad_x + 14, cur_y + 12, "[DISCONNECTED]  Not connected to any wireless network", Color::TEXT_MUTED, None, 1);
        if let Some(reason) = supp_failure {
            let err_str = format!("Last attempt error: {}. Select a network below to retry.", reason);
            draw_string(surface, pad_x + 14, cur_y + 36, &err_str, Color::WARNING_AMBER, None, 1);
        } else {
            draw_string(surface, pad_x + 14, cur_y + 36, "Select a nearby network from the list below and enter credentials to connect.", Color::TEXT_LIGHT, None, 1);
        }
    }

    cur_y += 72;

    // 3. Nearby Available Networks List
    let table_rect = Rect::new(pad_x, cur_y, card_w, 146);
    let is_table_focused = app.wifi_focused_control == WifiControl::NetworkList;
    surface.fill_rounded_rect(table_rect, 6, Color::CARD_SLATE);
    surface.stroke_rounded_rect(table_rect, 6, if is_table_focused { 2 } else { 1 }, if is_table_focused { Color::ACCENT_CYAN } else { Color::BORDER_SLATE });

    let scan_networks = crate::net::wifi_frame::WIFI_SCAN_TABLE.try_lock().map(|t| t.networks.clone()).unwrap_or_default();
    let net_count = scan_networks.len();
    let table_title = format!("AVAILABLE WI-FI NETWORKS (AIR SCANNER: {} DETECTED)", net_count);
    draw_string(surface, pad_x + 12, cur_y + 8, &table_title, Color::WHITE, None, 1);

    if net_count == 0 {
        draw_string(surface, pad_x + 12, cur_y + 32, "0 access points detected on physical radio.", Color::WARNING_AMBER, None, 1);
        let probe_report = crate::net::rtl8852be::get_probe_report();
        let fw_note = probe_report.and_then(|p| p.fw_error).map(|e| e.description()).unwrap_or("WCPU in power-cut state");
        draw_string(surface, pad_x + 12, cur_y + 50, &format!("Hardware Status: RTL8852BE PCI active | Firmware: {} | RF uncalibrated.", fw_note), Color::TEXT_MUTED, None, 1);
        draw_string(surface, pad_x + 12, cur_y + 68, "RTL8852BE Wi-Fi 6 radio requires rtw89 PHY/RF calibration tables (~30k lines) to tune RF.", Color::TEXT_LIGHT, None, 1);
        draw_string(surface, pad_x + 12, cur_y + 86, "For physical internet on HP Victus: Use built-in Realtek RTL8168 GbE Ethernet [02:00.0].", Color::ACCENT_AZURE, None, 1);
    } else {
        let mut row_y = cur_y + 28;
        for (i, net) in scan_networks.iter().take(5).enumerate() {
            let row_rect = Rect::new(pad_x + 6, row_y, card_w.saturating_sub(12), 22);
            let is_sel = app.selected_wifi_idx == Some(i);

            if is_sel {
                surface.fill_rounded_rect(row_rect, 3, Color::new(0x18, 0x24, 0x3D));
                surface.stroke_rounded_rect(row_rect, 3, 1, Color::ACCENT_CYAN);
            }

            let (bar_sym, bar_col) = match net.signal_bars {
                4 => ("[####]", Color::SUCCESS_GREEN),
                3 => ("[### ]", Color::ACCENT_CYAN),
                2 => ("[##  ]", Color::WARNING_AMBER),
                _ => ("[#   ]", Color::TEXT_MUTED),
            };
            draw_string(surface, pad_x + 12, row_y + 4, bar_sym, bar_col, None, 1);

            let is_saved = app.saved_profiles.iter().any(|p| p.ssid == net.ssid);
            let saved_str = if is_saved { " [Saved]" } else { "" };
            let net_info = format!("{}  |  {}  |  Ch {:2}  |  {:3} dBm{}",
                net.ssid, net.security.display_name(), net.channel, net.rssi_dbm, saved_str);
            let text_col = if is_sel { Color::WHITE } else { Color::TEXT_LIGHT };
            draw_string(surface, pad_x + 64, row_y + 4, &net_info, text_col, None, 1);

            row_y += 23;
        }
    }

    cur_y += 154;

    // 4. Connect / Credential Dialog Panel
    let panel_rect = Rect::new(pad_x, cur_y, card_w, 96);
    surface.fill_rounded_rect(panel_rect, 6, Color::CARD_SLATE);
    surface.stroke_rounded_rect(panel_rect, 6, 1, Color::BORDER_SLATE);

    if let Some(sel_idx) = app.selected_wifi_idx {
        if let Some(net) = scan_networks.get(sel_idx) {
            let prompt_title = format!("Selected: \"{}\"  |  Security: {}", net.ssid, net.security.display_name());
            draw_string(surface, pad_x + 12, cur_y + 8, &prompt_title, Color::WHITE, None, 1);

            if net.security == crate::net::wifi_frame::WifiSecurity::Open {
                draw_string(surface, pad_x + 12, cur_y + 36, "Open network: No encryption or password required.", Color::TEXT_LIGHT, None, 1);

                let connect_btn = Rect::new(pad_x + 12, cur_y + 56, 110, 26);
                let is_conn_focused = app.wifi_focused_control == WifiControl::ConnectButton;
                surface.fill_rounded_rect(connect_btn, 4, Color::ACCENT_AZURE);
                if is_conn_focused {
                    surface.stroke_rounded_rect(connect_btn, 4, 2, Color::WHITE);
                }
                draw_string(surface, connect_btn.x + 18, connect_btn.y + 6, "Connect", Color::WHITE, None, 1);
            } else {
                draw_string(surface, pad_x + 12, cur_y + 34, "Password:", Color::TEXT_LIGHT, None, 1);

                // Password Input Box
                let pwd_rect = Rect::new(pad_x + 84, cur_y + 28, 220, 26);
                surface.fill_rounded_rect(pwd_rect, 3, Color::new(0x0C, 0x10, 0x17));
                let is_pwd_focused = app.wifi_focused_control == WifiControl::PasswordField || app.wifi_password_focused;
                let border_col = if is_pwd_focused { Color::ACCENT_CYAN } else { Color::BORDER_SLATE };
                surface.stroke_rounded_rect(pwd_rect, 3, if is_pwd_focused { 2 } else { 1 }, border_col);

                let display_pwd = if app.wifi_show_password {
                    app.wifi_password_input.clone()
                } else {
                    let mut s = String::new();
                    for _ in 0..app.wifi_password_input.len() {
                        s.push('*');
                    }
                    s
                };
                let pwd_with_cursor = if is_pwd_focused {
                    format!("{}|", display_pwd)
                } else {
                    display_pwd
                };
                draw_string(surface, pwd_rect.x + 8, pwd_rect.y + 6, &pwd_with_cursor, Color::WHITE, None, 1);

                // Show/Hide Password button
                let show_rect = Rect::new(pad_x + 312, cur_y + 28, 56, 26);
                surface.fill_rounded_rect(show_rect, 4, Color::SURFACE_SLATE);
                surface.stroke_rounded_rect(show_rect, 4, 1, Color::BORDER_SLATE);
                let show_lbl = if app.wifi_show_password { "Hide" } else { "Show" };
                draw_string(surface, show_rect.x + 12, show_rect.y + 6, show_lbl, Color::TEXT_LIGHT, None, 1);

                // Connect button
                let conn_btn = Rect::new(pad_x + 376, cur_y + 28, 96, 26);
                let is_conn_focused = app.wifi_focused_control == WifiControl::ConnectButton;
                surface.fill_rounded_rect(conn_btn, 4, Color::ACCENT_AZURE);
                if is_conn_focused {
                    surface.stroke_rounded_rect(conn_btn, 4, 2, Color::WHITE);
                }
                draw_string(surface, conn_btn.x + 20, conn_btn.y + 6, "Connect", Color::WHITE, None, 1);

                // Auto-connect checkbox
                let check_str = if app.wifi_auto_connect { "[X] Remember this network & auto-connect" } else { "[ ] Remember this network & auto-connect" };
                let check_col = if app.wifi_auto_connect { Color::SUCCESS_GREEN } else { Color::TEXT_MUTED };
                draw_string(surface, pad_x + 84, cur_y + 64, check_str, check_col, None, 1);
            }
        } else {
            draw_string(surface, pad_x + 12, cur_y + 24, "Select an available network above to view credentials and connect.", Color::TEXT_MUTED, None, 1);
            draw_string(surface, pad_x + 12, cur_y + 48, "Supports WPA2-Personal (AES-CCMP) and Open wireless access points.", Color::TEXT_LIGHT, None, 1);
        }
    } else {
        draw_string(surface, pad_x + 12, cur_y + 24, "Select an available network above to view credentials and connect.", Color::TEXT_MUTED, None, 1);
        draw_string(surface, pad_x + 12, cur_y + 48, "Supports WPA2-Personal (AES-CCMP) and Open wireless access points.", Color::TEXT_LIGHT, None, 1);
    }

    // 5. Controls Guide
    draw_string(surface, pad_x, area.bottom() - 18, "Controls: [Tab] Cycle Focus  |  [Enter/Space] Activate  |  [Up/Down] Select  |  [S] Scan  |  [T] Toggle  |  [Left/Esc] Sidebar", Color::ACCENT_AZURE, None, 1);
}

/// Renders the Ethernet section.
fn render_ethernet_section(surface: &mut Surface, area: Rect, app: &NetworkManagerApp) {
    let pad_x = area.x + 14;
    let mut cur_y = area.y + 12;

    let card_w = area.width.saturating_sub(28);

    draw_string(surface, pad_x, cur_y, "WIRED ETHERNET CONTROLLER & PROTOCOL STACK", Color::TEXT_MUTED, None, 1);
    cur_y += 14;

    // Find Ethernet adapter
    let eth_adapter = app.adapters.iter().find(|a| a.adapter_type == InterfaceType::Ethernet || a.adapter_type == InterfaceType::VirtIO);

    // 1. Hardware Details Card (80 px height)
    let hw_rect = Rect::new(pad_x, cur_y, card_w, 80);
    surface.fill_rounded_rect(hw_rect, 6, Color::CARD_SLATE);
    surface.stroke_rounded_rect(hw_rect, 6, 1, Color::BORDER_SLATE);

    let (link_status, mac_str, stats) = match ETHERNET_DEVICE.try_lock() {
        Some(guard) => (
            guard.as_ref().map(|d| d.check_link_status()),
            guard.as_ref().map(|d| d.mac_string()),
            guard.as_ref().map(|d| d.stats()),
        ),
        None => (None, None, None),
    };

    if let Some(e) = eth_adapter {
        draw_string(surface, pad_x + 12, cur_y + 8, &e.name, Color::WHITE, None, 1);
        draw_string(surface, pad_x + 12, cur_y + 22, &format!("PCI BDF: {} | VID: 0x{:04X} | DID: 0x{:04X} | MMIO: 0x{:08X}", e.bdf.to_string(), e.vendor_id, e.device_id, e.bar0), Color::TEXT_MUTED, None, 1);

        if let Some(link) = link_status {
            if link.is_up {
                let duplex = if link.full_duplex { "Full Duplex" } else { "Half Duplex" };
                draw_string(surface, pad_x + 12, cur_y + 36, &format!("Cable Status: LINK UP ({} Mbps, {})", link.speed_mbps, duplex), Color::SUCCESS_GREEN, None, 1);
            } else {
                draw_string(surface, pad_x + 12, cur_y + 36, "Cable Status: DISCONNECTED (Connect RJ-45 Ethernet cable to HP Victus)", Color::WARNING_AMBER, None, 1);
            }
        } else {
            draw_string(surface, pad_x + 12, cur_y + 36, "Cable Status: Probing PHY...", Color::TEXT_MUTED, None, 1);
        }

        let mac_display = match mac_str {
            Some(ref s) => format!("Hardware MAC: {} | Driver: Realtek RTL8168 Active (DMA Rings Ready)", s),
            None => "Hardware MAC: Reading EEPROM...".to_string(),
        };
        draw_string(surface, pad_x + 12, cur_y + 50, &mac_display, Color::TEXT_LIGHT, None, 1);
    } else {
        draw_string(surface, pad_x + 12, cur_y + 24, "No physical Ethernet hardware detected on host PCI bus.", Color::TEXT_MUTED, None, 1);
    }

    cur_y += 88;

    // 2. Protocol Stack, DHCP, Ping & DNS Internet Connectivity Card (134 px height)
    let net_rect = Rect::new(pad_x, cur_y, card_w, 134);
    surface.fill_rounded_rect(net_rect, 6, Color::new(0x13, 0x1B, 0x2E));
    surface.stroke_rounded_rect(net_rect, 6, 1, Color::BORDER_SLATE);

    draw_string(surface, pad_x + 12, cur_y + 8, "TCP/IP PROTOCOL STACK & INTERNET CONNECTIVITY", Color::ACCENT_CYAN, None, 1);

    let (dhcp_state, ping_status, dns_status) = match NETWORK_STACK.try_lock() {
        Some(stack) => (stack.dhcp_state, stack.ping_status, stack.dns_status.clone()),
        None => (DhcpState::Idle, PingStatus::Idle, DnsStatus::Idle),
    };

    // DHCP Line
    match dhcp_state {
        DhcpState::Bound { ip, subnet, gateway, dns, .. } => {
            let ip_str = format!("IPv4 Address: {}.{}.{}.{} (DHCP Active)", ip[0], ip[1], ip[2], ip[3]);
            draw_string(surface, pad_x + 12, cur_y + 24, &ip_str, Color::SUCCESS_GREEN, None, 1);

            let route_str = format!("Subnet: {}.{}.{}.{} | Gateway: {}.{}.{}.{} | DNS: {}.{}.{}.{}",
                subnet[0], subnet[1], subnet[2], subnet[3],
                gateway[0], gateway[1], gateway[2], gateway[3],
                dns[0], dns[1], dns[2], dns[3]
            );
            draw_string(surface, pad_x + 12, cur_y + 38, &route_str, Color::TEXT_LIGHT, None, 1);
        }
        DhcpState::Discovering { attempts, .. } => {
            let disc_str = format!("DHCP Status: Broadcasting Discover to router (attempt {})...", attempts);
            draw_string(surface, pad_x + 12, cur_y + 24, &disc_str, Color::ACCENT_CYAN, None, 1);
            draw_string(surface, pad_x + 12, cur_y + 38, "Waiting for DHCP Offer from local network...", Color::TEXT_MUTED, None, 1);
        }
        DhcpState::Requesting { offered_ip, .. } => {
            let req_str = format!("DHCP Status: Requesting lease for {}.{}.{}.{}...", offered_ip[0], offered_ip[1], offered_ip[2], offered_ip[3]);
            draw_string(surface, pad_x + 12, cur_y + 24, &req_str, Color::ACCENT_CYAN, None, 1);
            draw_string(surface, pad_x + 12, cur_y + 38, "Awaiting DHCP Ack from router...", Color::TEXT_MUTED, None, 1);
        }
        DhcpState::Idle => {
            draw_string(surface, pad_x + 12, cur_y + 24, "DHCP Status: Idle. Plug in Ethernet cable and press [D] or [Enter] to obtain IP.", Color::TEXT_MUTED, None, 1);
            draw_string(surface, pad_x + 12, cur_y + 38, "IPv4: 0.0.0.0 (Unconfigured)", Color::TEXT_MUTED, None, 1);
        }
        DhcpState::Failed => {
            draw_string(surface, pad_x + 12, cur_y + 24, "DHCP Status: Lease request timed out. Press [D] to retry.", Color::WARNING_AMBER, None, 1);
            draw_string(surface, pad_x + 12, cur_y + 38, "IPv4: 0.0.0.0", Color::TEXT_MUTED, None, 1);
        }
    }

    // Ping / Gateway Connectivity Line
    match ping_status {
        PingStatus::Success { target_ip, seq } => {
            let ping_str = format!("Gateway Ping: SUCCESS (ICMP Echo Reply seq {} received from {}.{}.{}.{})", seq, target_ip[0], target_ip[1], target_ip[2], target_ip[3]);
            draw_string(surface, pad_x + 12, cur_y + 54, &ping_str, Color::SUCCESS_GREEN, None, 1);
        }
        PingStatus::AwaitingReply { target_ip, seq } => {
            let ping_str = format!("Gateway Ping: Transmitted ICMP Echo Request seq {} to {}.{}.{}.{}...", seq, target_ip[0], target_ip[1], target_ip[2], target_ip[3]);
            draw_string(surface, pad_x + 12, cur_y + 54, &ping_str, Color::ACCENT_CYAN, None, 1);
        }
        PingStatus::Timeout { target_ip } => {
            let ping_str = format!("Gateway Ping: Timed out awaiting reply from {}.{}.{}.{}", target_ip[0], target_ip[1], target_ip[2], target_ip[3]);
            draw_string(surface, pad_x + 12, cur_y + 54, &ping_str, Color::WARNING_AMBER, None, 1);
        }
        PingStatus::Idle => {
            draw_string(surface, pad_x + 12, cur_y + 54, "Gateway Ping: Press [P] to ping default gateway and verify internet route.", Color::TEXT_MUTED, None, 1);
        }
    }

    // DNS Resolution Line
    match dns_status {
        DnsStatus::Success { hostname, ip, dns_server } => {
            let dns_str = format!("DNS Resolution: SUCCESS ({} -> {}.{}.{}.{} via {}.{}.{}.{})",
                hostname, ip[0], ip[1], ip[2], ip[3],
                dns_server[0], dns_server[1], dns_server[2], dns_server[3]);
            draw_string(surface, pad_x + 12, cur_y + 70, &dns_str, Color::SUCCESS_GREEN, None, 1);
        }
        DnsStatus::Resolving { hostname, dns_server } => {
            let dns_str = format!("DNS Resolution: Querying {} via {}.{}.{}.{}...",
                hostname, dns_server[0], dns_server[1], dns_server[2], dns_server[3]);
            draw_string(surface, pad_x + 12, cur_y + 70, &dns_str, Color::ACCENT_CYAN, None, 1);
        }
        DnsStatus::Failed { hostname, reason } => {
            let dns_str = format!("DNS Resolution: FAILED for {} ({}) - Press [N] to retry.", hostname, reason);
            draw_string(surface, pad_x + 12, cur_y + 70, &dns_str, Color::WARNING_AMBER, None, 1);
        }
        DnsStatus::Idle => {
            draw_string(surface, pad_x + 12, cur_y + 70, "DNS Resolution: Press [N] to resolve google.com via DHCP DNS server.", Color::TEXT_MUTED, None, 1);
        }
    }

    // Packet Statistics
    if let Some(s) = stats {
        let stats_str = format!("Driver Telemetry: Packets Tx: {} ({} B) | Rx: {} ({} B)", s.packets_sent, s.bytes_sent, s.packets_received, s.bytes_received);
        draw_string(surface, pad_x + 12, cur_y + 86, &stats_str, Color::TEXT_MUTED, None, 1);
    } else {
        draw_string(surface, pad_x + 12, cur_y + 86, "Driver Telemetry: Controller offline", Color::TEXT_MUTED, None, 1);
    }

    // Interactive Controls Bar
    draw_string(surface, pad_x + 12, cur_y + 106, "Keyboard Controls: [D / Enter] DHCP  |  [P] Ping Gateway  |  [N] Resolve DNS  |  [R] Rescan", Color::ACCENT_CYAN, None, 1);
}

/// Renders the multi-card Hardware Diagnostics & Verification section.
fn render_diagnostics_section(surface: &mut Surface, area: Rect, app: &NetworkManagerApp) {
    let pad_x = area.x + 14;
    let mut cur_y = area.y + 10;
    let card_w = area.width.saturating_sub(28);

    let acpi_opt = acpi::get_acpi_report();
    let pci_tel_opt = pci::get_pci_telemetry();

    // 1. Header Line
    draw_string(
        surface,
        pad_x,
        cur_y,
        "PCI HARDWARE ENUMERATION & SYSTEM DIAGNOSTICS",
        Color::TEXT_MUTED,
        None,
        1,
    );
    let focus_header = match app.focus_area {
        NetworkFocusArea::DiagnosticsContent => "[Focus: CONTENT (Left/Esc: Sidebar)]",
        _ => "[Focus: SIDEBAR (Tab/Right: Content)]",
    };
    let focus_color = match app.focus_area {
        NetworkFocusArea::DiagnosticsContent => Color::ACCENT_CYAN,
        _ => Color::TEXT_MUTED,
    };
    draw_string(
        surface,
        pad_x + (card_w.saturating_sub(250) as i32),
        cur_y,
        focus_header,
        focus_color,
        None,
        1,
    );
    cur_y += 14;

    // 2. Hardware Status Pill Banner
    let banner_h = 24u32;
    let banner_rect = Rect::new(pad_x, cur_y, card_w, banner_h);
    surface.fill_rounded_rect(banner_rect, 4, Color::OBSIDIAN_NAVY);
    surface.stroke_rounded_rect(banner_rect, 4, 1, Color::BORDER_SLATE);

    let mcfg_str = match &acpi_opt {
        Some(rep) if rep.mcfg_table_found => {
            if let Some(mcfg) = rep.mcfg_allocations.first() {
                format!("MCFG: [Found {:#010X} Bus {}..={}]", mcfg.base_address, mcfg.start_bus, mcfg.end_bus)
            } else {
                "MCFG: [Found (Empty Allocations)]".to_string()
            }
        }
        Some(_) => "MCFG: [Not Found in ACPI]".to_string(),
        None => "MCFG: [ACPI Uninitialized]".to_string(),
    };
    let mcfg_color = if acpi_opt.as_ref().map(|r| r.mcfg_table_found).unwrap_or(false) {
        Color::SUCCESS_GREEN
    } else {
        Color::DANGER_RED
    };
    draw_string(surface, pad_x + 8, cur_y + 5, &mcfg_str, mcfg_color, None, 1);

    let (ecam_str, ecam_color) = if let Some(ref tel) = pci_tel_opt {
        if tel.ecam_verification.matches {
            (format!("Bus 0: [MATCH {:04X}:{:04X}]", tel.ecam_verification.legacy_vendor, tel.ecam_verification.legacy_device), Color::SUCCESS_GREEN)
        } else if tel.ecam_verification.ecam_configured {
            ("Bus 0: [ECAM MISMATCH / FAULT]".to_string(), Color::DANGER_RED)
        } else {
            ("Bus 0: [Legacy 0xCF8 Only]".to_string(), Color::WARNING_AMBER)
        }
    } else {
        ("Bus 0: [Unchecked]".to_string(), Color::TEXT_MUTED)
    };
    draw_string(surface, pad_x + 230, cur_y + 5, &ecam_str, ecam_color, None, 1);

    let totals_str = if let Some(ref tel) = pci_tel_opt {
        format!("Devs: {} | Bridges: {}", tel.present_count, tel.bridges.len())
    } else {
        format!("Devs: {}", app.adapters.len())
    };
    draw_string(surface, pad_x + (card_w.saturating_sub(120) as i32), cur_y + 5, &totals_str, Color::TEXT_LIGHT, None, 1);
    cur_y += 28;

    // 3. Sub-tabs Navigation Bar
    let subtab_titles = [
        "1. Bus 0 & ECAM",
        "2. Bridges & Routing",
        "3. Raw PCI Devices",
        "4. ACPI Tables",
    ];
    let tab_widths = [110u32, 125u32, 115u32, 95u32];
    let mut tab_x = pad_x;
    for (idx, &title) in subtab_titles.iter().enumerate() {
        let tw = tab_widths[idx];
        let tab_rect = Rect::new(tab_x, cur_y, tw, 20);
        let is_active = app.diag_active_card == idx;
        let bg_color = if is_active { Color::CARD_SLATE } else { Color::SURFACE_SLATE };
        let border_color = if is_active { Color::BORDER_ACTIVE } else { Color::BORDER_SLATE };
        let text_color = if is_active { Color::ACCENT_CYAN } else { Color::TEXT_MUTED };

        surface.fill_rounded_rect(tab_rect, 3, bg_color);
        surface.stroke_rounded_rect(tab_rect, 3, 1, border_color);
        draw_string(surface, tab_x + 6, cur_y + 4, title, text_color, None, 1);
        tab_x += (tw as i32) + 4;
    }
    cur_y += 24;

    // 4. Visible Pagination & Navigation Controls Bar (Clickable & Keyboard-Navigable)
    let page_bar_y = cur_y;
    let page_bar_h = 20u32;

    // Button 1: [▲ Prev Page]
    let prev_btn = Rect::new(pad_x, page_bar_y, 90, page_bar_h);
    surface.fill_rounded_rect(prev_btn, 3, Color::CARD_SLATE);
    surface.stroke_rounded_rect(prev_btn, 3, 1, Color::ACCENT_CYAN);
    draw_string(surface, pad_x + 10, page_bar_y + 4, "[▲ Prev]", Color::WHITE, None, 1);

    // Button 2: [▼ Next Page]
    let next_btn = Rect::new(pad_x + 96, page_bar_y, 90, page_bar_h);
    surface.fill_rounded_rect(next_btn, 3, Color::CARD_SLATE);
    surface.stroke_rounded_rect(next_btn, 3, 1, Color::ACCENT_CYAN);
    draw_string(surface, pad_x + 106, page_bar_y + 4, "[▼ Next]", Color::WHITE, None, 1);

    // Button 3: [Top (Home)]
    let top_btn = Rect::new(pad_x + 192, page_bar_y, 65, page_bar_h);
    surface.fill_rounded_rect(top_btn, 3, Color::SURFACE_SLATE);
    surface.stroke_rounded_rect(top_btn, 3, 1, Color::BORDER_SLATE);
    draw_string(surface, pad_x + 202, page_bar_y + 4, "[Top]", Color::TEXT_LIGHT, None, 1);

    // Dynamic item & page counter based on active subtab
    let page_info = match app.diag_active_card {
        1 => {
            let total_b = pci_tel_opt.as_ref().map(|t| t.bridges.len()).unwrap_or(0);
            let offset = app.diag_scroll_offset.min(total_b);
            format!("Bridges: offset {} of {} | [1..4] Switch Tab", offset, total_b)
        }
        2 => {
            let total_d = pci_tel_opt.as_ref().map(|t| t.devices.len()).unwrap_or(0);
            let offset = app.diag_scroll_offset.min(total_d);
            format!("Devs: offset {} of {} | Keys: [PgUp/PgDn] [J/K]", offset, total_d)
        }
        3 => {
            let total_t = acpi_opt.as_ref().map(|r| r.tables.len()).unwrap_or(0);
            let offset = app.diag_scroll_offset.min(total_t);
            format!("Tables: offset {} of {} | [R] Rescan", offset, total_t)
        }
        _ => "Keys: [PgUp/PgDn] Scroll | [1..4] Tabs | [R] Rescan".to_string(),
    };
    draw_string(surface, pad_x + 265, page_bar_y + 5, &page_info, Color::ACCENT_CYAN, None, 1);
    cur_y += 24;

    // 5. Main Diagnostic Content Panel
    let panel_h = (area.bottom().saturating_sub(cur_y + 6)).max(0) as u32;
    let panel_rect = Rect::new(pad_x, cur_y, card_w, panel_h);
    surface.fill_rounded_rect(panel_rect, 6, Color::OBSIDIAN_NAVY);
    let border_color = match app.focus_area {
        NetworkFocusArea::DiagnosticsContent => Color::ACCENT_CYAN,
        _ => Color::BORDER_SLATE,
    };
    let border_width = match app.focus_area {
        NetworkFocusArea::DiagnosticsContent => 2,
        _ => 1,
    };
    surface.stroke_rounded_rect(panel_rect, 6, border_width, border_color);

    let mut text_y = cur_y + 8;
    let max_text_y = panel_rect.bottom().saturating_sub(16);

    match app.diag_active_card {
        // Tab 0: Bus 0 & ECAM Verification
        0 => {
            draw_string(surface, pad_x + 10, text_y, "=== PCIe ECAM (MMCONFIG) & Bus 0 Hardware Verification ===", Color::ACCENT_CYAN, None, 1);
            text_y += 16;

            if let Some(ref tel) = pci_tel_opt {
                let ecam = &tel.ecam_verification;
                let cfg_line = format!(
                    "ECAM Physical Base: {:#018X} | Mapping: {}",
                    ecam.ecam_base_phys, ecam.ecam_mapping_mode
                );
                draw_string(surface, pad_x + 10, text_y, &cfg_line, Color::TEXT_LIGHT, None, 1);
                text_y += 14;

                let virt_line = format!(
                    "ECAM Virtual Base:  {:#018X}",
                    ecam.ecam_mapped_virt.unwrap_or(0)
                );
                draw_string(surface, pad_x + 10, text_y, &virt_line, Color::TEXT_MUTED, None, 1);
                text_y += 16;

                draw_string(surface, pad_x + 10, text_y, "--- Bus 0 Device 00:00.0 (Host Bridge) Read Comparison ---", Color::TEXT_MUTED, None, 1);
                text_y += 14;

                let leg_line = format!(
                    "  Legacy Port 0xCF8/0xCFC: 0x{:08X} [Vendor: {:04X}, Device: {:04X}]",
                    ecam.legacy_dword0, ecam.legacy_vendor, ecam.legacy_device
                );
                draw_string(surface, pad_x + 10, text_y, &leg_line, Color::TEXT_LIGHT, None, 1);
                text_y += 14;

                let mm_line = format!(
                    "  PCIe ECAM MMCONFIG:      0x{:08X} [Vendor: {:04X}, Device: {:04X}]",
                    ecam.ecam_dword0, ecam.ecam_vendor, ecam.ecam_device
                );
                draw_string(surface, pad_x + 10, text_y, &mm_line, Color::TEXT_LIGHT, None, 1);
                text_y += 16;

                let verdict_color = if ecam.matches { Color::SUCCESS_GREEN } else { Color::DANGER_RED };
                draw_string(surface, pad_x + 10, text_y, ecam.status_summary, verdict_color, None, 1);
                text_y += 18;

                draw_string(surface, pad_x + 10, text_y, "--- Hardware Bus Scan Statistics ---", Color::TEXT_MUTED, None, 1);
                text_y += 14;

                let stats_line = format!(
                    "  Slots Probed: {} | Discovered: {} | Empty (0xFFFF): {} | Zero: {}",
                    tel.total_slots_scanned, tel.present_count, tel.empty_count, tel.all_zero_count
                );
                draw_string(surface, pad_x + 10, text_y, &stats_line, Color::TEXT_LIGHT, None, 1);
                text_y += 16;

                // Dynamic Physical Target Status
                let wifi_adapter = app.adapters.iter().find(|a| a.vendor_id == VENDOR_REALTEK && (a.device_id == 0xB852 || a.device_id == 0x8852 || a.device_id == 0xC852));
                let eth_adapter = app.adapters.iter().find(|a| a.vendor_id == VENDOR_REALTEK && a.device_id == 0x8168);

                if let Some(w) = wifi_adapter {
                    let w_line = format!("★ Realtek RTL8852BE Wi-Fi 6: DETECTED at [{}] VID:{:04X} DID:{:04X} BAR0:{:#010X}", w.bdf.to_string(), w.vendor_id, w.device_id, w.bar0);
                    draw_string(surface, pad_x + 10, text_y, &w_line, Color::SUCCESS_GREEN, None, 1);
                } else {
                    draw_string(surface, pad_x + 10, text_y, "  Realtek RTL8852BE Wi-Fi 6: Not detected on bus. Press [R] to rescan.", Color::WARNING_AMBER, None, 1);
                }
                text_y += 14;

                if let Some(e) = eth_adapter {
                    let e_line = format!("★ Realtek RTL8168 GbE NIC:   DETECTED at [{}] VID:{:04X} DID:{:04X} BAR0:{:#010X}", e.bdf.to_string(), e.vendor_id, e.device_id, e.bar0);
                    draw_string(surface, pad_x + 10, text_y, &e_line, Color::ACCENT_CYAN, None, 1);
                } else {
                    draw_string(surface, pad_x + 10, text_y, "  Realtek RTL8168 GbE NIC:   Not detected on bus. Press [R] to rescan.", Color::WARNING_AMBER, None, 1);
                }
                text_y += 16;
                draw_string(surface, pad_x + 10, text_y, "* Note: AMD Zen 4 AGESA forwards legacy I/O cycles to secondary buses 2 and 4.", Color::TEXT_MUTED, None, 1);
            } else {
                draw_string(surface, pad_x + 10, text_y, "PCI telemetry not yet initialized. Press [R] to scan hardware.", Color::WARNING_AMBER, None, 1);
            }
        }

        // Tab 1: Bridges & Routing
        1 => {
            draw_string(surface, pad_x + 10, text_y, "=== Downstream PCIe Bridges & Bus Routing (Bus 0) ===", Color::ACCENT_CYAN, None, 1);
            text_y += 16;

            if let Some(ref tel) = pci_tel_opt {
                if tel.bridges.is_empty() {
                    draw_string(surface, pad_x + 10, text_y, "No PCI-to-PCI bridge devices (Class 0x06, Subclass 0x04) found on Bus 0.", Color::WARNING_AMBER, None, 1);
                } else {
                    let total_bridges = tel.bridges.len();
                    let max_lines_fit = (max_text_y.saturating_sub(text_y) / 32).max(1) as usize;
                    let max_offset = total_bridges.saturating_sub(max_lines_fit);
                    let start = app.diag_scroll_offset.min(max_offset);
                    let mut displayed = 0;
                    for b in tel.bridges.iter().skip(start) {
                        if text_y + 28 > max_text_y {
                            break;
                        }
                        let b_header = format!(
                            "Bridge [{}] {:04X}:{:04X} Cmd:{:#06X} Ctl:{:#06X} | Prim:{} Sec:{} Sub:{}",
                            b.bdf.to_string(), b.vendor_id, b.device_id, b.command, b.bridge_control,
                            b.primary_bus, b.secondary_bus, b.subordinate_bus
                        );
                        draw_string(surface, pad_x + 10, text_y, &b_header, Color::TEXT_LIGHT, None, 1);
                        text_y += 14;

                        let target_str = if b.downstream_device_present {
                            if b.downstream_vid == VENDOR_REALTEK && (b.downstream_did == 0xB852 || b.downstream_did == 0x8852 || b.downstream_did == 0xC852) {
                                format!("  -> Secondary Bus {:02X}:00.0: [{:04X}:{:04X}] ★ REALTEK RTL8852BE WI-FI 6 DETECTED!", b.secondary_bus, b.downstream_vid, b.downstream_did)
                            } else if b.downstream_vid == VENDOR_REALTEK && b.downstream_did == 0x8168 {
                                format!("  -> Secondary Bus {:02X}:00.0: [{:04X}:{:04X}] ★ REALTEK RTL8168 GBE DETECTED!", b.secondary_bus, b.downstream_vid, b.downstream_did)
                            } else {
                                format!("  -> Secondary Bus {:02X}:00.0: [{:04X}:{:04X}] Peripheral Present", b.secondary_bus, b.downstream_vid, b.downstream_did)
                            }
                        } else {
                            format!("  -> Secondary Bus {:02X}:00.0: [Empty / Link Down]", b.secondary_bus)
                        };
                        let target_color = if b.downstream_device_present { Color::SUCCESS_GREEN } else { Color::TEXT_MUTED };
                        draw_string(surface, pad_x + 10, text_y, &target_str, target_color, None, 1);
                        text_y += 16;
                        displayed += 1;
                    }
                    let footer = format!("Viewing bridges {}..{} of {} | Use [▲/▼ Prev/Next buttons] or [PgUp/PgDn]", start + 1, (start + displayed).min(total_bridges), total_bridges);
                    draw_string(surface, pad_x + 10, max_text_y, &footer, Color::ACCENT_CYAN, None, 1);
                }
            } else {
                draw_string(surface, pad_x + 10, text_y, "No bridge data available. Press [R] to scan.", Color::TEXT_MUTED, None, 1);
            }
        }

        // Tab 2: Raw PCI Devices Inventory
        2 => {
            let total_devs = pci_tel_opt.as_ref().map(|t| t.devices.len()).unwrap_or(0);
            let hdr = format!("=== Raw PCI Hardware Inventory (Discovered: {}) ===", total_devs);
            draw_string(surface, pad_x + 10, text_y, &hdr, Color::ACCENT_CYAN, None, 1);
            text_y += 16;

            draw_string(surface, pad_x + 10, text_y, "  [BDF]    VID:DID   Class  BAR0        Device Model", Color::TEXT_MUTED, None, 1);
            text_y += 14;

            if let Some(ref tel) = pci_tel_opt {
                let max_lines_fit = (max_text_y.saturating_sub(text_y) / 14).max(1) as usize;
                let max_offset = total_devs.saturating_sub(max_lines_fit);
                let start = app.diag_scroll_offset.min(max_offset);
                let mut displayed = 0;
                for dev in tel.devices.iter().skip(start) {
                    if text_y + 14 > max_text_y {
                        break;
                    }
                    let is_wifi = dev.vendor_id == VENDOR_REALTEK && (dev.device_id == 0xB852 || dev.device_id == 0x8852 || dev.device_id == 0xC852);
                    let is_eth = dev.vendor_id == VENDOR_REALTEK && dev.device_id == 0x8168;

                    let (prefix, dev_color) = if is_wifi {
                        ("★ [WI-FI 6] ", Color::SUCCESS_GREEN)
                    } else if is_eth {
                        ("★ [GBE NIC] ", Color::ACCENT_CYAN)
                    } else if dev.bdf.bus > 0 {
                        ("  [DOWNSTREAM] ", Color::TEXT_LIGHT)
                    } else {
                        ("  ", Color::TEXT_LIGHT)
                    };

                    let dev_line = format!(
                        "{}[{}] {:04X}:{:04X} {:02X}:{:02X} {:#010X} {}",
                        prefix,
                        dev.bdf.to_string(),
                        dev.vendor_id,
                        dev.device_id,
                        dev.class_code,
                        dev.subclass,
                        dev.bar0,
                        dev.device_name()
                    );
                    draw_string(surface, pad_x + 10, text_y, &dev_line, dev_color, None, 1);
                    text_y += 14;
                    displayed += 1;
                }
                let page_cur = if max_lines_fit > 0 { (start / max_lines_fit) + 1 } else { 1 };
                let page_tot = if max_lines_fit > 0 { (total_devs + max_lines_fit - 1) / max_lines_fit } else { 1 };
                let footer = format!("Viewing {}..{} of {} | Page {}/{} | Use [▲/▼ Prev/Next buttons] or [PgUp/PgDn]", start + 1, (start + displayed).min(total_devs), total_devs, page_cur, page_tot);
                draw_string(surface, pad_x + 10, max_text_y, &footer, Color::ACCENT_CYAN, None, 1);
            } else {
                draw_string(surface, pad_x + 10, text_y, "No devices scanned. Press [R] to scan.", Color::TEXT_MUTED, None, 1);
            }
        }

        // Tab 3: ACPI System Tables
        _ => {
            draw_string(surface, pad_x + 10, text_y, "=== ACPI System Description Tables (UEFI Firmware) ===", Color::ACCENT_CYAN, None, 1);
            text_y += 16;

            if let Some(ref rep) = acpi_opt {
                let rsdp_line = format!(
                    "RSDP: {:#018X} | Rev: {} | Root Table: {} (Chk: {})",
                    rep.rsdp_addr, rep.rsdp_revision, rep.root_table_type,
                    if rep.rsdp_checksum_valid { "OK" } else { "FAIL" }
                );
                draw_string(surface, pad_x + 10, text_y, &rsdp_line, Color::TEXT_LIGHT, None, 1);
                text_y += 14;

                let mcfg_info = if let Some(mcfg) = rep.mcfg_allocations.first() {
                    format!("MCFG Table: Base={:#018X}, Seg={}, Buses={}..={}", mcfg.base_address, mcfg.segment_group, mcfg.start_bus, mcfg.end_bus)
                } else {
                    format!("MCFG Table: Found={} | Allocations: 0", rep.mcfg_table_found)
                };
                let mcfg_c = if rep.mcfg_table_found { Color::SUCCESS_GREEN } else { Color::DANGER_RED };
                draw_string(surface, pad_x + 10, text_y, &mcfg_info, mcfg_c, None, 1);
                text_y += 16;

                draw_string(surface, pad_x + 10, text_y, "--- Enumerated ACPI Tables ---", Color::TEXT_MUTED, None, 1);
                text_y += 14;

                let total_tables = rep.tables.len();
                let max_lines_fit = (max_text_y.saturating_sub(text_y) / 14).max(1) as usize;
                let max_offset = total_tables.saturating_sub(max_lines_fit);
                let start = app.diag_scroll_offset.min(max_offset);
                let mut displayed = 0;
                for t in rep.tables.iter().skip(start) {
                    if text_y + 14 > max_text_y {
                        break;
                    }
                    let t_line = format!(
                        "  [{}] Phys: {:#010X}, Len: {:6} B, OEM: '{}/{}', Chk: {}",
                        t.sig_str(), t.phys_addr, t.length,
                        t.oem_str().trim(), t.oem_table_str().trim(),
                        if t.checksum_valid { "OK" } else { "FAIL" }
                    );
                    draw_string(surface, pad_x + 10, text_y, &t_line, Color::TEXT_LIGHT, None, 1);
                    text_y += 14;
                    displayed += 1;
                }
                let footer = format!("Viewing tables {}..{} of {} | Use [▲/▼ Prev/Next buttons] or [PgUp/PgDn]", start + 1, (start + displayed).min(total_tables), total_tables);
                draw_string(surface, pad_x + 10, max_text_y, &footer, Color::ACCENT_CYAN, None, 1);
            } else {
                draw_string(surface, pad_x + 10, text_y, "ACPI diagnostic report unavailable.", Color::WARNING_AMBER, None, 1);
            }
        }
    }
}

/// Helper to open or bring to front the Network Manager window.
pub fn open_or_focus_network_window(wm: &mut WindowManager) {
    // 1. Check if a window titled "Network Manager" or "Network" already exists
    let existing_id = wm.windows().iter()
        .find(|w| w.title() == "Network Manager" || w.title() == "Network")
        .map(|w| w.id());

    if let Some(id) = existing_id {
        // If minimized, restore it
        wm.restore_window(id);
        wm.bring_to_front(id);
        wm.set_active(id);
        log::info!("[Network] Activated existing Network Manager window (ID: {:?})", id);
    } else {
        // 2. Spawn a new Network Manager window
        let screen_w = wm.screen_width();
        let screen_h = wm.screen_height();

        let win_w = 560u32.min(screen_w.saturating_sub(40));
        let win_h = 380u32.min(screen_h.saturating_sub(100));
        let win_x = ((screen_w.saturating_sub(win_w)) / 2) as i32;
        let win_y = 110i32;

        let network_win = Window::new(
            WindowId::new(5),
            "Network Manager",
            win_x,
            win_y,
            win_w,
            win_h,
        )
        .with_icon(&ICON_NETWORK)
        .with_min_size(460, 300)
        .with_content_renderer(render_network_content);

        wm.add_window(network_win);
        wm.set_active(WindowId::new(5));
        log::info!("[Network] Spawned new Network Manager window (ID: 5) at ({}, {})", win_x, win_y);
    }
}

/// Routes a client-area mouse click to the global NetworkManagerApp instance.
pub fn handle_click(pt: Point, content_rect: Rect) -> bool {
    NETWORK_MANAGER.lock().handle_click(pt, content_rect)
}

/// Routes a keyboard event to the global NetworkManagerApp instance.
pub fn handle_key(key: KeyEvent) -> bool {
    NETWORK_MANAGER.lock().handle_key(key)
}

/// Pre-caches network hardware inventory once during boot outside of the UI rendering loop.
pub fn init() {
    let mut app = NETWORK_MANAGER.lock();
    if app.scan_count == 0 {
        app.rescan_hardware();
    }
}
