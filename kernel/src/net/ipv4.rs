//! IPv4 protocol support
//!
//! Parses incoming IPv4 packets and provides `send_packet`
//! to build an IPv4 frame wrapped in Ethernet.

#![allow(dead_code)]

use spin::Mutex;

#[derive(Clone, Copy)]
#[repr(C, packed)]
pub struct IPv4Header {
    pub version_ihl: u8,
    pub dscp_ecn: u8,
    pub total_len: u16,
    pub identification: u16,
    pub flags_fragment: u16,
    pub ttl: u8,
    pub protocol: u8,
    pub checksum: u16,
    pub src_ip: [u8; 4],
    pub dst_ip: [u8; 4],
}

pub const PROTOCOL_ICMP: u8 = 1;
pub const PROTOCOL_TCP: u8 = 6;
pub const PROTOCOL_UDP: u8 = 17;

fn ipv4_checksum(hdr: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    let mut i = 0;
    while i + 1 < hdr.len() {
        sum += u16::from_be_bytes([hdr[i], hdr[i + 1]]) as u32;
        i += 2;
    }
    while (sum >> 16) != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    !(sum as u16)
}

fn src_ip() -> [u8; 4] {
    super::get_local_ip()
}

fn mac_for_dst(dst_ip: [u8; 4]) -> Option<[u8; 6]> {
    super::arp::lookup(dst_ip)
}

pub fn send_packet(dst_ip: [u8; 4], protocol: u8, payload: &[u8]) {
    let src = src_ip();
    let total_len = 20 + payload.len();

    let hdr = IPv4Header {
        version_ihl: 0x45,
        dscp_ecn: 0,
        total_len: (total_len as u16).to_be(),
        identification: 0,
        flags_fragment: 0x4000u16.to_be(), // Don't fragment
        ttl: 64,
        protocol,
        checksum: 0,
        src_ip: src,
        dst_ip,
    };
    // Serialize header and calculate checksum
    let hdr_bytes = unsafe {
        core::slice::from_raw_parts(
            &hdr as *const IPv4Header as *const u8,
            core::mem::size_of::<IPv4Header>(),
        )
    };
    let mut hdr_buf = [0u8; 20];
    hdr_buf.copy_from_slice(hdr_bytes);
    hdr_buf[10] = 0;
    hdr_buf[11] = 0;
    let csum = ipv4_checksum(&hdr_buf);
    let csum_bytes = csum.to_be_bytes();
    hdr_buf[10] = csum_bytes[0];
    hdr_buf[11] = csum_bytes[1];

    let mut packet = alloc::vec::Vec::new();
    packet.extend_from_slice(&hdr_buf);
    packet.extend_from_slice(payload);

    // Find MAC via ARP, or broadcast if not cached
    if let Some(mac) = mac_for_dst(dst_ip) {
        super::send_frame(mac, super::ethernet::ETHERTYPE_IPV4, &packet);
    } else {
        // Send ARP request, drop packet for now
        super::arp::request(dst_ip);
    }
}

/// Parse IPv4 packet
pub fn parse(data: &[u8]) -> Option<(IPv4Header, &[u8])> {
    if data.len() < core::mem::size_of::<IPv4Header>() {
        return None;
    }
    let hdr = unsafe { &*(data.as_ptr() as *const IPv4Header) };
    let hdr_len = (hdr.version_ihl & 0x0F) as usize * 4;
    if data.len() < hdr_len {
        return None;
    }
    let payload = &data[hdr_len..];
    Some((*hdr, payload))
}
