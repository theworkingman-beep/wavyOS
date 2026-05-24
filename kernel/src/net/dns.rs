//! DNS resolver module
//!
//! Supports A record queries over UDP port 53 to the QEMU gateway (10.0.2.2).
//! Includes a cache with TTL tracking.

#![allow(dead_code)]

extern crate alloc;

use spin::Mutex;
use alloc::vec::Vec;
use alloc::string::String;

use super::udp;
use super::GATEWAY_IP;

// DNS constants
const DNS_PORT: u16 = 53;
const DNS_HEADER_SIZE: usize = 12;
const TYPE_A: u16 = 1;
const CLASS_IN: u16 = 1;
const RCODE_MASK: u16 = 0x000F;
const FLAG_RESPONSE: u16 = 0x8000;
const FLAG_RECURSION_DESIRED: u16 = 0x0100;

/// Maximum number of poll iterations when waiting for a DNS response.
/// Each iteration calls net::poll() to process incoming packets.
const MAX_POLL_ITERATIONS: u32 = 200_000;

/// Maximum cache entries
const MAX_CACHE_ENTRIES: usize = 32;

// --- DNS Header ---

#[derive(Clone, Copy, Debug)]
pub struct DnsHeader {
    pub id: u16,
    pub flags: u16,
    pub qdcount: u16,
    pub ancount: u16,
    pub nscount: u16,
    pub arcount: u16,
}

impl DnsHeader {
    fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < DNS_HEADER_SIZE {
            return None;
        }
        Some(DnsHeader {
            id: u16::from_be_bytes([data[0], data[1]]),
            flags: u16::from_be_bytes([data[2], data[3]]),
            qdcount: u16::from_be_bytes([data[4], data[5]]),
            ancount: u16::from_be_bytes([data[6], data[7]]),
            nscount: u16::from_be_bytes([data[8], data[9]]),
            arcount: u16::from_be_bytes([data[10], data[11]]),
        })
    }

    fn serialize(&self) -> [u8; DNS_HEADER_SIZE] {
        let mut buf = [0u8; DNS_HEADER_SIZE];
        buf[0..2].copy_from_slice(&self.id.to_be_bytes());
        buf[2..4].copy_from_slice(&self.flags.to_be_bytes());
        buf[4..6].copy_from_slice(&self.qdcount.to_be_bytes());
        buf[6..8].copy_from_slice(&self.ancount.to_be_bytes());
        buf[8..10].copy_from_slice(&self.nscount.to_be_bytes());
        buf[10..12].copy_from_slice(&self.arcount.to_be_bytes());
        buf
    }
}

// --- DNS Question ---

#[derive(Clone, Debug)]
pub struct DnsQuestion {
    pub name: String,
    pub qtype: u16,
    pub qclass: u16,
}

// --- DNS Resource Record ---

#[derive(Clone, Debug)]
pub struct DnsRecord {
    pub name: String,
    pub rtype: u16,
    pub rclass: u16,
    pub ttl: u32,
    pub rdata: Vec<u8>,
}

// --- DNS Cache Entry ---

#[derive(Clone)]
struct CacheEntry {
    name: String,
    ip: [u8; 4],
    ttl: u32,
    expires_at: u64,
}

/// Global DNS cache
static DNS_CACHE: Mutex<Vec<CacheEntry>> = Mutex::new(alloc::vec::Vec::new());

/// Monotonic counter used as a rough clock for TTL expiration.
/// Incremented each time we poll or resolve. Not wall-clock accurate
/// but sufficient for relative TTL tracking.
static MONO_COUNTER: Mutex<u64> = Mutex::new(0);

/// Source port used for DNS queries
static DNS_SRC_PORT: Mutex<u16> = Mutex::new(5000);

/// Next transaction ID for DNS queries
static NEXT_TXID: Mutex<u16> = Mutex::new(0x1234);

/// UDP socket index for DNS (bound once)
static DNS_SOCKET: Mutex<Option<usize>> = Mutex::new(None);

/// Pending query ID we're waiting for a response to
static PENDING_QUERY: Mutex<Option<(u16, String)>> = Mutex::new(None);

/// Result of the most recent DNS resolution (populated by handle_response)
static LAST_RESOLVE_RESULT: Mutex<Option<[u8; 4]>> = Mutex::new(None);

