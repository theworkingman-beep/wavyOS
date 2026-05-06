//! ARP (Address Resolution Protocol)
//!
//! Maintains a small ARP table cache for IP -> MAC resolution,
//! replies to incoming ARP requests for our IP, and can send ARP
//! requests to discover hosts.

#![allow(dead_code)]

extern crate alloc;

use spin::Mutex;
use alloc::vec::Vec;

const ARP_HW_TYPE_ETHERNET: u16 = 1;
const ARP_PROTO_TYPE_IPV4: u16 = 0x0800;
const ARP_HW_SIZE: u8 = 6;
const ARP_PROTO_SIZE: u8 = 4;

pub const ARP_OPCODE_REQUEST: u16 = 1;
pub const ARP_OPCODE_REPLY: u16 = 2;

#[repr(C, packed)]
pub struct ArpHeader {
    pub hw_type: u16,
    pub proto_type: u16,
    pub hw_size: u8,
    pub proto_size: u8,
    pub opcode: u16,
    pub sender_mac: [u8; 6],
    pub sender_ip: [u8; 4],
    pub target_mac: [u8; 6],
    pub target_ip: [u8; 4],
}

#[derive(Clone)]
struct ArpEntry {
    ip: [u8; 4],
    mac: [u8; 6],
}

// Not Copy because we may extend later, but we can use alloc::vec::Vec.
static ARP_TABLE: Mutex<Vec<ArpEntry>> = Mutex::new(alloc::vec::Vec::new());

fn local_ip() -> [u8; 4] {
    crate::net::get_local_ip()
}
fn local_mac() -> [u8; 6] {
    crate::net::get_local_mac()
}

fn send_arp(opcode: u16, target_ip: [u8; 4], dst_mac: [u8; 6]) {
    let hdr = ArpHeader {
        hw_type: u16::to_be(ARP_HW_TYPE_ETHERNET),
        proto_type: u16::to_be(ARP_PROTO_TYPE_IPV4),
        hw_size: ARP_HW_SIZE,
        proto_size: ARP_PROTO_SIZE,
        opcode: u16::to_be(opcode),
        sender_mac: local_mac(),
        sender_ip: local_ip(),
        target_mac: [0u8; 6],
        target_ip,
    };
    let payload = unsafe {
        core::slice::from_raw_parts(
            &hdr as *const ArpHeader as *const u8,
            core::mem::size_of::<ArpHeader>(),
        )
    };
    crate::net::send_frame(dst_mac, super::ethernet::ETHERTYPE_ARP, payload);
}

pub fn handle_arp(data: &[u8]) {
    if data.len() < core::mem::size_of::<ArpHeader>() {
        return;
    }
    let hdr = unsafe { &*(data.as_ptr() as *const ArpHeader) };
    let opcode = u16::from_be(hdr.opcode);

    if u16::from_be(hdr.hw_type) != ARP_HW_TYPE_ETHERNET
        || u16::from_be(hdr.proto_type) != ARP_PROTO_TYPE_IPV4
    {
        return;
    }

    // Cache the sender unconditionally
    cache(hdr.sender_ip, hdr.sender_mac);

    match opcode {
        ARP_OPCODE_REQUEST if hdr.target_ip == local_ip() => {
            log::info!("arp: request for our IP from {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                hdr.sender_mac[0], hdr.sender_mac[1], hdr.sender_mac[2],
                hdr.sender_mac[3], hdr.sender_mac[4], hdr.sender_mac[5]);
            send_arp(ARP_OPCODE_REPLY, hdr.sender_ip, hdr.sender_mac);
        }
        ARP_OPCODE_REPLY if hdr.target_ip == local_ip() => {
            log::info!("arp: reply from {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x} -> {}.{}.{}.{}",
                hdr.sender_mac[0], hdr.sender_mac[1], hdr.sender_mac[2],
                hdr.sender_mac[3], hdr.sender_mac[4], hdr.sender_mac[5],
                hdr.sender_ip[0], hdr.sender_ip[1], hdr.sender_ip[2], hdr.sender_ip[3]);
        }
        _ => {}
    }
}

pub fn request(target_ip: [u8; 4]) {
    log::info!("arp: requesting {}.{}.{}.{}", target_ip[0], target_ip[1], target_ip[2], target_ip[3]);
    send_arp(ARP_OPCODE_REQUEST, target_ip, [0xFF; 6]);
}

pub fn lookup(ip: [u8; 4]) -> Option<[u8; 6]> {
    for entry in ARP_TABLE.lock().iter() {
        if entry.ip == ip {
            return Some(entry.mac);
        }
    }
    None
}

fn cache(ip: [u8; 4], mac: [u8; 6]) {
    let mut table = ARP_TABLE.lock();
    for entry in table.iter_mut() {
        if entry.ip == ip {
            entry.mac = mac;
            return;
        }
    }
    table.push(ArpEntry { ip, mac });
}
