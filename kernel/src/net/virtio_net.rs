//! Virtio network driver (virtio-net)
//!
//! Implements a legacy virtio-net driver for QEMU with PCI transport.
//! Uses I/O port access on x86_64. Supports both legacy and transitional
//! virtio-net-pci devices.

#![allow(dead_code)]

use spin::Mutex;

const VIRTIO_VENDOR_ID: u16 = 0x1AF4;
const VIRTIO_NET_DEVICE_ID: u16 = 0x1000;

const VIRTIO_STATUS_RESET: u8 = 0;
const VIRTIO_STATUS_ACK: u8 = 1;
const VIRTIO_STATUS_DRIVER: u8 = 2;
const VIRTIO_STATUS_DRIVER_OK: u8 = 4;
const VIRTIO_STATUS_FEATURES_OK: u8 = 8;

const VIRTIO_NET_F_MAC: u32 = 1 << 5;

const QUEUE_SIZE: u16 = 16;
const MAX_PACKET_SIZE: usize = 1526;

const VRING_DESC_F_NEXT: u16 = 1;
const VRING_DESC_F_WRITE: u16 = 2;

/// Virtqueue descriptor
#[repr(C, align(16))]
struct VirtDesc {
    addr: u64,
    len: u32,
    flags: u16,
    next: u16,
}

/// Available ring
#[repr(C)]
struct VirtAvail {
    flags: u16,
    idx: u16,
    ring: [u16; QUEUE_SIZE as usize],
    event: u16,
}

/// Used ring element
#[repr(C)]
#[derive(Clone, Copy)]
struct VirtUsedElem {
    id: u32,
    len: u32,
}

/// Used ring
#[repr(C, align(4096))]
struct VirtUsed {
    flags: u16,
    idx: u16,
    ring: [VirtUsedElem; QUEUE_SIZE as usize],
    event: u16,
}

/// Combined virtqueue (legacy layout: desc | avail | pad | used)
#[repr(C, align(4096))]
struct VirtQueue {
    desc: [VirtDesc; QUEUE_SIZE as usize],
    avail: VirtAvail,
    _pad: [u8; 4096 - (core::mem::size_of::<[VirtDesc; QUEUE_SIZE as usize]>() + core::mem::size_of::<VirtAvail>())],
    used: VirtUsed,
}

/// Virtio net packet header
#[repr(C)]
struct VirtioNetHdr {
    flags: u8,
    gso_type: u8,
    hdr_len: u16,
    gso_size: u16,
    csum_start: u16,
    csum_offset: u16,
    num_buffers: u16,
}

struct NetState {
    io_base: u16,
    mac: [u8; 6],
    rx_queue: *mut VirtQueue,
    tx_queue: *mut VirtQueue,
    rx_buffers: *mut [[u8; MAX_PACKET_SIZE]; QUEUE_SIZE as usize],
    tx_buffers: *mut [[u8; MAX_PACKET_SIZE]; QUEUE_SIZE as usize],
    rx_avail_idx: u16,
    tx_avail_idx: u16,
    rx_used_idx: u16,
    tx_used_idx: u16,
}

unsafe impl Send for NetState {}

static STATE: Mutex<Option<NetState>> = Mutex::new(None);

// ── I/O port helpers (x86_64) ─────────────────────────────────────────────

#[cfg(target_arch = "x86_64")]
unsafe fn inb(port: u16) -> u8 {
    let val: u8;
    core::arch::asm!("in al, dx", out("al") val, in("dx") port, options(nostack, preserves_flags));
    val
}
#[cfg(target_arch = "x86_64")]
unsafe fn inw(port: u16) -> u16 {
    let val: u16;
    core::arch::asm!("in ax, dx", out("ax") val, in("dx") port, options(nostack, preserves_flags));
    val
}
#[cfg(target_arch = "x86_64")]
unsafe fn inl(port: u16) -> u32 {
    let val: u32;
    core::arch::asm!("in eax, dx", out("eax") val, in("dx") port, options(nostack, preserves_flags));
    val
}
#[cfg(target_arch = "x86_64")]
unsafe fn outb(port: u16, val: u8) {
    core::arch::asm!("out dx, al", in("dx") port, in("al") val, options(nostack, preserves_flags));
}
#[cfg(target_arch = "x86_64")]
unsafe fn outw(port: u16, val: u16) {
    core::arch::asm!("out dx, ax", in("dx") port, in("ax") val, options(nostack, preserves_flags));
}
#[cfg(target_arch = "x86_64")]
unsafe fn outl(port: u16, val: u32) {
    core::arch::asm!("out dx, eax", in("dx") port, in("eax") val, options(nostack, preserves_flags));
}

