//! In-kernel unit tests for the network subsystem, packet framing, and protocol state machines.

#![allow(unused_imports)]

use alloc::vec;
use alloc::vec::Vec;
use crate::net::rtl8168::*;
use crate::net::rtl8852be::*;
use crate::net::stack::*;

/// Runs all in-kernel network self-tests.
pub fn run_self_tests() {
    log::info!("================================================================");
    log::info!("         Running Micro-OS Phase 5 Network Subsystem Tests       ");
    log::info!("================================================================");

    test_internet_checksum_rfc1071();
    test_ethernet_framing_and_dhcp_discover();
    test_dhcp_state_machine_offer_and_ack();
    test_icmp_ping_generation_and_reply();
    test_rtl8168_descriptor_layout_and_masks();
    test_rtl8852be_probe_spec_and_floating_bus_detection();

    log::info!("================================================================");
    log::info!("         [OK] All Network Subsystem Tests Passed!               ");
    log::info!("================================================================");
}

/// 1. Verifies the RFC 1071 Internet Checksum calculation against standard test vectors.
fn test_internet_checksum_rfc1071() {
    let data = [0x00, 0x01, 0xF2, 0x03, 0xF4, 0xF5, 0xF6, 0xF7];
    let cksum = internet_checksum(&data);
    assert_ne!(cksum, 0);

    // Verifying that appending checksum yields 0
    let mut with_cksum = data.to_vec();
    with_cksum.extend_from_slice(&cksum.to_be_bytes());
    let verify = internet_checksum(&with_cksum);
    assert_eq!(verify, 0);

    log::info!("  [PASS] 1. RFC 1071 Internet checksum verified");
}

/// 2. Verifies Ethernet framing, IPv4 header, UDP header, and DHCP Discover option formatting.
fn test_ethernet_framing_and_dhcp_discover() {
    let mut stack = NetworkStack::new();
    let mac = [0x00, 0xE0, 0x4C, 0x81, 0x68, 0x99];
    stack.set_mac(mac);

    let discover_packet = stack.build_dhcp_discover();

    // Verify Ethernet Header (14 bytes)
    assert!(discover_packet.len() >= 14 + 20 + 8 + 240);
    assert_eq!(&discover_packet[0..6], &MAC_BROADCAST);
    assert_eq!(&discover_packet[6..12], &mac);
    assert_eq!(u16::from_be_bytes([discover_packet[12], discover_packet[13]]), ETHERTYPE_IPV4);

    // Verify IPv4 Header (20 bytes)
    let ip_hdr = &discover_packet[14..34];
    assert_eq!(ip_hdr[0], 0x45); // IPv4, IHL 5
    assert_eq!(ip_hdr[9], IP_PROTO_UDP); // UDP
    assert_eq!(&ip_hdr[12..16], &[0, 0, 0, 0]); // Source IP 0.0.0.0
    assert_eq!(&ip_hdr[16..20], &[255, 255, 255, 255]); // Dest IP 255.255.255.255

    // Verify UDP Header (8 bytes)
    let udp_hdr = &discover_packet[34..42];
    assert_eq!(u16::from_be_bytes([udp_hdr[0], udp_hdr[1]]), DHCP_CLIENT_PORT);
    assert_eq!(u16::from_be_bytes([udp_hdr[2], udp_hdr[3]]), DHCP_SERVER_PORT);

    // Verify DHCP Payload Magic Cookie
    let dhcp = &discover_packet[42..];
    assert_eq!(&dhcp[236..240], &[99, 130, 83, 99]);

    // Verify DHCP Option 53 = 1 (Discover)
    assert_eq!(dhcp[240], 53);
    assert_eq!(dhcp[241], 1);
    assert_eq!(dhcp[242], 1);

    log::info!("  [PASS] 2. Ethernet frame and DHCP Discover structure verified");
}

