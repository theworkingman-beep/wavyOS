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
pub mod udp;
pub mod tcp;
mod virtio_net;

/// Local MAC address
static LOCAL_MAC: Mutex<[u8; 6]> = Mutex::new([0x52, 0x54, 0x00, 0x12, 0x34, 0x56]);

/// Local IP address (QEMU user-mode networking)
static LOCAL_IP: Mutex<[u8; 4]> = Mutex::new([10, 0, 2, 15]);

/// Initialize the networking stack
pub fn init() {
    log::info!("net: initializing networking stack");

    // Initialize virtio-net driver
    if !virtio_net::init() {
        log::warn!("net: virtio-net initialization failed or no device found");
        return;
    }

    // Get MAC address from driver
    let mac = virtio_net::get_mac();
    *LOCAL_MAC.lock() = mac;

    log::info!("net: networking stack initialized");
    log::info!("net: MAC = {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]);
    log::info!("net: IP = {}.{}.{}.{}",
        LOCAL_IP.lock()[0], LOCAL_IP.lock()[1], LOCAL_IP.lock()[2], LOCAL_IP.lock()[3]);
}

/// Send a raw Ethernet frame
pub fn send_frame(dst_mac: [u8; 6], ethertype: u16, payload: &[u8]) -> bool {
    virtio_net::send_frame(dst_mac, ethertype, payload)
}

/// Receive a raw Ethernet frame
pub fn recv_frame(buf: &mut [u8]) -> usize {
    virtio_net::recv_frame(buf)
}

/// Poll for incoming packets and process them
pub fn poll() {
    let mut buf = [0u8; 1526];
    let len = recv_frame(&mut buf);

    if len > 0 {
        log::debug!("net: received {} bytes", len);
        process_frame(&buf[..len]);
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
                _ => {
                    log::debug!("net: unknown ethertype {:#x}", hdr.ethertype());
                }
            }
        }
        None => {
            log::warn!("net: failed to parse Ethernet frame");
        }
    }
}

/// Handle IPv4 packet
fn handle_ipv4(data: &[u8]) {
    match ipv4::parse(data) {
        Some((hdr, payload)) => {
            let src_ip = hdr.src_ip;
            let dst_ip = hdr.dst_ip;

            // Check if packet is for us
            if dst_ip != *LOCAL_IP.lock() {
                return;
            }

            match hdr.protocol {
                ipv4::PROTOCOL_ICMP => {
                    log::debug!("net: ICMP from {}.{}.{}.{}", src_ip[0], src_ip[1], src_ip[2], src_ip[3]);
                }
                ipv4::PROTOCOL_TCP => {
                    log::debug!("net: TCP from {}.{}.{}.{}", src_ip[0], src_ip[1], src_ip[2], src_ip[3]);
                }
                ipv4::PROTOCOL_UDP => {
                    log::debug!("net: UDP from {}.{}.{}.{}", src_ip[0], src_ip[1], src_ip[2], src_ip[3]);
                }
                _ => {
                    log::debug!("net: unknown protocol {}", hdr.protocol);
                }
            }
        }
        None => {
            log::warn!("net: failed to parse IPv4 packet");
        }
    }
}

/// Send an ARP request
pub fn arp_request(target_ip: [u8; 4]) {
    log::debug!("net: ARP request for {}.{}.{}.{}", target_ip[0], target_ip[1], target_ip[2], target_ip[3]);
    // Build ARP request packet
    // This is simplified - just log for now
}

/// Get local IP address
pub fn get_local_ip() -> [u8; 4] {
    *LOCAL_IP.lock()
}

/// Get local MAC address
pub fn get_local_mac() -> [u8; 6] {
    *LOCAL_MAC.lock()
}

/// Simple checksum calculation for IP headers
pub fn checksum(data: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    let mut i = 0;
    while i < data.len() {
        if i + 1 < data.len() {
            sum += ((data[i] as u32) << 8) | (data[i + 1] as u32);
        } else {
            sum += (data[i] as u32) << 8;
        }
        i += 2;
    }
    while sum >> 16 != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    !sum as u16
}
