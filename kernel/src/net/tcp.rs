//! TCP protocol support
//!
//! Minimal TCP state machine for client connections.
//! Supports CLOSED -> SYN_SENT -> ESTABLISHED

#![allow(dead_code)]

use spin::Mutex;
use alloc::collections::vec_deque::VecDeque;

#[repr(C, packed)]
pub struct TcpHeader {
    pub src_port: u16,
    pub dst_port: u16,
    pub seq_num: u32,
    pub ack_num: u32,
    pub data_offset_flags: u16,
    pub window: u16,
    pub checksum: u16,
    pub urgent: u16,
}

bitflags::bitflags! {
    struct TcpFlags: u16 {
        const FIN = 0b0000_0000_0000_0001;
        const SYN = 0b0000_0000_0000_0010;
        const RST = 0b0000_0000_0000_0100;
        const PSH = 0b0000_0000_0000_1000;
        const ACK = 0b0000_0000_0001_0000;
        const URG = 0b0000_0000_0010_0000;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TcpState {
    Closed,
    SynSent,
    Established,
    FinWait1,
    FinWait2,
    TimeWait,
    CloseWait,
    LastAck,
}

struct TcpSocket {
    state: TcpState,
    local_port: u16,
    remote_ip: [u8; 4],
    remote_port: u16,
    seq: u32,
    ack: u32,
    rx_queue: VecDeque<alloc::vec::Vec<u8>>,
    rx_pending: alloc::vec::Vec<u8>,
}

const MAX_TCP_SOCKETS: usize = 4;

struct TcpSocketTable {
    sockets: [Option<TcpSocket>; MAX_TCP_SOCKETS],
}

impl TcpSocketTable {
    const fn new() -> Self {
        Self { sockets: [None, None, None, None] }
    }
}

static SOCKETS: Mutex<TcpSocketTable> = Mutex::new(TcpSocketTable::new());

fn tcp_flags(data_offset_flags: u16) -> TcpFlags {
    TcpFlags::from_bits_truncate(u16::from_be(data_offset_flags) & 0x003F)
}

fn tcp_data_offset(data_offset_flags: u16) -> usize {
    (((u16::from_be(data_offset_flags) >> 12) & 0x0F) as usize) * 4
}

fn tcp_checksum(src_ip: [u8; 4], dst_ip: [u8; 4], protocol: u8, data: &[u8]) -> u16 {
    let mut pseudo = alloc::vec::Vec::new();
    pseudo.extend_from_slice(&src_ip);
    pseudo.extend_from_slice(&dst_ip);
    pseudo.push(0);
    pseudo.push(protocol);
    pseudo.push((data.len() >> 8) as u8);
    pseudo.push((data.len() & 0xFF) as u8);
    pseudo.extend_from_slice(data);

    let mut sum: u32 = 0;
    let mut i = 0;
    while i + 1 < pseudo.len() {
        sum += u16::from_be_bytes([pseudo[i], pseudo[i + 1]]) as u32;
        i += 2;
    }
    if i < pseudo.len() {
        sum += (pseudo[i] as u32) << 8;
    }
    while (sum >> 16) != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    !(sum as u16)
}

fn send_segment(socket: &TcpSocket, flags: TcpFlags, data: &[u8]) {
    let hdr_len = 20usize;
    let total_len = hdr_len + data.len();
    let mut packet = alloc::vec::Vec::with_capacity(total_len);

    let src_port = socket.local_port.to_be_bytes();
    let dst_port = socket.remote_port.to_be_bytes();
    let seq = socket.seq.to_be_bytes();
    let ack = socket.ack.to_be_bytes();
    let doff = (((hdr_len / 4) as u16) << 12) | flags.bits();
    let doff_bytes = doff.to_be_bytes();
    let window = 65535u16.to_be_bytes();

    packet.push(src_port[0]);
    packet.push(src_port[1]);
    packet.push(dst_port[0]);
    packet.push(dst_port[1]);
    packet.extend_from_slice(&seq);
    packet.extend_from_slice(&ack);
    packet.extend_from_slice(&doff_bytes);
    packet.extend_from_slice(&window);
    packet.push(0); // checksum placeholder
    packet.push(0);
    packet.push(0); // urgent ptr
    packet.push(0);
    packet.extend_from_slice(data);

    let csum = tcp_checksum(super::get_local_ip(), socket.remote_ip, super::ipv4::PROTOCOL_TCP, &packet);
    let csum_bytes = csum.to_be_bytes();
    packet[16] = csum_bytes[0];
    packet[17] = csum_bytes[1];

    super::ipv4::send_packet(socket.remote_ip, super::ipv4::PROTOCOL_TCP, &packet);
}

fn next_port() -> u16 {
    static NEXT_PORT: Mutex<u16> = Mutex::new(40000);
    let mut p = NEXT_PORT.lock();
    let port = *p;
    *p += 1;
    port
}

/// Open a TCP connection. Returns socket index or None.
pub fn connect(dst_ip: [u8; 4], dst_port: u16) -> Option<usize> {
    let local_port = next_port();
    let sockets = &mut SOCKETS.lock().sockets;
    for (i, entry) in sockets.iter_mut().enumerate() {
        if entry.is_none() {
            let mut socket = TcpSocket {
                state: TcpState::SynSent,
                local_port,
                remote_ip: dst_ip,
                remote_port: dst_port,
                seq: 1000, // initial sequence number
                ack: 0,
                rx_queue: VecDeque::new(),
                rx_pending: alloc::vec::Vec::new(),
            };
            send_segment(&socket, TcpFlags::SYN, &[]);
            socket.seq += 1;
            *entry = Some(socket);
            log::info!("tcp: connect to {}.{}.{}.{}", dst_ip[0], dst_ip[1], dst_ip[2], dst_ip[3]);
            return Some(i);
        }
    }
    None
}

/// Send data on an established TCP connection
pub fn send(socket_idx: usize, data: &[u8]) -> bool {
    let sockets = &mut SOCKETS.lock().sockets;
    if let Some(socket) = sockets.get_mut(socket_idx).and_then(|e| e.as_mut()) {
        if socket.state != TcpState::Established {
            return false;
        }
        send_segment(socket, TcpFlags::PSH | TcpFlags::ACK, data);
        socket.seq += data.len() as u32;
        true
    } else {
        false
    }
}

/// Receive data from a TCP connection into `buf`.
/// Returns number of bytes copied.
pub fn recv(socket_idx: usize, buf: &mut [u8]) -> usize {
    let sockets = &mut SOCKETS.lock().sockets;
    let socket = match sockets.get_mut(socket_idx).and_then(|e| e.as_mut()) {
        Some(s) => s,
        None => return 0,
    };

    if socket.state != TcpState::Established && socket.rx_pending.is_empty() {
        return 0;
    }

    // Drain pending RX into buf
    let mut copied = 0usize;
    while copied < buf.len() && !socket.rx_pending.is_empty() {
        buf[copied] = socket.rx_pending.remove(0);
        copied += 1;
    }
    copied
}

/// Close a TCP connection
pub fn close(socket_idx: usize) {
    let sockets = &mut SOCKETS.lock().sockets;
    if let Some(socket) = sockets.get_mut(socket_idx).and_then(|e| e.as_mut()) {
        if socket.state == TcpState::Established {
            send_segment(socket, TcpFlags::FIN | TcpFlags::ACK, &[]);
            socket.seq += 1;
            socket.state = TcpState::FinWait1;
            log::info!("tcp: sent FIN");
        } else {
            sockets[socket_idx] = None;
        }
    }
}

/// Get socket state
pub fn state(socket_idx: usize) -> TcpState {
    let table = SOCKETS.lock();
    let sockets = &table.sockets;
    match sockets.get(socket_idx).and_then(|e| e.as_ref()) {
        Some(s) => s.state,
        None => TcpState::Closed,
    }
}

/// Handle an incoming TCP segment
pub fn handle(data: &[u8], src_ip: [u8; 4]) {
    if data.len() < 20 {
        return;
    }
    let src_port = u16::from_be_bytes([data[0], data[1]]);
    let dst_port = u16::from_be_bytes([data[2], data[3]]);
    let seq = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
    let ack = u32::from_be_bytes([data[8], data[9], data[10], data[11]]);
    let flags = tcp_flags(((data[12] as u16) << 8 | data[13] as u16));
    let offset = tcp_data_offset(((data[12] as u16) << 8 | data[13] as u16));
    if data.len() < offset {
        return;
    }
    let payload = &data[offset..];
    let payload_len = payload.len();

    let sockets = &mut SOCKETS.lock().sockets;
    for socket_opt in sockets.iter_mut() {
        if let Some(socket) = socket_opt {
            if socket.local_port == dst_port && socket.remote_port == src_port && socket.remote_ip == src_ip {
                match socket.state {
                    TcpState::SynSent if flags.contains(TcpFlags::SYN | TcpFlags::ACK) && ack == socket.seq => {
                        socket.seq = ack;
                        socket.ack = seq + 1;
                        send_segment(socket, TcpFlags::ACK, &[]);
                        socket.state = TcpState::Established;
                        log::info!("tcp: connection established {}.{}.{}.{}:{}",
                            src_ip[0], src_ip[1], src_ip[2], src_ip[3], src_port);
                    }
                    TcpState::Established => {
                        let has_data = payload_len > 0;
                        let mut should_ack = false;

                        if has_data {
                            socket.ack = seq + payload_len as u32;
                            socket.rx_pending.extend_from_slice(payload);
                            should_ack = true;
                            log::info!("tcp: received {} bytes payload", payload_len);
                        }

                        if flags.contains(TcpFlags::FIN) {
                            socket.ack = seq + payload_len as u32 + 1;
                            send_segment(socket, TcpFlags::ACK, &[]);
                            socket.state = TcpState::CloseWait;
                            send_segment(socket, TcpFlags::FIN | TcpFlags::ACK, &[]);
                            socket.seq += 1;
                            socket.state = TcpState::LastAck;
                            log::info!("tcp: received FIN, sending FIN-ACK");
                            return;
                        } else if should_ack || flags.contains(TcpFlags::ACK) {
                            send_segment(socket, TcpFlags::ACK, &[]);
                        }
                    }
                    TcpState::FinWait1 if flags.contains(TcpFlags::ACK) => {
                        if flags.contains(TcpFlags::FIN) {
                            socket.ack = seq + payload_len as u32 + 1;
                            send_segment(socket, TcpFlags::ACK, &[]);
                            *socket_opt = None;
                            log::info!("tcp: connection closed");
                        } else {
                            socket.state = TcpState::FinWait2;
                        }
                    }
                    TcpState::FinWait2 if flags.contains(TcpFlags::FIN) => {
                        socket.ack = seq + payload_len as u32 + 1;
                        send_segment(socket, TcpFlags::ACK, &[]);
                        *socket_opt = None;
                        log::info!("tcp: connection closed");
                    }
                    TcpState::LastAck if flags.contains(TcpFlags::ACK) => {
                        *socket_opt = None;
                        log::info!("tcp: connection closed (LastAck)");
                    }
                    _ => {}
                }
                return;
            }
        }
    }
    log::debug!("tcp: no socket for {}.{}.{}.{}:{}", src_ip[0], src_ip[1], src_ip[2], src_ip[3], dst_port);
}