fn advance_clock() -> u64 {
    let mut counter = MONO_COUNTER.lock();
    *counter += 1;
    *counter
}

fn current_time() -> u64 {
    *MONO_COUNTER.lock()
}

fn ensure_socket() -> Option<usize> {
    let mut sock = DNS_SOCKET.lock();
    if let Some(idx) = *sock {
        return Some(idx);
    }
    let port = {
        let mut p = DNS_SRC_PORT.lock();
        let port = *p;
        *p = if *p >= 65535 { 5000 } else { *p + 1 };
        port
    };
    let idx = udp::bind(port)?;
    log::info!("dns: bound UDP socket {} to port {}", idx, port);
    *sock = Some(idx);
    Some(idx)
}

/// Encode a domain name in DNS wire format (label sequence).
/// e.g. "example.com" -> [7, 'e', 'x', 'a', 'm', 'p', 'l', 'e', 3, 'c', 'o', 'm', 0]
fn encode_name(name: &str) -> Vec<u8> {
    let mut buf = Vec::new();
    for label in name.split('.') {
        let len = label.len();
        if len > 63 || len == 0 {
            continue;
        }
        buf.push(len as u8);
        buf.extend_from_slice(label.as_bytes());
    }
    buf.push(0); // root label
    buf
}

/// Decode a DNS name from wire format, handling compression pointers.
fn decode_name(data: &[u8], offset: usize) -> Option<(String, usize)> {
    let mut name = String::new();
    let mut pos = offset;
    let mut jumped = false;
    let mut end_pos = 0; // position after the name in the original section

    loop {
        if pos >= data.len() {
            return None;
        }
        let len = data[pos] as usize;
        if len == 0 {
            if !jumped {
                end_pos = pos + 1;
            }
            break;
        }
        // Check for compression pointer (top 2 bits = 11)
        if (len & 0xC0) == 0xC0 {
            if pos + 1 >= data.len() {
                return None;
            }
            if !jumped {
                end_pos = pos + 2;
            }
            let ptr = ((data[pos] as usize & 0x3F) << 8) | (data[pos + 1] as usize);
            pos = ptr;
            jumped = true;
            continue;
        }
        // Regular label
        if pos + 1 + len > data.len() {
            return None;
        }
        if !name.is_empty() {
            name.push('.');
        }
        for i in 0..len {
            let b = data[pos + 1 + i];
            name.push(b as char);
        }
        pos += 1 + len;
    }

    let final_pos = if jumped { end_pos } else { pos };
    Some((name, final_pos))
}

/// Build a DNS query packet for an A record
fn build_query(name: &str, txid: u16) -> Vec<u8> {
    let header = DnsHeader {
        id: txid,
        flags: FLAG_RECURSION_DESIRED, // Recursion desired
        qdcount: 1,
        ancount: 0,
        nscount: 0,
        arcount: 0,
    };

    let mut packet = Vec::new();
    packet.extend_from_slice(&header.serialize());
    packet.extend_from_slice(&encode_name(name));

    // QTYPE = A (1), QCLASS = IN (1)
    packet.extend_from_slice(&TYPE_A.to_be_bytes());
    packet.extend_from_slice(&CLASS_IN.to_be_bytes());

    packet
}

/// Parse a DNS response packet
fn parse_response(data: &[u8]) -> Option<(DnsHeader, Vec<DnsQuestion>, Vec<DnsRecord>)> {
    let header = DnsHeader::parse(data)?;

    // Parse questions
    let mut questions = Vec::new();
    let mut offset = DNS_HEADER_SIZE;
    for _ in 0..header.qdcount {
        let (name, new_offset) = decode_name(data, offset)?;
        offset = new_offset;
        if offset + 4 > data.len() {
            return None;
        }
        let qtype = u16::from_be_bytes([data[offset], data[offset + 1]]);
        let qclass = u16::from_be_bytes([data[offset + 2], data[offset + 3]]);
        offset += 4;
        questions.push(DnsQuestion {
            name,
            qtype,
            qclass,
        });
    }

    // Parse answer records
    let mut answers = Vec::new();
    for _ in 0..header.ancount {
        let (name, new_offset) = decode_name(data, offset)?;
        offset = new_offset;
        if offset + 10 > data.len() {
            return None;
        }
        let rtype = u16::from_be_bytes([data[offset], data[offset + 1]]);
        let rclass = u16::from_be_bytes([data[offset + 2], data[offset + 3]]);
        let ttl = u32::from_be_bytes([data[offset + 4], data[offset + 5], data[offset + 6], data[offset + 7]]);
        let rdlength = u16::from_be_bytes([data[offset + 8], data[offset + 9]]) as usize;
        offset += 10;
        if offset + rdlength > data.len() {
            return None;
        }
        let rdata = Vec::from(&data[offset..offset + rdlength]);
        offset += rdlength;
        answers.push(DnsRecord {
            name,
            rtype,
            rclass,
            ttl,
            rdata,
        });
    }

    Some((header, questions, answers))
}