#[cfg(not(target_arch = "x86_64"))]
unsafe fn inb(_port: u16) -> u8 { 0 }
#[cfg(not(target_arch = "x86_64"))]
unsafe fn inw(_port: u16) -> u16 { 0 }
#[cfg(not(target_arch = "x86_64"))]
unsafe fn inl(_port: u16) -> u32 { 0 }
#[cfg(not(target_arch = "x86_64"))]
unsafe fn outb(_port: u16, _val: u8) {}
#[cfg(not(target_arch = "x86_64"))]
unsafe fn outw(_port: u16, _val: u16) {}
#[cfg(not(target_arch = "x86_64"))]
unsafe fn outl(_port: u16, _val: u32) {}

// ── Legacy virtio register access ───────────────────────────────────────────

fn virtio_read_status(io_base: u16) -> u8 {
    unsafe { inb(io_base + 0x12) }
}
fn virtio_write_status(io_base: u16, val: u8) {
    unsafe { outb(io_base + 0x12, val) }
}
fn virtio_read_features(io_base: u16) -> u32 {
    unsafe { inl(io_base + 0x00) }
}
fn virtio_write_features(io_base: u16, val: u32) {
    unsafe { outl(io_base + 0x04, val) }
}
fn virtio_read_queue_size(io_base: u16) -> u16 {
    unsafe { inw(io_base + 0x0C) }
}
fn virtio_write_queue_select(io_base: u16, val: u16) {
    unsafe { outw(io_base + 0x0E, val) }
}
fn virtio_write_queue_pfn(io_base: u16, pfn: u32) {
    unsafe { outl(io_base + 0x08, pfn) }
}
fn virtio_write_queue_notify(io_base: u16, val: u16) {
    unsafe { outw(io_base + 0x10, val) }
}

// ── PCI config space access ────────────────────────────────────────────────

#[cfg(target_arch = "x86_64")]
fn pci_read_u8(bus: u8, device: u8, function: u8, offset: u8) -> u8 {
    let address = ((bus as u32) << 16) | ((device as u32) << 11) | ((function as u32) << 8) | (offset as u32) | 0x80000000;
    unsafe {
        core::arch::asm!(
            "out dx, eax",
            in("dx") 0xCF8u16,
            in("eax") address,
            options(nostack, preserves_flags)
        );
        let val: u8;
        core::arch::asm!(
            "in al, dx",
            in("dx") (0xCFC + (offset & 0x03) as u16),
            out("al") val,
            options(nostack, preserves_flags)
        );
        val
    }
}

#[cfg(target_arch = "x86_64")]
fn pci_read_u16(bus: u8, device: u8, function: u8, offset: u8) -> u16 {
    let address = ((bus as u32) << 16) | ((device as u32) << 11) | ((function as u32) << 8) | ((offset & 0xFC) as u32) | 0x80000000;
    unsafe {
        core::arch::asm!(
            "out dx, eax",
            in("dx") 0xCF8u16,
            in("eax") address,
            options(nostack, preserves_flags)
        );
        let val: u16;
        core::arch::asm!(
            "in ax, dx",
            in("dx") 0xCFCu16,
            out("ax") val,
            options(nostack, preserves_flags)
        );
        val
    }
}

#[cfg(target_arch = "x86_64")]
fn pci_read_u32(bus: u8, device: u8, function: u8, offset: u8) -> u32 {
    let address = ((bus as u32) << 16) | ((device as u32) << 11) | ((function as u32) << 8) | ((offset & 0xFC) as u32) | 0x80000000;
    unsafe {
        core::arch::asm!(
            "out dx, eax",
            in("dx") 0xCF8u16,
            in("eax") address,
            options(nostack, preserves_flags)
        );
        let val: u32;
        core::arch::asm!(
            "in eax, dx",
            in("dx") 0xCFCu16,
            out("eax") val,
            options(nostack, preserves_flags)
        );
        val
    }
}