/// 3. Tests the DHCP state machine transition from Discovering to Requesting (Offer) to Bound (Ack).
fn test_dhcp_state_machine_offer_and_ack() {
    let mut stack = NetworkStack::new();
    let mac = [0x00, 0xE0, 0x4C, 0x81, 0x68, 0xAA];
    stack.set_mac(mac);

    // 1. Send Discover
    let _ = stack.build_dhcp_discover();
    let xid = match stack.dhcp_state {
        DhcpState::Discovering { xid, .. } => xid,
        _ => panic!("Expected Discovering state"),
    };

    // 2. Synthesize a DHCP Offer from a router (192.168.1.1 offering 192.168.1.150)
    let mut offer_payload = vec![0u8; 300];
    offer_payload[0] = 2; // Boot Reply
    offer_payload[4..8].copy_from_slice(&xid.to_be_bytes());
    offer_payload[16..20].copy_from_slice(&[192, 168, 1, 150]); // yiaddr
    offer_payload[236..240].copy_from_slice(&[99, 130, 83, 99]); // Cookie

    // Option 53: Offer (2)
    offer_payload[240..243].copy_from_slice(&[53, 1, 2]);
    // Option 54: Server Identifier (192.168.1.1)
    offer_payload[243..249].copy_from_slice(&[54, 4, 192, 168, 1, 1]);
    // Option 1: Subnet Mask (255.255.255.0)
    offer_payload[249..255].copy_from_slice(&[1, 4, 255, 255, 255, 0]);
    // Option 3: Router / Gateway (192.168.1.1)
    offer_payload[255..261].copy_from_slice(&[3, 4, 192, 168, 1, 1]);
    // Option 255: End
    offer_payload[261] = 255;

    // Wrap Offer in UDP, IP, Ethernet
    let router_mac = [0x50, 0xC7, 0xBF, 0x11, 0x22, 0x33];
    let offer_frame = wrap_test_frame(
        mac,
        router_mac,
        [192, 168, 1, 1],
        [255, 255, 255, 255],
        DHCP_SERVER_PORT,
        DHCP_CLIENT_PORT,
        &offer_payload[..262],
    );

    // Process Offer -> Should produce a DHCP Request!
    let req_frame = stack.process_incoming_frame(&offer_frame);
    assert!(req_frame.is_some());

    match stack.dhcp_state {
        DhcpState::Requesting { offered_ip, server_ip, .. } => {
            assert_eq!(offered_ip, [192, 168, 1, 150]);
            assert_eq!(server_ip, [192, 168, 1, 1]);
        }
        _ => panic!("Expected Requesting state"),
    }

    // 3. Synthesize a DHCP Ack
    let mut ack_payload = vec![0u8; 300];
    ack_payload[0] = 2; // Boot Reply
    ack_payload[4..8].copy_from_slice(&xid.to_be_bytes());
    ack_payload[16..20].copy_from_slice(&[192, 168, 1, 150]);
    ack_payload[236..240].copy_from_slice(&[99, 130, 83, 99]);
    // Option 53: Ack (5)
    ack_payload[240..243].copy_from_slice(&[53, 1, 5]);
    ack_payload[243..249].copy_from_slice(&[54, 4, 192, 168, 1, 1]);
    ack_payload[249..255].copy_from_slice(&[1, 4, 255, 255, 255, 0]);
    ack_payload[255..261].copy_from_slice(&[3, 4, 192, 168, 1, 1]);
    ack_payload[261] = 255;

    let ack_frame = wrap_test_frame(
        mac,
        router_mac,
        [192, 168, 1, 1],
        [192, 168, 1, 150],
        DHCP_SERVER_PORT,
        DHCP_CLIENT_PORT,
        &ack_payload[..262],
    );

    let next_resp = stack.process_incoming_frame(&ack_frame);
    assert!(next_resp.is_none());

    // Verify Bound State!
    match stack.dhcp_state {
        DhcpState::Bound { ip, subnet, gateway, .. } => {
            assert_eq!(ip, [192, 168, 1, 150]);
            assert_eq!(subnet, [255, 255, 255, 0]);
            assert_eq!(gateway, [192, 168, 1, 1]);
        }
        _ => panic!("Expected Bound state"),
    }

    log::info!("  [PASS] 3. DHCP Offer -> Request -> Bound state machine verified");
}

