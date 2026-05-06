//! ICMP (Internet Control Message Protocol)
//!
//! Supports Echo Request (ping) and Echo Reply.

#![allow(dead_code)]

pub const PROTOCOL_NUMBER: u8 = 1;

fn checksum(data: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    let mut i = 0;
    while i + 1 < data.len() {
        sum += u16::from_be_bytes([data[i], data[i + 1]]) as u32;
        i += 2;
    }
    if i < data.len() {
        sum += (data[i] as u32) << 8;
    }
    while (sum >> 16) != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    !sum as u16
}

fn checksum_for_packet(icmp_header: &[u8], payload: &[u8]) -> u16 {
    let mut buf = alloc::vec::Vec::new();
    buf.extend_from_slice(icmp_header);
    buf.extend_from_slice(payload);
    checksum(&buf)
}

/// ICMP header
#[repr(C, packed)]
pub struct IcmpHeader {
    pub typ: u8,
    pub code: u8,
    pub checksum: u16,
    pub rest: [u8; 4],
}

pub const ICMP_ECHO_REQUEST: u8 = 8;
pub const ICMP_ECHO_REPLY: u8 = 0;

/// Handle an incoming ICMP packet
pub fn handle(data: &[u8], src_ip: [u8; 4]) {
    if data.len() < 8 {
        return;
    }
    let hdr = unsafe { &*(data.as_ptr() as *const IcmpHeader) };
    match hdr.typ {
        ICMP_ECHO_REQUEST => {
            log::info!("icmp: echo request from {}.{}.{}.{}", src_ip[0], src_ip[1], src_ip[2], src_ip[3]);
            send_echo_reply(src_ip, u16::from_be_bytes([data[4], data[5]]), u16::from_be_bytes([data[6], data[7]]), &data[8..]);
        }
        ICMP_ECHO_REPLY => {
            log::info!("icmp: echo reply from {}.{}.{}.{}", src_ip[0], src_ip[1], src_ip[2], src_ip[3]);
        }
        _ => {}
    }
}

/// Send an ICMP echo reply
fn send_echo_reply(dst_ip: [u8; 4], id: u16, seq: u16, payload: &[u8]) {
    let id_bytes = id.to_be_bytes();
    let seq_bytes = seq.to_be_bytes();
    let mut raw = [0u8; 8];
    raw[0] = ICMP_ECHO_REPLY;
    raw[1] = 0;
    raw[2] = 0;
    raw[3] = 0;
    raw[4] = id_bytes[0];
    raw[5] = id_bytes[1];
    raw[6] = seq_bytes[0];
    raw[7] = seq_bytes[1];

    let csum = checksum_for_packet(&raw, payload);
    let csum_bytes = csum.to_be_bytes();
    raw[2] = csum_bytes[0];
    raw[3] = csum_bytes[1];

    let mut packet = alloc::vec::Vec::new();
    packet.extend_from_slice(&raw);
    packet.extend_from_slice(payload);

    super::ipv4::send_packet(dst_ip, PROTOCOL_NUMBER, &packet);
}

/// Send an ICMP echo request (ping)
pub fn send_echo_request(dst_ip: [u8; 4], id: u16, seq: u16, payload: &[u8]) {
    let id_bytes = id.to_be_bytes();
    let seq_bytes = seq.to_be_bytes();
    let mut raw = [0u8; 8];
    raw[0] = ICMP_ECHO_REQUEST;
    raw[1] = 0;
    raw[2] = 0;
    raw[3] = 0;
    raw[4] = id_bytes[0];
    raw[5] = id_bytes[1];
    raw[6] = seq_bytes[0];
    raw[7] = seq_bytes[1];

    let csum = checksum_for_packet(&raw, payload);
    let csum_bytes = csum.to_be_bytes();
    raw[2] = csum_bytes[0];
    raw[3] = csum_bytes[1];

    let mut packet = alloc::vec::Vec::new();
    packet.extend_from_slice(&raw);
    packet.extend_from_slice(payload);

    super::ipv4::send_packet(dst_ip, PROTOCOL_NUMBER, &packet);
}