#[cfg(not(target_arch = "x86_64"))]
fn pci_read_u8(_bus: u8, _device: u8, _function: u8, _offset: u8) -> u8 { 0 }
#[cfg(not(target_arch = "x86_64"))]
fn pci_read_u16(_bus: u8, _device: u8, _function: u8, _offset: u8) -> u16 { 0 }
#[cfg(not(target_arch = "x86_64"))]
fn pci_read_u32(_bus: u8, _device: u8, _function: u8, _offset: u8) -> u32 { 0 }

fn read_bar0_io(bus: u8, device: u8, function: u8) -> u16 {
    let bar0 = pci_read_u32(bus, device, function, 0x10);
    // Legacy virtio BAR0 is I/O-mapped: bit 0 = 1
    (bar0 & !0x3) as u16
}

fn find_virtio_net_device() -> Option<(u8, u8, u8)> {
    for bus in 0..=255 {
        for device in 0..=31 {
            for function in 0..=7 {
                let vendor = pci_read_u16(bus, device, function, 0x00);
                let dev_id = pci_read_u16(bus, device, function, 0x02);
                if vendor == VIRTIO_VENDOR_ID && dev_id == VIRTIO_NET_DEVICE_ID {
                    let class = pci_read_u8(bus, device, function, 0x0B);
                    let subclass = pci_read_u8(bus, device, function, 0x0A);
                    if class == 0x02 && subclass == 0x00 {
                        return Some((bus, device, function));
                    }
                }
            }
        }
    }
    None
}

// ── Memory allocation helpers ──────────────────────────────────────────────

fn alloc_queue() -> &'static mut VirtQueue {
    let size = core::mem::size_of::<VirtQueue>();
    let align = 4096;
    let layout = alloc::alloc::Layout::from_size_align(size, align).unwrap();
    let ptr = unsafe { alloc::alloc::alloc_zeroed(layout) as *mut VirtQueue };
    if ptr.is_null() {
        panic!("virtio-net: queue alloc failed");
    }
    unsafe { &mut *ptr }
}

fn alloc_buffers() -> *mut [[u8; MAX_PACKET_SIZE]; QUEUE_SIZE as usize] {
    let size = core::mem::size_of::<[[u8; MAX_PACKET_SIZE]; QUEUE_SIZE as usize]>();
    let layout = alloc::alloc::Layout::from_size_align(size, 16).unwrap();
    let ptr = unsafe { alloc::alloc::alloc_zeroed(layout) as *mut [[u8; MAX_PACKET_SIZE]; QUEUE_SIZE as usize] };
    if ptr.is_null() {
        panic!("virtio-net: buffer alloc failed");
    }
    ptr
}

// ── Queue setup ────────────────────────────────────────────────────────────

fn setup_queue(io_base: u16, queue_index: u16, queue: *mut VirtQueue) {
    virtio_write_queue_select(io_base, queue_index);
    let max_size = virtio_read_queue_size(io_base);
    log::info!("virtio-net: queue {} max_size={}", queue_index, max_size);

    let pfn = (queue as u64 / 4096) as u32;
    virtio_write_queue_pfn(io_base, pfn);
}

fn populate_rx_queue(state: &mut NetState) {
    unsafe {
        let queue = &mut *state.rx_queue;
        let buffers = &mut *state.rx_buffers;

        for i in 0..QUEUE_SIZE as usize {
            queue.desc[i] = VirtDesc {
                addr: &mut buffers[i][0] as *mut u8 as u64,
                len: MAX_PACKET_SIZE as u32,
                flags: VRING_DESC_F_WRITE,
                next: 0,
            };

            let idx = state.rx_avail_idx % QUEUE_SIZE;
            queue.avail.ring[idx as usize] = i as u16;
            state.rx_avail_idx += 1;
        }

        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
        queue.avail.idx = state.rx_avail_idx;

        virtio_write_queue_notify(state.io_base, 0);
    }
}