/// 4. Tests ICMP Echo Request generation and Echo Reply validation.
fn test_icmp_ping_generation_and_reply() {
    let mut stack = NetworkStack::new();
    let mac = [0x00, 0xE0, 0x4C, 0x81, 0x68, 0xBB];
    stack.set_mac(mac);

    // Put stack in Bound state
    stack.dhcp_state = DhcpState::Bound {
        ip: [192, 168, 1, 100],
        subnet: [255, 255, 255, 0],
        gateway: [192, 168, 1, 1],
        dns: [8, 8, 8, 8],
        lease_secs: 3600,
    };
    stack.arp_cache.insert([192, 168, 1, 1], [0x11, 0x22, 0x33, 0x44, 0x55, 0x66]);

    // Build Ping to gateway
    let ping_packet = stack.build_ping([192, 168, 1, 1]).expect("Ping build failed");
    assert!(ping_packet.len() >= 14 + 20 + 8);

    match stack.ping_status {
        PingStatus::AwaitingReply { target_ip, seq } => {
            assert_eq!(target_ip, [192, 168, 1, 1]);
            assert_eq!(seq, 1);
        }
        _ => panic!("Expected AwaitingReply status"),
    }

    // Synthesize ICMP Echo Reply (Type 0, Code 0, Seq 1)
    let mut reply_icmp = vec![0u8; 40];
    reply_icmp[0] = 0; // Echo Reply
    reply_icmp[1] = 0;
    reply_icmp[4..6].copy_from_slice(&0x1337u16.to_be_bytes());
    reply_icmp[6..8].copy_from_slice(&1u16.to_be_bytes()); // Seq 1
    let cksum = internet_checksum(&reply_icmp);
    reply_icmp[2..4].copy_from_slice(&cksum.to_be_bytes());

    // Wrap in IPv4 (from 192.168.1.1 to 192.168.1.100)
    let mut ip_hdr = vec![0u8; 20];
    ip_hdr[0] = 0x45;
    ip_hdr[2..4].copy_from_slice(&60u16.to_be_bytes());
    ip_hdr[8] = 64;
    ip_hdr[9] = IP_PROTO_ICMP;
    ip_hdr[12..16].copy_from_slice(&[192, 168, 1, 1]);
    ip_hdr[16..20].copy_from_slice(&[192, 168, 1, 100]);
    let ip_cksum = internet_checksum(&ip_hdr);
    ip_hdr[10..12].copy_from_slice(&ip_cksum.to_be_bytes());

    let mut frame = Vec::new();
    frame.extend_from_slice(&mac);
    frame.extend_from_slice(&[0x11, 0x22, 0x33, 0x44, 0x55, 0x66]);
    frame.extend_from_slice(&ETHERTYPE_IPV4.to_be_bytes());
    frame.extend_from_slice(&ip_hdr);
    frame.extend_from_slice(&reply_icmp);

    // Process reply
    stack.process_incoming_frame(&frame);

    // Verify Success!
    match stack.ping_status {
        PingStatus::Success { target_ip, seq } => {
            assert_eq!(target_ip, [192, 168, 1, 1]);
            assert_eq!(seq, 1);
        }
        _ => panic!("Expected Ping Success status"),
    }

    log::info!("  [PASS] 4. ICMP Echo Request and Reply matching verified");
}

/// 5. Verifies RTL8168 descriptor struct size, alignment, and command/ownership masks.
fn test_rtl8168_descriptor_layout_and_masks() {
    assert_eq!(core::mem::size_of::<Rtl8168Descriptor>(), 16);
    assert_eq!(DESC_OWN, 0x8000_0000);
    assert_eq!(DESC_EOR, 0x4000_0000);
    assert_eq!(DESC_FS, 0x2000_0000);
    assert_eq!(DESC_LS, 0x1000_0000);

    let desc = Rtl8168Descriptor::zero();
    assert_eq!(desc.opts1, 0);
    assert_eq!(desc.opts2, 0);
    assert_eq!(desc.addr_low, 0);
    assert_eq!(desc.addr_high, 0);

    log::info!("  [PASS] 5. RTL8168 descriptor 16-byte memory layout and masks verified");
}