/// Cache a DNS A record with TTL tracking
fn cache_result(name: &str, ip: [u8; 4], ttl: u32) {
    let now = current_time();
    // Approximate: treat each clock tick as ~1ms, so ttl (in seconds) maps to ttl*1000 ticks.
    // This is rough but functional for preventing stale entries.
    let expires_at = now + (ttl as u64) * 1000;

    let mut cache = DNS_CACHE.lock();
    // Update existing entry if present
    for entry in cache.iter_mut() {
        if entry.name == name {
            entry.ip = ip;
            entry.ttl = ttl;
            entry.expires_at = expires_at;
            log::info!(
                "dns: cache update {} -> {}.{}.{}.{} ttl={}",
                name, ip[0], ip[1], ip[2], ip[3], ttl
            );
            return;
        }
    }
    // Evict expired entries if cache is full
    if cache.len() >= MAX_CACHE_ENTRIES {
        let now = current_time();
        cache.retain(|e| e.expires_at > now);
    }
    // If still full after eviction, remove oldest
    if cache.len() >= MAX_CACHE_ENTRIES {
        cache.remove(0);
    }
    cache.push(CacheEntry {
        name: String::from(name),
        ip,
        ttl,
        expires_at,
    });
    log::info!(
        "dns: cache add {} -> {}.{}.{}.{} ttl={}",
        name, ip[0], ip[1], ip[2], ip[3], ttl
    );
}

/// Look up a name in the DNS cache. Returns None if not found or expired.
fn cache_lookup(name: &str) -> Option<[u8; 4]> {
    let now = current_time();
    let cache = DNS_CACHE.lock();
    for entry in cache.iter() {
        if entry.name == name && entry.expires_at > now {
            return Some(entry.ip);
        }
    }
    None
}

/// Handle an incoming DNS response.
/// Called from net::handle_ipv4 when a UDP packet on port 53 is received.
pub fn handle_response(data: &[u8]) {
    let (header, _questions, answers) = match parse_response(data) {
        Some(r) => r,
        None => {
            log::debug!("dns: failed to parse response");
            return;
        }
    };

    // Check RCODE for errors
    let rcode = header.flags & RCODE_MASK;
    if rcode != 0 {
        log::warn!("dns: response error RCODE={}", rcode);
        return;
    }

    // Check if this matches our pending query
    let matches_pending = {
        let pending = PENDING_QUERY.lock();
        match *pending {
            Some((ref id, ref _name)) if *id == header.id => true,
            _ => false,
        }
    };

    // Extract A records
    let mut found_ip: Option<[u8; 4]> = None;
    for record in &answers {
        if record.rtype == TYPE_A && record.rdata.len() == 4 {
            let ip = [record.rdata[0], record.rdata[1], record.rdata[2], record.rdata[3]];
            log::info!(
                "dns: A record {} -> {}.{}.{}.{} ttl={}",
                record.name, ip[0], ip[1], ip[2], ip[3], record.ttl
            );
            cache_result(&record.name, ip, record.ttl);
            if found_ip.is_none() {
                found_ip = Some(ip);
            }
        }
    }

    // If this matches pending query, store result
    if matches_pending {
        if let Some(ip) = found_ip {
            *LAST_RESOLVE_RESULT.lock() = Some(ip);
            log::info!(
                "dns: resolved pending query -> {}.{}.{}.{}",
                ip[0], ip[1], ip[2], ip[3]
            );
        }
    }
}