// ── Public API ─────────────────────────────────────────────────────────────

pub fn init() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        match find_virtio_net_device() {
            Some((bus, device, function)) => {
                let io_base = read_bar0_io(bus, device, function);
                log::info!("virtio-net: found at {}.{}.{}, I/O base 0x{:x}", bus, device, function, io_base);

                let mac = [
                    pci_read_u8(bus, device, function, 0x14),
                    pci_read_u8(bus, device, function, 0x15),
                    pci_read_u8(bus, device, function, 0x16),
                    pci_read_u8(bus, device, function, 0x17),
                    pci_read_u8(bus, device, function, 0x18),
                    pci_read_u8(bus, device, function, 0x19),
                ];

                // Reset device
                virtio_write_status(io_base, VIRTIO_STATUS_RESET);
                // Acknowledge
                virtio_write_status(io_base, VIRTIO_STATUS_ACK);
                // Driver present
                virtio_write_status(io_base, VIRTIO_STATUS_ACK | VIRTIO_STATUS_DRIVER);

                // Negotiate features
                let features = virtio_read_features(io_base);
                log::info!("virtio-net: host features = 0x{:x}", features);
                let guest_features = features & VIRTIO_NET_F_MAC;
                virtio_write_features(io_base, guest_features);

                // Features OK
                virtio_write_status(io_base, VIRTIO_STATUS_ACK | VIRTIO_STATUS_DRIVER | VIRTIO_STATUS_FEATURES_OK);

                // Allocate and setup queues
                let rx_queue = alloc_queue();
                let tx_queue = alloc_queue();
                let rx_buffers = alloc_buffers();
                let tx_buffers = alloc_buffers();

                setup_queue(io_base, 0, rx_queue);
                setup_queue(io_base, 1, tx_queue);

                let mut state = NetState {
                    io_base,
                    mac,
                    rx_queue,
                    tx_queue,
                    rx_buffers,
                    tx_buffers,
                    rx_avail_idx: 0,
                    tx_avail_idx: 0,
                    rx_used_idx: 0,
                    tx_used_idx: 0,
                };

                populate_rx_queue(&mut state);

                // DRIVER_OK
                virtio_write_status(io_base, VIRTIO_STATUS_ACK | VIRTIO_STATUS_DRIVER | VIRTIO_STATUS_FEATURES_OK | VIRTIO_STATUS_DRIVER_OK);

                *STATE.lock() = Some(state);
                log::info!("virtio-net: driver ready, MAC {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                    mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]);
                true
            }
            None => {
                log::warn!("virtio-net: no PCI device found");
                false
            }
        }
    }

    #[cfg(not(target_arch = "x86_64"))]
    {
        log::info!("virtio-net: aarch64 not yet supported");
        false
    }
}

pub fn get_mac() -> [u8; 6] {
    match STATE.lock().as_ref() {
        Some(state) => state.mac,
        None => [0x52, 0x54, 0x00, 0x12, 0x34, 0x56],
    }
}

