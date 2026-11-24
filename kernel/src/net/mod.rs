//! Network Subsystem: Device Drivers and Protocol Stack.
//!
//! Provides unified network device lifecycle management, cooperative packet polling,
//! and integration between hardware drivers and the TCP/IP stack.

#![allow(dead_code)]

pub mod rtl8168;
pub mod rtl8852be;
pub mod rtl8852be_dma;
pub mod rtl8852be_fw;
pub mod wifi_frame;
pub mod stack;
pub mod tests;

use spin::Mutex;
use alloc::vec::Vec;
use crate::arch::x86_64::pci::{self, VENDOR_REALTEK};
use crate::net::rtl8168::{LinkStatus, Rtl8168Driver};
use crate::net::stack::{DhcpState, DnsStatus, NetworkStack, PingStatus};

/// Global singleton for the active physical Ethernet driver.
pub static ETHERNET_DEVICE: Mutex<Option<Rtl8168Driver>> = Mutex::new(None);

/// Global singleton for the network protocol stack.
pub static NETWORK_STACK: Mutex<NetworkStack> = Mutex::new(NetworkStack::new());

/// Initializes network hardware by scanning the PCI bus for supported Ethernet controllers.
pub fn init() {
    let mut dev_guard = ETHERNET_DEVICE.lock();
    if dev_guard.is_some() {
        return; // Already initialized
    }

    let pci_devices = pci::scan_pci_bus();
    for dev in &pci_devices {
        // Detect Realtek Gigabit Ethernet controllers (RTL8168 / RTL8111 / RTL8125)
        if dev.vendor_id == VENDOR_REALTEK
            && (dev.device_id == 0x8168
                || dev.device_id == 0x8161
                || dev.device_id == 0x8111
                || dev.device_id == 0x8125
                || dev.device_id == 0x3000)
        {
            log::info!("[Net] Found Realtek Ethernet controller at BDF {} (DID 0x{:04X})", dev.bdf.to_string(), dev.device_id);
            match Rtl8168Driver::init_device(dev) {
                Ok(driver) => {
                    let mac = driver.mac_address();
                    NETWORK_STACK.lock().set_mac(mac);
                    log::info!("[Net] Realtek RTL8168 registered as active Ethernet device. MAC: {}", driver.mac_string());
                    *dev_guard = Some(driver);
                    break;
                }
                Err(e) => {
                    log::error!("[Net] Failed to initialize Realtek RTL8168 at BDF {}: {:?}", dev.bdf.to_string(), e);
                }
            }
        }
    }

    // Stage 2: Probe Realtek RTL8852BE-VT Wi-Fi 6 PCIe hardware (read-only register telemetry)
    let _ = rtl8852be::probe_hardware();
}

/// Cooperatively polls the active network interface for incoming packets
/// and processes them through the network stack. Returns `true` if any packet activity occurred.
pub fn poll() -> bool {
    let mut activity = false;

    // Poll physical Wi-Fi radio receiver
    rtl8852be::poll_wifi();

    let mut dev_guard = match ETHERNET_DEVICE.try_lock() {
        Some(g) => g,
        None => return false,
    };
    let driver = match dev_guard.as_mut() {
        Some(d) => d,
        None => return false,
    };

    let link = driver.check_link_status();
    if !link.is_up {
        return false;
    }

    let mut buf = [0u8; 2048];
    let mut responses: Vec<Vec<u8>> = Vec::new();

    // Drain up to 8 packets per poll turn to stay bounded
    for _ in 0..8 {
        if let Some(len) = driver.receive_packet(&mut buf) {
            activity = true;
            if let Some(mut stack) = NETWORK_STACK.try_lock() {
                if let Some(resp) = stack.process_incoming_frame(&buf[..len]) {
                    responses.push(resp);
                }
            }
        } else {
            break;
        }
    }

    // Transmit any generated responses (e.g. ARP replies, DHCP Requests)
    for resp in responses {
        let _ = driver.send_packet(&resp);
    }

    activity
}

/// Initiates DHCP address discovery over the active Wi-Fi or Ethernet link.
pub fn start_dhcp() -> Result<(), &'static str> {
    if rtl8852be::is_wifi_associated() {
        return rtl8852be::start_wifi_dhcp();
    }

    let mut dev_guard = ETHERNET_DEVICE.lock();
    let driver = dev_guard.as_mut().ok_or("No network controller initialized")?;

    let link = driver.check_link_status();
    if !link.is_up {
        return Err("Ethernet cable is disconnected (Link Down)");
    }

    let discover_packet = NETWORK_STACK.lock().build_dhcp_discover();
    driver
        .send_packet(&discover_packet)
        .map_err(|_| "Failed to transmit DHCP Discover packet")?;

    Ok(())
}

