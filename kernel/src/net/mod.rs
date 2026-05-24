//! Networking stack (TCP/IP)
//!
//! Implements a basic TCP/IP stack for VibeOS.
//! Uses virtio-net for Ethernet frame transmission.

#![allow(dead_code)]

use core::ptr;
use spin::Mutex;

pub mod ethernet;
pub mod arp;
pub mod ipv4;
pub mod icmp;
pub mod udp;
pub mod tcp;
pub mod dns;
mod virtio_net;

/// Local MAC address
static LOCAL_MAC: Mutex<[u8; 6]> = Mutex::new([0x52, 0x54, 0x00, 0x12, 0x34, 0x56]);

/// Local IP address (QEMU user-mode networking)
static LOCAL_IP: Mutex<[u8; 4]> = Mutex::new([10, 0, 2, 15]);

/// DHCP gate
pub const GATEWAY_IP: [u8; 4] = [10, 0, 2, 2];

/// Initialize the networking stack
pub fn init() {
    log::info!("net: initializing networking stack");

    if !virtio_net::init() {
        log::warn!("net: virtio-net initialization failed");
        return;
    }

    let mac = virtio_net::get_mac();
    *LOCAL_MAC.lock() = mac;

    log::info!("net: MAC = {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]);
}

/// Send a raw Ethernet frame
pub fn send_frame(dst_mac: [u8; 6], ethertype: u16, payload: &[u8]) -> bool {
    virtio_net::send_frame(dst_mac, ethertype, payload)
}

/// Receive a raw Ethernet frame
pub fn recv_frame(buf: &mut [u8]) -> usize {
    virtio_net::recv_frame(buf)
}

/// Poll for incoming packets
pub fn poll() {
    // Process up to 4 packets at once
    for _ in 0..4 {
        let mut buf = [0u8; 1526];
        let len = recv_frame(&mut buf);
        if len > 0 {
            process_frame(&buf[..len]);
        } else {
            break;
        }
    }
}

/// Process a received Ethernet frame
fn process_frame(frame: &[u8]) {
    match ethernet::parse(frame) {
        Some((hdr, payload)) => {
            match hdr.ethertype() {
                ethernet::ETHERTYPE_ARP => {
                    arp::handle_arp(payload);
                }
                ethernet::ETHERTYPE_IPV4 => {
                    handle_ipv4(payload);
                }
                #[allow(unreachable_patterns)]
                _ => {}
            }
        }
        None => {}
    }
}

/// Handle IPv4 packet
fn handle_ipv4(data: &[u8]) {
    match ipv4::parse(data) {
        Some((hdr, payload)) => {
            if hdr.dst_ip != *LOCAL_IP.lock() {
                // Accept broadcast/multicast or our IP
                let dst = u32::from_be_bytes(hdr.dst_ip);
                if dst != 0xFFFFFFFF && dst & 0xF000_0000 != 0xE000_0000 {
                    return;
                }
            }
            match hdr.protocol {
                ipv4::PROTOCOL_ICMP => {
                    icmp::handle(payload, hdr.src_ip);
                }
                ipv4::PROTOCOL_UDP => {
                    // Check if this is a DNS response (source port 53)
                    if payload.len() >= 8 {
                        let src_port = u16::from_be_bytes([payload[0], payload[1]]);
                        if src_port == 53 {
                            let udp_len = u16::from_be_bytes([payload[4], payload[5]]) as usize;
                            if udp_len >= 8 && udp_len <= payload.len() {
                                dns::handle_response(&payload[8..udp_len]);
                            }
                        }
                    }
                    udp::handle(payload, hdr.src_ip);
                }
                ipv4::PROTOCOL_TCP => {
                    tcp::handle(payload, hdr.src_ip);
                }
                _ => {}
            }
        }
        None => {}
    }
}

/// Get local IP
pub fn get_local_ip() -> [u8; 4] {
    *LOCAL_IP.lock()
}

/// Get local MAC
pub fn get_local_mac() -> [u8; 6] {
    *LOCAL_MAC.lock()
}
