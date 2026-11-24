//! Bare-Metal `no_std` Network Protocol Stack (Ethernet, ARP, IPv4, UDP, DHCP, ICMP).
//!
//! Provides deterministic, zero-external-dependency network stack execution:
//! - RFC 826: Address Resolution Protocol (ARP) resolution and cache.
//! - RFC 791: Internet Protocol Version 4 (IPv4) packet handling and checksums.
//! - RFC 768: User Datagram Protocol (UDP).
//! - RFC 2131: Dynamic Host Configuration Protocol (DHCP) state machine.
//! - RFC 792: Internet Control Message Protocol (ICMP) echo request / reply (Ping).

#![allow(dead_code)]

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

pub const ETHERTYPE_IPV4: u16 = 0x0800;
pub const ETHERTYPE_ARP: u16 = 0x0806;

pub const IP_PROTO_ICMP: u8 = 1;
pub const IP_PROTO_UDP: u8 = 17;

pub const DHCP_CLIENT_PORT: u16 = 68;
pub const DHCP_SERVER_PORT: u16 = 67;

pub const MAC_BROADCAST: [u8; 6] = [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
pub const IP_BROADCAST: [u8; 4] = [255, 255, 255, 255];
pub const IP_ZERO: [u8; 4] = [0, 0, 0, 0];

// ============================================================================
// Internet Checksum Utility
// ============================================================================

/// Computes RFC 1071 Internet Checksum.
pub fn internet_checksum(data: &[u8]) -> u16 {
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

// ============================================================================
// DHCP State Machine
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DhcpState {
    Idle,
    Discovering { xid: u32, attempts: u8 },
    Requesting { xid: u32, offered_ip: [u8; 4], server_ip: [u8; 4] },
    Bound {
        ip: [u8; 4],
        subnet: [u8; 4],
        gateway: [u8; 4],
        dns: [u8; 4],
        lease_secs: u32,
    },
    Failed,
}

// ============================================================================
// Ping / ICMP State
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PingStatus {
    Idle,
    AwaitingReply { target_ip: [u8; 4], seq: u16 },
    Success { target_ip: [u8; 4], seq: u16 },
    Timeout { target_ip: [u8; 4] },
}

// ============================================================================
// DNS State (RFC 1035 UDP Port 53)
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DnsStatus {
    Idle,
    Resolving { hostname: String, dns_server: [u8; 4] },
    Success { hostname: String, ip: [u8; 4], dns_server: [u8; 4] },
    Failed { hostname: String, reason: &'static str },
}

// ============================================================================
// Network Stack
// ============================================================================

pub struct NetworkStack {
    pub mac_addr: [u8; 6],
    pub dhcp_state: DhcpState,
    pub ping_status: PingStatus,
    pub dns_status: DnsStatus,
    pub arp_cache: BTreeMap<[u8; 4], [u8; 6]>,
    next_xid: u32,
    ping_seq: u16,
    dns_xid: u16,
}

impl NetworkStack {
    pub const fn new() -> Self {
        Self {
            mac_addr: [0; 6],
            dhcp_state: DhcpState::Idle,
            ping_status: PingStatus::Idle,
            dns_status: DnsStatus::Idle,
            arp_cache: BTreeMap::new(),
            next_xid: 0x5A1B_3C9D,
            ping_seq: 1,
            dns_xid: 0xD155,
        }
    }

    pub fn set_mac(&mut self, mac: [u8; 6]) {
        self.mac_addr = mac;
    }

    /// Returns the currently assigned IPv4 address if bound by DHCP.
    pub fn ipv4_address(&self) -> Option<[u8; 4]> {
        match self.dhcp_state {
            DhcpState::Bound { ip, .. } => Some(ip),
            _ => None,
        }
    }

    /// Generates a DHCP Discover packet wrapped in UDP, IPv4, and Ethernet.
    pub fn build_dhcp_discover(&mut self) -> Vec<u8> {
        let xid = self.next_xid;
        self.next_xid = self.next_xid.wrapping_add(1);
        self.dhcp_state = DhcpState::Discovering { xid, attempts: 1 };

        let mut dhcp_payload = vec![0u8; 240];
        dhcp_payload[0] = 1; // Boot Request
        dhcp_payload[1] = 1; // Hardware Type: Ethernet
        dhcp_payload[2] = 6; // Hardware Address Length: 6
        dhcp_payload[3] = 0; // Hops

        dhcp_payload[4..8].copy_from_slice(&xid.to_be_bytes()); // XID
        dhcp_payload[10] = 0x80; // Broadcast Flag (0x8000)
        dhcp_payload[28..34].copy_from_slice(&self.mac_addr); // Client Hardware Address

        // DHCP Magic Cookie: 99, 130, 83, 99
        dhcp_payload[236..240].copy_from_slice(&[99, 130, 83, 99]);

        // Options:
        // Option 53: DHCP Message Type = 1 (Discover)
        let mut options = vec![53, 1, 1];
        // Option 55: Parameter Request List (1: Subnet, 3: Router, 6: DNS)
        options.extend_from_slice(&[55, 3, 1, 3, 6]);
        // Option 255: End
        options.push(255);

        dhcp_payload.extend_from_slice(&options);

        self.wrap_udp_packet(
            IP_ZERO,
            IP_BROADCAST,
            DHCP_CLIENT_PORT,
            DHCP_SERVER_PORT,
            MAC_BROADCAST,
            &dhcp_payload,
        )
    }

    /// Generates a DHCP Request packet for an offered IP.
    pub fn build_dhcp_request(&mut self, xid: u32, offered_ip: [u8; 4], server_ip: [u8; 4]) -> Vec<u8> {
        self.dhcp_state = DhcpState::Requesting {
            xid,
            offered_ip,
            server_ip,
        };

        let mut dhcp_payload = vec![0u8; 240];
        dhcp_payload[0] = 1; // Boot Request
        dhcp_payload[1] = 1; // Hardware Type: Ethernet
        dhcp_payload[2] = 6; // Hardware Address Length: 6
        dhcp_payload[3] = 0; // Hops

        dhcp_payload[4..8].copy_from_slice(&xid.to_be_bytes());
        dhcp_payload[10] = 0x80; // Broadcast Flag
        dhcp_payload[28..34].copy_from_slice(&self.mac_addr);

        // Magic Cookie
        dhcp_payload[236..240].copy_from_slice(&[99, 130, 83, 99]);

        // Option 53: DHCP Message Type = 3 (Request)
        let mut options = vec![53, 1, 3];
        // Option 50: Requested IP
        options.extend_from_slice(&[50, 4, offered_ip[0], offered_ip[1], offered_ip[2], offered_ip[3]]);
        // Option 54: Server Identifier
        options.extend_from_slice(&[54, 4, server_ip[0], server_ip[1], server_ip[2], server_ip[3]]);
        // Option 55: Parameter Request List
        options.extend_from_slice(&[55, 3, 1, 3, 6]);
        // Option 255: End
        options.push(255);

        dhcp_payload.extend_from_slice(&options);

        self.wrap_udp_packet(
            IP_ZERO,
            IP_BROADCAST,
            DHCP_CLIENT_PORT,
            DHCP_SERVER_PORT,
            MAC_BROADCAST,
            &dhcp_payload,
        )
    }

    /// Generates an ICMP Echo Request (Ping).
    pub fn build_ping(&mut self, target_ip: [u8; 4]) -> Option<Vec<u8>> {
        let (my_ip, gateway_ip) = match self.dhcp_state {
            DhcpState::Bound { ip, gateway, .. } => (ip, gateway),
            _ => return None,
        };

        // Determine target MAC (either in ARP cache, or gateway MAC)
        let target_mac = if let Some(mac) = self.arp_cache.get(&target_ip) {
            *mac
        } else if let Some(gw_mac) = self.arp_cache.get(&gateway_ip) {
            *gw_mac
        } else {
            MAC_BROADCAST
        };

        let seq = self.ping_seq;
        self.ping_seq = self.ping_seq.wrapping_add(1);
        self.ping_status = PingStatus::AwaitingReply { target_ip, seq };

        // ICMP Echo Request payload: Type 8, Code 0, Checksum, Identifier, Sequence, 32 bytes data
        let mut icmp = vec![0u8; 40];
        icmp[0] = 8; // Echo Request
        icmp[1] = 0; // Code 0
        icmp[4..6].copy_from_slice(&0x1337u16.to_be_bytes()); // Identifier
        icmp[6..8].copy_from_slice(&seq.to_be_bytes()); // Sequence
        // Fill pattern data
        for i in 8..40 {
            icmp[i] = (i - 8) as u8 + b'a';
        }

        let cksum = internet_checksum(&icmp);
        icmp[2..4].copy_from_slice(&cksum.to_be_bytes());

        Some(self.wrap_ipv4_packet(my_ip, target_ip, IP_PROTO_ICMP, target_mac, &icmp))
    }

    /// Generates a DNS A-record query for `hostname` via `dns_server` over UDP port 53.
    pub fn build_dns_query(&mut self, hostname: &str, dns_server: [u8; 4]) -> Option<Vec<u8>> {
        let (my_ip, gateway_ip) = match self.dhcp_state {
            DhcpState::Bound { ip, gateway, .. } => (ip, gateway),
            _ => return None,
        };

        let target_mac = if let Some(mac) = self.arp_cache.get(&dns_server) {
            *mac
        } else if let Some(gw_mac) = self.arp_cache.get(&gateway_ip) {
            *gw_mac
        } else {
            MAC_BROADCAST
        };

        let xid = self.dns_xid;
        self.dns_xid = self.dns_xid.wrapping_add(1);
        self.dns_status = DnsStatus::Resolving {
            hostname: hostname.into(),
            dns_server,
        };

        let mut dns_payload = Vec::with_capacity(64);
        // Header (12 bytes): ID, Flags 0x0100 (standard query, recursion desired), QDCOUNT=1, ANCOUNT=0, NSCOUNT=0, ARCOUNT=0
        dns_payload.extend_from_slice(&xid.to_be_bytes());
        dns_payload.extend_from_slice(&0x0100u16.to_be_bytes());
        dns_payload.extend_from_slice(&1u16.to_be_bytes());
        dns_payload.extend_from_slice(&0u16.to_be_bytes());
        dns_payload.extend_from_slice(&0u16.to_be_bytes());
        dns_payload.extend_from_slice(&0u16.to_be_bytes());

        // Question: QNAME
        for part in hostname.split('.') {
            if part.is_empty() || part.len() > 63 {
                return None;
            }
            dns_payload.push(part.len() as u8);
            dns_payload.extend_from_slice(part.as_bytes());
        }
        dns_payload.push(0); // Zero length label marks end of QNAME

        // QTYPE = 1 (A), QCLASS = 1 (IN)
        dns_payload.extend_from_slice(&1u16.to_be_bytes());
        dns_payload.extend_from_slice(&1u16.to_be_bytes());

        Some(self.wrap_udp_packet(
            my_ip,
            dns_server,
            53053,
            53,
            target_mac,
            &dns_payload,
        ))
    }

    /// Parses an incoming raw Ethernet frame and processes ARP, DHCP, or ICMP.
    ///
    /// Returns any outgoing packet that must be immediately transmitted in response.
    pub fn process_incoming_frame(&mut self, frame: &[u8]) -> Option<Vec<u8>> {
        if frame.len() < 14 {
            return None;
        }

        let dest_mac: [u8; 6] = frame[0..6].try_into().ok()?;
        let src_mac: [u8; 6] = frame[6..12].try_into().ok()?;
        let ethertype = u16::from_be_bytes([frame[12], frame[13]]);

        // Check if addressed to us or broadcast
        if dest_mac != self.mac_addr && dest_mac != MAC_BROADCAST {
            return None;
        }

        let payload = &frame[14..];

        match ethertype {
            ETHERTYPE_ARP => self.handle_arp(payload, src_mac),
            ETHERTYPE_IPV4 => self.handle_ipv4(payload, src_mac),
            _ => None,
        }
    }

    fn handle_arp(&mut self, payload: &[u8], _src_mac: [u8; 6]) -> Option<Vec<u8>> {
        if payload.len() < 28 {
            return None;
        }

        let opcode = u16::from_be_bytes([payload[6], payload[7]]);
        let sender_mac: [u8; 6] = payload[8..14].try_into().ok()?;
        let sender_ip: [u8; 4] = payload[14..18].try_into().ok()?;
        let target_ip: [u8; 4] = payload[24..28].try_into().ok()?;

        // Cache sender's MAC
        self.arp_cache.insert(sender_ip, sender_mac);

        // If it's an ARP request for our assigned IP, send an ARP reply
        if opcode == 1 {
            if let DhcpState::Bound { ip, .. } = self.dhcp_state {
                if target_ip == ip {
                    let mut arp_reply = vec![0u8; 28];
                    arp_reply[0..2].copy_from_slice(&1u16.to_be_bytes()); // Ethernet
                    arp_reply[2..4].copy_from_slice(&ETHERTYPE_IPV4.to_be_bytes()); // IPv4
                    arp_reply[4] = 6; // HW size
                    arp_reply[5] = 4; // Proto size
                    arp_reply[6..8].copy_from_slice(&2u16.to_be_bytes()); // Opcode 2 = Reply
                    arp_reply[8..14].copy_from_slice(&self.mac_addr);
                    arp_reply[14..18].copy_from_slice(&ip);
                    arp_reply[18..24].copy_from_slice(&sender_mac);
                    arp_reply[24..28].copy_from_slice(&sender_ip);

                    return Some(self.wrap_ethernet_frame(sender_mac, ETHERTYPE_ARP, &arp_reply));
                }
            }
        }

        None
    }

    fn handle_ipv4(&mut self, payload: &[u8], src_mac: [u8; 6]) -> Option<Vec<u8>> {
        if payload.len() < 20 {
            return None;
        }

        let ihl = (payload[0] & 0x0F) as usize * 4;
        if ihl < 20 || payload.len() < ihl {
            return None;
        }

        // Validate IPv4 header checksum
        if internet_checksum(&payload[..ihl]) != 0 {
            return None;
        }

        let protocol = payload[9];
        let src_ip: [u8; 4] = payload[12..16].try_into().ok()?;
        let dest_ip: [u8; 4] = payload[16..20].try_into().ok()?;

        // Cache source MAC
        self.arp_cache.insert(src_ip, src_mac);

        let ip_body = &payload[ihl..];

        match protocol {
            IP_PROTO_UDP => self.handle_udp(src_ip, dest_ip, ip_body),
            IP_PROTO_ICMP => self.handle_icmp(src_ip, ip_body),
            _ => None,
        }
    }

    fn handle_udp(&mut self, _src_ip: [u8; 4], _dest_ip: [u8; 4], body: &[u8]) -> Option<Vec<u8>> {
        if body.len() < 8 {
            return None;
        }

        let src_port = u16::from_be_bytes([body[0], body[1]]);
        let dest_port = u16::from_be_bytes([body[2], body[3]]);
        let udp_payload = &body[8..];

        if src_port == DHCP_SERVER_PORT && dest_port == DHCP_CLIENT_PORT {
            self.handle_dhcp(udp_payload)
        } else if src_port == 53 || dest_port == 53053 {
            self.handle_dns(udp_payload)
        } else {
            None
        }
    }

    fn handle_dns(&mut self, payload: &[u8]) -> Option<Vec<u8>> {
        if payload.len() < 12 {
            return None;
        }

        let flags = u16::from_be_bytes([payload[2], payload[3]]);
        let qdcount = u16::from_be_bytes([payload[4], payload[5]]) as usize;
        let ancount = u16::from_be_bytes([payload[6], payload[7]]) as usize;

        // Check if QR bit (bit 15) is set (1 = response)
        if (flags & 0x8000) == 0 {
            return None;
        }

        let rcode = flags & 0x000F;
        if rcode != 0 {
            if let DnsStatus::Resolving { ref hostname, .. } = self.dns_status {
                self.dns_status = DnsStatus::Failed {
                    hostname: hostname.clone(),
                    reason: "DNS server error (NXDOMAIN / ServFail)",
                };
            }
            return None;
        }

        if ancount == 0 {
            if let DnsStatus::Resolving { ref hostname, .. } = self.dns_status {
                self.dns_status = DnsStatus::Failed {
                    hostname: hostname.clone(),
                    reason: "No A records returned",
                };
            }
            return None;
        }

        // Skip Question section
        let mut offset = 12;
        for _ in 0..qdcount {
            while offset < payload.len() {
                let len = payload[offset] as usize;
                offset += 1;
                if len == 0 {
                    break;
                }
                if len >= 0xC0 {
                    offset += 1; // 2-byte compression pointer
                    break;
                }
                offset += len;
            }
            offset += 4; // Skip QTYPE and QCLASS
        }

        // Parse Answer section
        for _ in 0..ancount {
            if offset >= payload.len() {
                break;
            }

            // Name
            if payload[offset] >= 0xC0 {
                offset += 2;
            } else {
                while offset < payload.len() {
                    let len = payload[offset] as usize;
                    offset += 1;
                    if len == 0 {
                        break;
                    }
                    offset += len;
                }
            }

            if offset + 10 > payload.len() {
                break;
            }

            let rtype = u16::from_be_bytes([payload[offset], payload[offset + 1]]);
            let rclass = u16::from_be_bytes([payload[offset + 2], payload[offset + 3]]);
            let rdlength = u16::from_be_bytes([payload[offset + 8], payload[offset + 9]]) as usize;
            offset += 10;

            if offset + rdlength > payload.len() {
                break;
            }

            // If Type A (1), Class IN (1), and length 4: IPv4 address found!
            if rtype == 1 && rclass == 1 && rdlength == 4 {
                let ip: [u8; 4] = [
                    payload[offset],
                    payload[offset + 1],
                    payload[offset + 2],
                    payload[offset + 3],
                ];

                if let DnsStatus::Resolving { ref hostname, dns_server } = self.dns_status {
                    self.dns_status = DnsStatus::Success {
                        hostname: hostname.clone(),
                        ip,
                        dns_server,
                    };
                }
                return None;
            }

            offset += rdlength;
        }

        None
    }

    fn handle_dhcp(&mut self, payload: &[u8]) -> Option<Vec<u8>> {
        if payload.len() < 240 {
            return None;
        }

        let xid = u32::from_be_bytes([payload[4], payload[5], payload[6], payload[7]]);
        let yiaddr: [u8; 4] = payload[16..20].try_into().ok()?;

        // Verify magic cookie
        if &payload[236..240] != &[99, 130, 83, 99] {
            return None;
        }

        // Parse Options
        let mut msg_type: Option<u8> = None;
        let mut server_id: Option<[u8; 4]> = None;
        let mut subnet_mask: [u8; 4] = [255, 255, 255, 0];
        let mut router_ip: [u8; 4] = [0, 0, 0, 0];
        let mut dns_ip: [u8; 4] = [8, 8, 8, 8];
        let mut lease_secs: u32 = 86400;

        let mut idx = 240;
        while idx < payload.len() {
            let opt = payload[idx];
            if opt == 255 {
                break;
            }
            if opt == 0 {
                idx += 1;
                continue;
            }
            if idx + 1 >= payload.len() {
                break;
            }
            let len = payload[idx + 1] as usize;
            if idx + 2 + len > payload.len() {
                break;
            }
            let val = &payload[idx + 2..idx + 2 + len];

            match opt {
                53 if len == 1 => msg_type = Some(val[0]),
                54 if len == 4 => server_id = val.try_into().ok(),
                1 if len == 4 => {
                    if let Ok(s) = val.try_into() {
                        subnet_mask = s;
                    }
                }
                3 if len >= 4 => {
                    if let Ok(r) = val[0..4].try_into() {
                        router_ip = r;
                    }
                }
                6 if len >= 4 => {
                    if let Ok(d) = val[0..4].try_into() {
                        dns_ip = d;
                    }
                }
                51 if len == 4 => {
                    lease_secs = u32::from_be_bytes([val[0], val[1], val[2], val[3]]);
                }
                _ => {}
            }

            idx += 2 + len;
        }

        match (self.dhcp_state, msg_type) {
            (DhcpState::Discovering { xid: expected_xid, .. }, Some(2)) => {
                // DHCP Offer (Message type 2)
                if xid == expected_xid {
                    let s_ip = server_id.unwrap_or(router_ip);
                    Some(self.build_dhcp_request(xid, yiaddr, s_ip))
                } else {
                    None
                }
            }
            (DhcpState::Requesting { xid: expected_xid, offered_ip, .. }, Some(5)) => {
                // DHCP Ack (Message type 5)
                if xid == expected_xid {
                    self.dhcp_state = DhcpState::Bound {
                        ip: offered_ip,
                        subnet: subnet_mask,
                        gateway: router_ip,
                        dns: dns_ip,
                        lease_secs,
                    };
                }
                None
            }
            _ => None,
        }
    }

    fn handle_icmp(&mut self, src_ip: [u8; 4], body: &[u8]) -> Option<Vec<u8>> {
        if body.len() < 8 {
            return None;
        }

        // Validate ICMP checksum
        if internet_checksum(body) != 0 {
            return None;
        }

        let icmp_type = body[0];
        let seq = u16::from_be_bytes([body[6], body[7]]);

        if icmp_type == 0 {
            // Echo Reply
            if let PingStatus::AwaitingReply { target_ip, seq: expected_seq } = self.ping_status {
                if src_ip == target_ip && seq == expected_seq {
                    self.ping_status = PingStatus::Success { target_ip, seq };
                }
            }
        }

        None
    }

    fn wrap_ethernet_frame(&self, dest_mac: [u8; 6], ethertype: u16, payload: &[u8]) -> Vec<u8> {
        let mut frame = Vec::with_capacity(14 + payload.len());
        frame.extend_from_slice(&dest_mac);
        frame.extend_from_slice(&self.mac_addr);
        frame.extend_from_slice(&ethertype.to_be_bytes());
        frame.extend_from_slice(payload);
        frame
    }

    fn wrap_ipv4_packet(
        &self,
        src_ip: [u8; 4],
        dest_ip: [u8; 4],
        proto: u8,
        dest_mac: [u8; 6],
        payload: &[u8],
    ) -> Vec<u8> {
        let total_len = (20 + payload.len()) as u16;
        let mut ip_hdr = vec![0u8; 20];
        ip_hdr[0] = 0x45; // Version 4, IHL 5
        ip_hdr[1] = 0; // DSCP / ECN
        ip_hdr[2..4].copy_from_slice(&total_len.to_be_bytes());
        ip_hdr[4..6].copy_from_slice(&0x4242u16.to_be_bytes()); // ID
        ip_hdr[6..8].copy_from_slice(&0x4000u16.to_be_bytes()); // Don't fragment
        ip_hdr[8] = 64; // TTL
        ip_hdr[9] = proto;
        ip_hdr[12..16].copy_from_slice(&src_ip);
        ip_hdr[16..20].copy_from_slice(&dest_ip);

        let cksum = internet_checksum(&ip_hdr);
        ip_hdr[10..12].copy_from_slice(&cksum.to_be_bytes());

        let mut full_ip = ip_hdr;
        full_ip.extend_from_slice(payload);

        self.wrap_ethernet_frame(dest_mac, ETHERTYPE_IPV4, &full_ip)
    }

    fn wrap_udp_packet(
        &self,
        src_ip: [u8; 4],
        dest_ip: [u8; 4],
        src_port: u16,
        dest_port: u16,
        dest_mac: [u8; 6],
        payload: &[u8],
    ) -> Vec<u8> {
        let udp_len = (8 + payload.len()) as u16;
        let mut udp_hdr = vec![0u8; 8];
        udp_hdr[0..2].copy_from_slice(&src_port.to_be_bytes());
        udp_hdr[2..4].copy_from_slice(&dest_port.to_be_bytes());
        udp_hdr[4..6].copy_from_slice(&udp_len.to_be_bytes());
        udp_hdr[6..8].copy_from_slice(&0u16.to_be_bytes()); // Checksum optional for IPv4 UDP

        let mut full_udp = udp_hdr;
        full_udp.extend_from_slice(payload);

        self.wrap_ipv4_packet(src_ip, dest_ip, IP_PROTO_UDP, dest_mac, &full_udp)
    }
}
