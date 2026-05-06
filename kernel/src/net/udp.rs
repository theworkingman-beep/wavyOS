//! UDP protocol support and socket API

#![allow(dead_code)]

extern crate alloc;

use spin::Mutex;
use alloc::collections::vec_deque::VecDeque;

#[repr(C, packed)]
pub struct UdpHeader {
    pub src_port: u16,
    pub dst_port: u16,
    pub len: u16,
    pub checksum: u16,
}

struct UdpSocket {
    local_port: u16,
    remote_ip: [u8; 4],
    remote_port: u16,
    rx_queue: VecDeque<UdpPacket>,
}

struct UdpPacket {
    src_ip: [u8; 4],
    src_port: u16,
    data: alloc::vec::Vec<u8>,
}

static SOCKETS: Mutex<[Option<UdpSocket>; 4]> = Mutex::new([
    None, None, None, None,
]);

/// Parse UDP packet and dispatch to sockets
pub fn handle(data: &[u8], src_ip: [u8; 4]) {
    if data.len() < 8 {
        return;
    }
    let src_port = u16::from_be_bytes([data[0], data[1]]);
    let dst_port = u16::from_be_bytes([data[2], data[3]]);
    let len = u16::from_be_bytes([data[4], data[5]]) as usize;
    if len < 8 || len > data.len() {
        return;
    }
    let payload = &data[8..len];

    let mut sockets = SOCKETS.lock();
    for entry in sockets.iter_mut() {
        if let Some(socket) = entry {
            if socket.local_port == dst_port {
                let packet = UdpPacket {
                    src_ip,
                    src_port,
                    data: alloc::vec::Vec::from(payload),
                };
                if socket.rx_queue.len() >= 16 {
                    socket.rx_queue.pop_front();
                }
                socket.rx_queue.push_back(packet);
                log::info!("udp: received {} bytes on port {}", payload.len(), dst_port);
                return;
            }
        }
    }
    log::debug!("udp: no socket for port {}", dst_port);
}

/// Bind a UDP socket to a local port. Returns socket index or None.
pub fn bind(port: u16) -> Option<usize> {
    let mut sockets = SOCKETS.lock();
    for (i, entry) in sockets.iter_mut().enumerate() {
        if entry.is_none() {
            *entry = Some(UdpSocket {
                local_port: port,
                remote_ip: [0; 4],
                remote_port: 0,
                rx_queue: VecDeque::new(),
            });
            log::info!("udp: bound socket {} to port {}", i, port);
            return Some(i);
        }
    }
    None
}

/// Send a UDP datagram
pub fn sendto(socket_idx: usize, dst_ip: [u8; 4], dst_port: u16, data: &[u8]) -> bool {
    let src_port = {
        let sockets = SOCKETS.lock();
        match sockets.get(socket_idx).and_then(|e| e.as_ref()) {
            Some(s) => s.local_port,
            None => return false,
        }
    };

    let len = (8 + data.len()) as u16;
    let mut packet = alloc::vec::Vec::with_capacity(len as usize);
    packet.push((src_port >> 8) as u8);
    packet.push((src_port & 0xFF) as u8);
    packet.push((dst_port >> 8) as u8);
    packet.push((dst_port & 0xFF) as u8);
    packet.push((len >> 8) as u8);
    packet.push((len & 0xFF) as u8);
    packet.push(0); // checksum hi
    packet.push(0); // checksum lo
    packet.extend_from_slice(data);

    super::ipv4::send_packet(dst_ip, super::ipv4::PROTOCOL_UDP, &packet);
    true
}

/// Receive a UDP datagram. Returns (src_ip, src_port, bytes_read).
pub fn recvfrom(socket_idx: usize, buf: &mut [u8]) -> Option<([u8; 4], u16, usize)> {
    let mut sockets = SOCKETS.lock();
    let socket = match sockets.get_mut(socket_idx).and_then(|e| e.as_mut()) {
        Some(s) => s,
        None => return None,
    };

    let packet = socket.rx_queue.pop_front()?;
    let copy_len = packet.data.len().min(buf.len());
    buf[..copy_len].copy_from_slice(&packet.data[..copy_len]);
    Some((packet.src_ip, packet.src_port, copy_len))
}