/// Resolve a domain name to an IPv4 address.
///
/// Checks the cache first. If not cached, sends a DNS query via UDP to the
/// gateway (10.0.2.2) and polls for a response with a timeout.
///
/// Returns `Some([u8; 4])` on success, `None` on failure.
pub fn resolve(name: &str) -> Option<[u8; 4]> {
    // Check cache first
    if let Some(ip) = cache_lookup(name) {
        log::info!("dns: cache hit {} -> {}.{}.{}.{}", name, ip[0], ip[1], ip[2], ip[3]);
        return Some(ip);
    }

    // Ensure we have a UDP socket
    let sock_idx = ensure_socket()?;

    // Allocate a transaction ID
    let txid = {
        let mut id = NEXT_TXID.lock();
        let tid = *id;
        *id = id.wrapping_add(1);
        tid
    };

    // Build and send the query
    let query = build_query(name, txid);

    // Set up pending query state
    *PENDING_QUERY.lock() = Some((txid, String::from(name)));
    *LAST_RESOLVE_RESULT.lock() = None;

    // Send the DNS query via UDP to the gateway
    let sent = udp::sendto(sock_idx, GATEWAY_IP, DNS_PORT, &query);
    if !sent {
        log::warn!("dns: failed to send query for {}", name);
        *PENDING_QUERY.lock() = None;
        return None;
    }
    log::info!("dns: sent query for {} (txid={})", name, txid);

    // Poll for response with timeout
    for _ in 0..MAX_POLL_ITERATIONS {
        advance_clock();

        // Check if we got a response
        if let Some(ip) = *LAST_RESOLVE_RESULT.lock() {
            *PENDING_QUERY.lock() = None;
            return Some(ip);
        }

        // Poll network for incoming packets
        super::poll();
    }

    // Timeout
    log::warn!("dns: resolve timeout for {}", name);
    *PENDING_QUERY.lock() = None;
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_name() {
        let encoded = encode_name("example.com");
        assert_eq!(
            encoded,
            vec![7, b'e', b'x', b'a', b'm', b'p', b'l', b'e', 3, b'c', b'o', b'm', 0]
        );
    }

    #[test]
    fn test_decode_name_simple() {
        let data = vec![7, b'e', b'x', b'a', b'm', b'p', b'l', b'e', 3, b'c', b'o', b'm', 0];
        let (name, offset) = decode_name(&data, 0).unwrap();
        assert_eq!(name, "example.com");
        assert_eq!(offset, 13);
    }

    #[test]
    fn test_decode_name_compressed() {
        // Build a response-like buffer with a compressed name
        let mut data = vec![
            // Header (12 bytes)
            0x00, 0x01, 0x81, 0x80, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
            // Question: "example.com"
            7, b'e', b'x', b'a', b'm', b'p', b'l', b'e', 3, b'c', b'o', b'm', 0,
            // QTYPE=A, QCLASS=IN
            0x00, 0x01, 0x00, 0x01,
            // Answer: compressed name pointer to offset 12 (the question name)
            0xC0, 0x0C,
            // TYPE=A, CLASS=IN
            0x00, 0x01, 0x00, 0x01,
            // TTL = 300
            0x00, 0x00, 0x01, 0x2C,
            // RDLENGTH = 4
            0x00, 0x04,
            // RDATA = 93.184.216.34
            0x5D, 0xB8, 0xD8, 0x22,
        ];

        let (header, questions, answers) = parse_response(&data).unwrap();
        assert_eq!(header.id, 1);
        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0].name, "example.com");
        assert_eq!(answers.len(), 1);
        assert_eq!(answers[0].name, "example.com");
        assert_eq!(answers[0].rtype, TYPE_A);
        assert_eq!(answers[0].rdata, vec![0x5D, 0xB8, 0xD8, 0x22]);
    }

    #[test]
    fn test_build_query() {
        let query = build_query("example.com", 0x1234);
        // Header
        assert_eq!(query[0..2], [0x12, 0x34]); // ID
        // Flags: recursion desired
        assert_eq!(query[2..4], [0x01, 0x00]);
        // QDCOUNT = 1
        assert_eq!(query[4..6], [0x00, 0x01]);
        // Encoded name
        assert_eq!(
            query[12..25],
            [7, b'e', b'x', b'a', b'm', b'p', b'l', b'e', 3, b'c', b'o', b'm', 0]
        );
    }
}