/// Send an Ethernet frame. Prepends virtio-net header + Ethernet II header.
pub fn send_frame(dst_mac: [u8; 6], ethertype: u16, payload: &[u8]) -> bool {
    let mut guard = STATE.lock();
    let state = match guard.as_mut() {
        Some(s) => s,
        None => return false,
    };

    let vhdr_size = core::mem::size_of::<VirtioNetHdr>();
    let eth_hdr_size = 14;
    let total = vhdr_size + eth_hdr_size + payload.len();
    if total > MAX_PACKET_SIZE {
        log::warn!("virtio-net: packet too large: {}", total);
        return false;
    }

    // Simple queue-full check
    if (state.tx_avail_idx - state.tx_used_idx) >= QUEUE_SIZE {
        log::warn!("virtio-net: tx queue full");
        return false;
    }

    let desc_idx = (state.tx_avail_idx % QUEUE_SIZE) as usize;
    let buf = unsafe { &mut (*state.tx_buffers)[desc_idx] };

    // Build virtio-net header (all zeros = no checksum offload, no GSO)
    let vhdr = VirtioNetHdr {
        flags: 0,
        gso_type: 0,
        hdr_len: 0,
        gso_size: 0,
        csum_start: 0,
        csum_offset: 0,
        num_buffers: 0,
    };
    unsafe {
        core::ptr::write_unaligned(buf.as_mut_ptr() as *mut VirtioNetHdr, vhdr);
    }

    // Ethernet II header
    buf[vhdr_size..vhdr_size + 6].copy_from_slice(&dst_mac);
    buf[vhdr_size + 6..vhdr_size + 12].copy_from_slice(&state.mac);
    buf[vhdr_size + 12] = (ethertype >> 8) as u8;
    buf[vhdr_size + 13] = (ethertype & 0xFF) as u8;

    // Payload
    if !payload.is_empty() {
        buf[vhdr_size + eth_hdr_size..vhdr_size + eth_hdr_size + payload.len()].copy_from_slice(payload);
    }

    // Setup descriptor
    unsafe {
        let queue = &mut *state.tx_queue;
        queue.desc[desc_idx] = VirtDesc {
            addr: buf.as_mut_ptr() as u64,
            len: total as u32,
            flags: 0,
            next: 0,
        };

        // Add to avail ring
        let avail_slot = (state.tx_avail_idx % QUEUE_SIZE) as usize;
        queue.avail.ring[avail_slot] = desc_idx as u16;

        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
        state.tx_avail_idx += 1;
        queue.avail.idx = state.tx_avail_idx;
    }

    virtio_write_queue_notify(state.io_base, 1);
    true
}

/// Receive an Ethernet frame. Returns number of bytes copied into `buf`.
pub fn recv_frame(buf: &mut [u8]) -> usize {
    let mut guard = STATE.lock();
    let state = match guard.as_mut() {
        Some(s) => s,
        None => return 0,
    };

    // First, reap any completed TX descriptors
    unsafe {
        let tx_queue = &mut *state.tx_queue;
        let tx_used_idx = core::ptr::read_volatile(core::ptr::addr_of!(tx_queue.used.idx));
        while state.tx_used_idx != tx_used_idx {
            state.tx_used_idx += 1;
        }
    }

    // Check for received packets
    let used_idx: u16;
    let used_elem: VirtUsedElem;
    unsafe {
        let rx_queue = &mut *state.rx_queue;
        used_idx = core::ptr::read_volatile(core::ptr::addr_of!(rx_queue.used.idx));

        if used_idx == state.rx_used_idx {
            return 0;
        }

        let used_slot = (state.rx_used_idx % QUEUE_SIZE) as usize;
        used_elem = core::ptr::read_volatile(rx_queue.used.ring.as_ptr().add(used_slot));
    }

    let desc_idx = used_elem.id as usize;
    let len = used_elem.len as usize;

    if desc_idx >= QUEUE_SIZE as usize || len > MAX_PACKET_SIZE {
        state.rx_used_idx += 1;
        return 0;
    }

    // Skip virtio-net header
    let vhdr_size = core::mem::size_of::<VirtioNetHdr>();
    let data_len = if len > vhdr_size { len - vhdr_size } else { 0 };
    let copy_len = data_len.min(buf.len());

    if copy_len > 0 {
        let src_buf = unsafe { &(*state.rx_buffers)[desc_idx] };
        let src = &src_buf[vhdr_size..vhdr_size + copy_len];
        buf[..copy_len].copy_from_slice(src);
    }

    // Advance used index
    state.rx_used_idx += 1;

    // Re-add descriptor to RX avail ring
    unsafe {
        let rx_queue = &mut *state.rx_queue;
        let avail_slot = (state.rx_avail_idx % QUEUE_SIZE) as usize;
        core::ptr::write_volatile(rx_queue.avail.ring.as_mut_ptr().add(avail_slot), desc_idx as u16);
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
        state.rx_avail_idx += 1;
        core::ptr::write_volatile(core::ptr::addr_of_mut!(rx_queue.avail.idx), state.rx_avail_idx);
    }
    virtio_write_queue_notify(state.io_base, 0);

    copy_len
}