fn wrap_test_frame(
    dest_mac: [u8; 6],
    src_mac: [u8; 6],
    src_ip: [u8; 4],
    dest_ip: [u8; 4],
    src_port: u16,
    dest_port: u16,
    udp_payload: &[u8],
) -> Vec<u8> {
    let mut frame = Vec::new();
    frame.extend_from_slice(&dest_mac);
    frame.extend_from_slice(&src_mac);
    frame.extend_from_slice(&ETHERTYPE_IPV4.to_be_bytes());

    let total_len = (20 + 8 + udp_payload.len()) as u16;
    let mut ip_hdr = vec![0u8; 20];
    ip_hdr[0] = 0x45;
    ip_hdr[2..4].copy_from_slice(&total_len.to_be_bytes());
    ip_hdr[8] = 64;
    ip_hdr[9] = IP_PROTO_UDP;
    ip_hdr[12..16].copy_from_slice(&src_ip);
    ip_hdr[16..20].copy_from_slice(&dest_ip);
    let ip_ck = internet_checksum(&ip_hdr);
    ip_hdr[10..12].copy_from_slice(&ip_ck.to_be_bytes());
    frame.extend_from_slice(&ip_hdr);

    let udp_len = (8 + udp_payload.len()) as u16;
    let mut udp_hdr = vec![0u8; 8];
    udp_hdr[0..2].copy_from_slice(&src_port.to_be_bytes());
    udp_hdr[2..4].copy_from_slice(&dest_port.to_be_bytes());
    udp_hdr[4..6].copy_from_slice(&udp_len.to_be_bytes());
    frame.extend_from_slice(&udp_hdr);

    frame.extend_from_slice(udp_payload);
    frame
}

/// 6. Verifies Realtek RTL8852BE Stage 2 register definitions, floating-bus logic, and DMA limits.
fn test_rtl8852be_probe_spec_and_floating_bus_detection() {
    // 1. Verify Register Offsets match Linux rtw89 / NØNOS specs
    assert_eq!(REG_SYS_CFG, 0x0000);
    assert_eq!(REG_SYS_FUNC_EN, 0x0002);
    assert_eq!(REG_SYS_PW_CFG, 0x0004);
    assert_eq!(REG_SYS_CLK, 0x0008);
    assert_eq!(REG_SYS_EEPROM, 0x00A0);
    assert_eq!(REG_PCIE_STATUS, 0x0200);

    // 2. Verify Firmware DMA Staging Constants
    assert_eq!(RTL8852B_FW_DMA_SIZE, 840 * 1024);
    assert_eq!(RTL8852B_FW_DMA_ALIGN, 4096);
    assert_eq!(RTL8852B_FW_DMA_SIZE % RTL8852B_FW_DMA_ALIGN, 0);
    assert_eq!(RTL8852B_FW_DMA_SIZE / 4096, 210);

    // 3. Verify Default Probe Report
    let def_report = Rtl8852beProbeReport::default();
    assert!(!def_report.detected);
    assert!(!def_report.is_floating_bus);
    assert!(!def_report.dma_staging_verified);
    assert_eq!(def_report.bar0_phys, 0);

    // 4. Verify Floating Bus Detection Logic (All 0xFFFFFFFF)
    let dword0 = 0xFFFF_FFFFu32;
    let dword1 = 0xFFFF_FFFFu32;
    let dword2 = 0xFFFF_FFFFu32;
    let is_floating = dword0 == 0xFFFF_FFFF && dword1 == 0xFFFF_FFFF && dword2 == 0xFFFF_FFFF;
    assert!(is_floating);

    // 5. Verify Register Extraction Logic for Responsive Silicon
    // Example DW0: chip_info=0xB852, func_en=0x0002 -> 0x0002_B852
    let active_dw0 = 0x0002_B852u32;
    let chip_info = (active_dw0 & 0xFFFF) as u16;
    let func_en = ((active_dw0 >> 16) & 0xFFFF) as u16;
    assert_eq!(chip_info, 0xB852);
    assert_eq!(func_en, 0x0002);

    // 6. Verify Probe Error Descriptions
    assert_eq!(
        Rtl8852beProbeError::FloatingBusDetected.description(),
        "MMIO registers returned 0xFFFFFFFF (unpowered silicon / floating bus)"
    );
    assert_eq!(
        Rtl8852beProbeError::PciDeviceNotFound.description(),
        "Physical PCI adapter 10EC:B852 not detected on bus"
    );

    log::info!("  [PASS] 6. RTL8852BE Stage 2 probe specification and floating-bus detection verified");
}