/// Sends an ICMP Echo Request (Ping) to the default gateway over active Wi-Fi or Ethernet.
pub fn ping_gateway() -> Result<(), &'static str> {
    if rtl8852be::is_wifi_connected() {
        let ping_packet = {
            let mut stack = NETWORK_STACK.lock();
            let gateway_ip = match stack.dhcp_state {
                DhcpState::Bound { gateway, .. } => gateway,
                _ => return Err("Cannot ping gateway: No DHCP lease obtained"),
            };
            stack
                .build_ping(gateway_ip)
                .ok_or("Failed to construct ICMP ping packet")?
        };
        return rtl8852be::wifi_send_ethernet_packet(&ping_packet);
    }

    let mut dev_guard = ETHERNET_DEVICE.lock();
    let driver = dev_guard.as_mut().ok_or("No network controller initialized")?;

    let link = driver.check_link_status();
    if !link.is_up {
        return Err("Ethernet cable is disconnected (Link Down)");
    }

    let ping_packet = {
        let mut stack = NETWORK_STACK.lock();
        let gateway_ip = match stack.dhcp_state {
            DhcpState::Bound { gateway, .. } => gateway,
            _ => return Err("Cannot ping gateway: No DHCP lease obtained"),
        };
        stack
            .build_ping(gateway_ip)
            .ok_or("Failed to construct ICMP ping packet")?
    };

    driver
        .send_packet(&ping_packet)
        .map_err(|_| "Failed to transmit ICMP ping packet")?;

    Ok(())
}

/// Transmits a general Ethernet packet over the active connected network interface.
pub fn send_packet(packet: &[u8]) -> Result<(), &'static str> {
    if rtl8852be::is_wifi_connected() {
        return rtl8852be::wifi_send_ethernet_packet(packet);
    }
    let mut dev_guard = ETHERNET_DEVICE.lock();
    if let Some(driver) = dev_guard.as_mut() {
        let link = driver.check_link_status();
        if link.is_up {
            return driver.send_packet(packet).map_err(|_| "Ethernet transmission error");
        }
    }
    Err("No network interface connected")
}

/// Returns `true` if either Wi-Fi or Ethernet has verified connection with active IP.
pub fn is_any_connected() -> bool {
    if rtl8852be::is_wifi_connected() {
        return true;
    }
    let eth_up = ETHERNET_DEVICE.try_lock().and_then(|d| d.as_ref().map(|d| d.check_link_status().is_up)).unwrap_or(false);
    let has_ip = NETWORK_STACK.try_lock().map(|s| s.ipv4_address().is_some()).unwrap_or(false);
    eth_up && has_ip
}

/// Returns the current hardware link status if a controller is present.
pub fn get_link_status() -> Option<LinkStatus> {
    ETHERNET_DEVICE.try_lock().and_then(|d| d.as_ref().map(|d| d.check_link_status()))
}

/// Returns the current DHCP lease status.
pub fn get_dhcp_state() -> DhcpState {
    NETWORK_STACK.try_lock().map(|s| s.dhcp_state).unwrap_or(DhcpState::Idle)
}

/// Returns the current ICMP ping status.
pub fn get_ping_status() -> PingStatus {
    NETWORK_STACK.try_lock().map(|s| s.ping_status).unwrap_or(PingStatus::Idle)
}

/// Sends a DNS resolution query for `hostname` via the active DHCP-assigned DNS server.
pub fn resolve_dns(hostname: &str) -> Result<(), &'static str> {
    let packet = {
        let mut stack = NETWORK_STACK.lock();
        let dns = match stack.dhcp_state {
            DhcpState::Bound { dns, .. } => dns,
            _ => return Err("Cannot resolve DNS: No DHCP lease obtained"),
        };
        stack.build_dns_query(hostname, dns).ok_or("Failed to construct DNS query packet")?
    };

    if rtl8852be::is_wifi_connected() {
        return rtl8852be::wifi_send_ethernet_packet(&packet);
    }

    let mut dev_guard = ETHERNET_DEVICE.lock();
    let driver = dev_guard.as_mut().ok_or("No network controller initialized")?;
    let link = driver.check_link_status();
    if !link.is_up {
        return Err("Ethernet cable is disconnected (Link Down)");
    }
    driver.send_packet(&packet).map_err(|_| "Failed to transmit DNS query packet")?;
    Ok(())
}

/// Returns the current DNS query status.
pub fn get_dns_status() -> DnsStatus {
    NETWORK_STACK.try_lock().map(|s| s.dns_status.clone()).unwrap_or(DnsStatus::Idle)
}
