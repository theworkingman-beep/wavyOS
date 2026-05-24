//! Virtio network driver (virtio-net)
//!
//! Implements a virtio-net driver for QEMU with two transport modes:
//! - x86_64: PCI transport with I/O port access (legacy virtio-net-pci)
//! - aarch64: MMIO transport (virtio-net-device on QEMU virt machine)

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

/// Transport type: PCI (x86_64) or MMIO (aarch64)
#[derive(Clone, Copy, PartialEq)]
enum Transport {
    PciIo { io_base: u16 },
    Mmio { mmio_base: u64 },
}

struct NetState {
    transport: Transport,
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

// ── MMIO readl/writel helpers (aarch64) ────────────────────────────────────

/// Read a 32-bit value from an MMIO address
unsafe fn mmio_readl(addr: u64) -> u32 {
    core::ptr::read_volatile(addr as *const u32)
}

/// Write a 32-bit value to an MMIO address
unsafe fn mmio_writel(addr: u64, val: u32) {
    core::ptr::write_volatile(addr as *mut u32, val)
}

/// Read a 16-bit value from an MMIO address
unsafe fn mmio_readw(addr: u64) -> u16 {
    core::ptr::read_volatile(addr as *const u16)
}

/// Write a 16-bit value to an MMIO address
unsafe fn mmio_writew(addr: u64, val: u16) {
    core::ptr::write_volatile(addr as *mut u16, val)
}

/// Read an 8-bit value from an MMIO address
unsafe fn mmio_readb(addr: u64) -> u8 {
    core::ptr::read_volatile(addr as *const u8)
}

/// Write an 8-bit value to an MMIO address
unsafe fn mmio_writeb(addr: u64, val: u8) {
    core::ptr::write_volatile(addr as *mut u8, val)
}

// ── Legacy virtio PCI register access (x86_64 I/O port) ────────────────────

#[cfg(target_arch = "x86_64")]
fn virtio_read_status(io_base: u16) -> u8 {
    unsafe { inb(io_base + 0x12) }
}
#[cfg(target_arch = "x86_64")]
fn virtio_write_status(io_base: u16, val: u8) {
    unsafe { outb(io_base + 0x12, val) }
}
#[cfg(target_arch = "x86_64")]
fn virtio_read_features(io_base: u16) -> u32 {
    unsafe { inl(io_base + 0x00) }
}
#[cfg(target_arch = "x86_64")]
fn virtio_write_features(io_base: u16, val: u32) {
    unsafe { outl(io_base + 0x04, val) }
}
#[cfg(target_arch = "x86_64")]
fn virtio_read_queue_size(io_base: u16) -> u16 {
    unsafe { inw(io_base + 0x0C) }
}
#[cfg(target_arch = "x86_64")]
fn virtio_write_queue_select(io_base: u16, val: u16) {
    unsafe { outw(io_base + 0x0E, val) }
}
#[cfg(target_arch = "x86_64")]
fn virtio_write_queue_pfn(io_base: u16, pfn: u32) {
    unsafe { outl(io_base + 0x08, pfn) }
}
#[cfg(target_arch = "x86_64")]
fn virtio_write_queue_notify(io_base: u16, val: u16) {
    unsafe { outw(io_base + 0x10, val) }
}

// Dummy PCI register access stubs for non-x86_64 (never called, but needed for compile)
#[cfg(not(target_arch = "x86_64"))]
fn virtio_read_status(_io_base: u16) -> u8 { unreachable!() }
#[cfg(not(target_arch = "x86_64"))]
fn virtio_write_status(_io_base: u16, _val: u8) { unreachable!() }
#[cfg(not(target_arch = "x86_64"))]
fn virtio_read_features(_io_base: u16) -> u32 { unreachable!() }
#[cfg(not(target_arch = "x86_64"))]
fn virtio_write_features(_io_base: u16, _val: u32) { unreachable!() }
#[cfg(not(target_arch = "x86_64"))]
fn virtio_read_queue_size(_io_base: u16) -> u16 { unreachable!() }
#[cfg(not(target_arch = "x86_64"))]
fn virtio_write_queue_select(_io_base: u16, _val: u16) { unreachable!() }
#[cfg(not(target_arch = "x86_64"))]
fn virtio_write_queue_pfn(_io_base: u16, _pfn: u32) { unreachable!() }
#[cfg(not(target_arch = "x86_64"))]
fn virtio_write_queue_notify(_io_base: u16, _val: u16) { unreachable!() }

// ── MMIO virtio register access (aarch64) ─────────────────────────────────
//
// MMIO transport register layout (virtio spec):
//   0x000 MagicValue       (R)  0x74726956 ("virt")
//   0x004 Version          (R)  1 = legacy, 2 = modern
//   0x008 DeviceID         (R)  1 = net, 2 = blk, etc. 0 = end of list
//   0x00C VendorID        (R)
//   0x010 DeviceFeatures   (R)
//   0x014 DeviceFeaturesSel(W)
//   0x018 DriverFeatures   (W)
//   0x01C DriverFeaturesSel(W)
//   0x020 QueueSel         (W)  queue select
//   0x028 QueueNumMax      (R)  max queue size
//   0x030 QueueNum         (W)  selected queue size
//   0x034 QueueReady       (R)  1 = ready
//   0x038 QueueNotify      (W)  notify
//   0x040 InterruptStatus   (R)
//   0x044 InterruptACK     (W)
//   0x048 Status           (R/W)  device status
//   0x050 DeviceSpec       (R)  config space start
//   0x080 QueueDescLow     (W)  descriptor area address low
//   0x080 QueueDescHigh    (W)  descriptor area address high (offset 0x84)
//   0x090 QueueDriverLow   (W)  driver area (avail ring) address low
//   0x090 QueueDriverHigh  (W)  driver area address high (offset 0x94)
//   0x0a0 QueueDeviceLow   (W)  device area (used ring) address low
//   0x0a0 QueueDeviceHigh  (W)  device area address high (offset 0xa4)

const MMIO_MAGIC_VALUE: u32 = 0x74726956; // "virt" in little-endian
const MMIO_VERSION: u32 = 2; // We expect version 2 (modern MMIO)

const MMIO_OFFSET_MAGIC_VALUE: u64 = 0x000;
const MMIO_OFFSET_VERSION: u64 = 0x004;
const MMIO_OFFSET_DEVICE_ID: u64 = 0x008;
const MMIO_OFFSET_VENDOR_ID: u64 = 0x00C;
const MMIO_OFFSET_DEVICE_FEATURES: u64 = 0x010;
const MMIO_OFFSET_DEVICE_FEATURES_SEL: u64 = 0x014;
const MMIO_OFFSET_DRIVER_FEATURES: u64 = 0x018;
const MMIO_OFFSET_DRIVER_FEATURES_SEL: u64 = 0x01C;
const MMIO_OFFSET_QUEUE_SEL: u64 = 0x020;
const MMIO_OFFSET_QUEUE_NUM_MAX: u64 = 0x028;
const MMIO_OFFSET_QUEUE_NUM: u64 = 0x030;
const MMIO_OFFSET_QUEUE_READY: u64 = 0x034;
const MMIO_OFFSET_QUEUE_NOTIFY: u64 = 0x038;
const MMIO_OFFSET_INTERRUPT_STATUS: u64 = 0x040;
const MMIO_OFFSET_INTERRUPT_ACK: u64 = 0x044;
const MMIO_OFFSET_STATUS: u64 = 0x048;
const MMIO_OFFSET_CONFIG: u64 = 0x050;
const MMIO_OFFSET_QUEUE_DESC_LOW: u64 = 0x080;
const MMIO_OFFSET_QUEUE_DESC_HIGH: u64 = 0x084;
const MMIO_OFFSET_QUEUE_DRIVER_LOW: u64 = 0x090;
const MMIO_OFFSET_QUEUE_DRIVER_HIGH: u64 = 0x094;
const MMIO_OFFSET_QUEUE_DEVICE_LOW: u64 = 0x0a0;
const MMIO_OFFSET_QUEUE_DEVICE_HIGH: u64 = 0x0a4;

/// QEMU virt machine MMIO virtio device region starts at this address.
/// Each device occupies a 4KB region. Typically:
///   0x0A000000 - 0x0A0003FF: first virtio MMIO device
///   0x0A000400 - 0x0A0007FF: second virtio MMIO device
///   ...
/// Up to about 32 devices in the standard QEMU virt platform.
#[cfg(target_arch = "aarch64")]
const VIRTIO_MMIO_BASE: u64 = 0x0A000000;
#[cfg(target_arch = "aarch64")]
const VIRTIO_MMIO_SIZE: u64 = 0x200; // 512 bytes per device (4K stride but only first 512 used)
#[cfg(target_arch = "aarch64")]
const VIRTIO_MMIO_STRIDE: u64 = 0x200; // QEMU virt platform uses 512-byte stride
#[cfg(target_arch = "aarch64")]
const VIRTIO_MMIO_MAX_DEVICES: u32 = 32; // Scan up to 32 MMIO slots

/// MMIO register access functions
fn mmio_read_status(base: u64) -> u8 {
    unsafe { mmio_readl(base + MMIO_OFFSET_STATUS) as u8 }
}

fn mmio_write_status(base: u64, val: u8) {
    unsafe { mmio_writel(base + MMIO_OFFSET_STATUS, val as u32) }
}

fn mmio_read_device_features(base: u64) -> u32 {
    // Select feature bits set 0 first
    unsafe {
        mmio_writel(base + MMIO_OFFSET_DEVICE_FEATURES_SEL, 0);
        mmio_readl(base + MMIO_OFFSET_DEVICE_FEATURES)
    }
}

fn mmio_write_driver_features(base: u64, val: u32) {
    unsafe {
        // Select feature bits set 0
        mmio_writel(base + MMIO_OFFSET_DRIVER_FEATURES_SEL, 0);
        mmio_writel(base + MMIO_OFFSET_DRIVER_FEATURES, val);
    }
}

fn mmio_write_queue_select(base: u64, queue_index: u16) {
    unsafe { mmio_writel(base + MMIO_OFFSET_QUEUE_SEL, queue_index as u32) }
}

fn mmio_read_queue_num_max(base: u64) -> u16 {
    unsafe { mmio_readl(base + MMIO_OFFSET_QUEUE_NUM_MAX) as u16 }
}

fn mmio_write_queue_num(base: u64, size: u16) {
    unsafe { mmio_writel(base + MMIO_OFFSET_QUEUE_NUM, size as u32) }
}

fn mmio_read_queue_ready(base: u64) -> bool {
    unsafe { mmio_readl(base + MMIO_OFFSET_QUEUE_READY) != 0 }
}

fn mmio_write_queue_notify(base: u64, val: u16) {
    unsafe { mmio_writel(base + MMIO_OFFSET_QUEUE_NOTIFY, val as u32) }
}

/// Set up queue addresses for MMIO transport (modern virtio).
/// Writes the descriptor table, driver (avail) ring, and device (used) ring
/// addresses as split 32-bit low/high pairs.
fn mmio_setup_queue(base: u64, queue_index: u16, queue: *mut VirtQueue) {
    mmio_write_queue_select(base, queue_index);

    let queue_size = mmio_read_queue_num_max(base);
    if queue_size < QUEUE_SIZE {
        log::warn!("virtio-net MMIO: queue {} max_size={} < requested {}", queue_index, queue_size, QUEUE_SIZE);
    }
    let actual_size = QUEUE_SIZE.min(queue_size);
    mmio_write_queue_num(base, actual_size);

    // Calculate addresses of the three ring sections within the VirtQueue:
    // desc table starts at offset 0
    // avail ring starts after desc table
    // used ring starts after the padding (at VirtQueue.used offset)
    let desc_addr = queue as u64;
    let avail_addr = unsafe { core::ptr::addr_of!((*queue).avail) as u64 };
    let used_addr = unsafe { core::ptr::addr_of!((*queue).used) as u64 };

    unsafe {
        // Write descriptor area address
        mmio_writel(base + MMIO_OFFSET_QUEUE_DESC_LOW, desc_addr as u32);
        mmio_writel(base + MMIO_OFFSET_QUEUE_DESC_HIGH, (desc_addr >> 32) as u32);

        // Write driver area (avail ring) address
        mmio_writel(base + MMIO_OFFSET_QUEUE_DRIVER_LOW, avail_addr as u32);
        mmio_writel(base + MMIO_OFFSET_QUEUE_DRIVER_HIGH, (avail_addr >> 32) as u32);

        // Write device area (used ring) address
        mmio_writel(base + MMIO_OFFSET_QUEUE_DEVICE_LOW, used_addr as u32);
        mmio_writel(base + MMIO_OFFSET_QUEUE_DEVICE_HIGH, (used_addr >> 32) as u32);
    }

    // Mark queue ready
    unsafe {
        mmio_writel(base + MMIO_OFFSET_QUEUE_READY, 1);
    }

    log::info!("virtio-net MMIO: queue {} setup, size={}, desc={:#x}, avail={:#x}, used={:#x}",
        queue_index, actual_size, desc_addr, avail_addr, used_addr);
}

/// Read MAC address from MMIO config space (offset 0x050)
fn mmio_read_mac(base: u64) -> [u8; 6] {
    unsafe {
        let config_base = base + MMIO_OFFSET_CONFIG;
        [
            mmio_readb(config_base + 0),
            mmio_readb(config_base + 1),
            mmio_readb(config_base + 2),
            mmio_readb(config_base + 3),
            mmio_readb(config_base + 4),
            mmio_readb(config_base + 5),
        ]
    }
}

/// Scan QEMU virt platform MMIO region for a virtio-net device.
/// Returns the MMIO base address if found.
#[cfg(target_arch = "aarch64")]
fn mmio_find_virtio_net() -> Option<u64> {
    for i in 0..VIRTIO_MMIO_MAX_DEVICES {
        let base = VIRTIO_MMIO_BASE + (i as u64) * VIRTIO_MMIO_STRIDE;
        let magic = unsafe { mmio_readl(base + MMIO_OFFSET_MAGIC_VALUE) };
        if magic != MMIO_MAGIC_VALUE {
            continue;
        }
        let version = unsafe { mmio_readl(base + MMIO_OFFSET_VERSION) };
        if version != MMIO_VERSION {
            // Also accept version 1 (legacy MMIO) for compatibility
            if version != 1 {
                continue;
            }
        }
        let device_id = unsafe { mmio_readl(base + MMIO_OFFSET_DEVICE_ID) };
        if device_id == 0 {
            // Empty slot (device_id 0 means no device)
            continue;
        }
        let vendor_id = unsafe { mmio_readl(base + MMIO_OFFSET_VENDOR_ID) };
        // virtio-net has DeviceID = 1
        if device_id == 1 {
            log::info!("virtio-net MMIO: found at {:#x}, version={}, vendor={:#x}",
                base, version, vendor_id);
            return Some(base);
        }
    }
    None
}

// ── PCI config space access (x86_64) ────────────────────────────────────────

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

#[cfg(target_arch = "x86_64")]
fn read_bar0_io(bus: u8, device: u8, function: u8) -> u16 {
    let bar0 = pci_read_u32(bus, device, function, 0x10);
    // Legacy virtio BAR0 is I/O-mapped: bit 0 = 1
    (bar0 & !0x3) as u16
}
#[cfg(not(target_arch = "x86_64"))]
fn read_bar0_io(_bus: u8, _device: u8, _function: u8) -> u16 { unreachable!() }

#[cfg(target_arch = "x86_64")]
fn find_virtio_net_device() -> Option<(u8, u8, u8)> {
    for bus in 0..=255 {
        for device in 0..31 {
            for function in 0..7 {
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
#[cfg(not(target_arch = "x86_64"))]
fn find_virtio_net_device() -> Option<(u8, u8, u8)> { unreachable!() }

// Dummy PCI config space access stubs for non-x86_64 (never called)
#[cfg(not(target_arch = "x86_64"))]
fn pci_read_u8(_bus: u8, _device: u8, _function: u8, _offset: u8) -> u8 { unreachable!() }
#[cfg(not(target_arch = "x86_64"))]
fn pci_read_u16(_bus: u8, _device: u8, _function: u8, _offset: u8) -> u16 { unreachable!() }
#[cfg(not(target_arch = "x86_64"))]
fn pci_read_u32(_bus: u8, _device: u8, _function: u8, _offset: u8) -> u32 { unreachable!() }

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

// ── Transport-abstracted register access ───────────────────────────────────

fn transport_write_status(transport: Transport, val: u8) {
    match transport {
        Transport::PciIo { io_base } => virtio_write_status(io_base, val),
        Transport::Mmio { mmio_base } => mmio_write_status(mmio_base, val),
    }
}

fn transport_read_features(transport: Transport) -> u32 {
    match transport {
        Transport::PciIo { io_base } => virtio_read_features(io_base),
        Transport::Mmio { mmio_base } => mmio_read_device_features(mmio_base),
    }
}

fn transport_write_features(transport: Transport, val: u32) {
    match transport {
        Transport::PciIo { io_base } => virtio_write_features(io_base, val),
        Transport::Mmio { mmio_base } => mmio_write_driver_features(mmio_base, val),
    }
}

fn transport_write_queue_notify(transport: Transport, val: u16) {
    match transport {
        Transport::PciIo { io_base } => virtio_write_queue_notify(io_base, val),
        Transport::Mmio { mmio_base } => mmio_write_queue_notify(mmio_base, val),
    }
}

/// Setup queue using transport-specific mechanism
fn transport_setup_queue(transport: Transport, queue_index: u16, queue: *mut VirtQueue) {
    match transport {
        Transport::PciIo { io_base } => setup_queue_pci(io_base, queue_index, queue),
        Transport::Mmio { mmio_base } => mmio_setup_queue(mmio_base, queue_index, queue),
    }
}

// ── Queue setup ────────────────────────────────────────────────────────────

#[cfg(target_arch = "x86_64")]
fn setup_queue_pci(io_base: u16, queue_index: u16, queue: *mut VirtQueue) {
    virtio_write_queue_select(io_base, queue_index);
    let max_size = virtio_read_queue_size(io_base);
    log::info!("virtio-net PCI: queue {} max_size={}", queue_index, max_size);

    let pfn = (queue as u64 / 4096) as u32;
    virtio_write_queue_pfn(io_base, pfn);
}
#[cfg(not(target_arch = "x86_64"))]
fn setup_queue_pci(_io_base: u16, _queue_index: u16, _queue: *mut VirtQueue) { unreachable!() }

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

        transport_write_queue_notify(state.transport, 0);
    }
}

// ── Public API ─────────────────────────────────────────────────────────────

pub fn init() -> bool {
    // ── x86_64: PCI transport path ─────────────────────────────────────
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

                let transport = Transport::PciIo { io_base };

                // Reset device
                transport_write_status(transport, VIRTIO_STATUS_RESET);
                // Acknowledge
                transport_write_status(transport, VIRTIO_STATUS_ACK);
                // Driver present
                transport_write_status(transport, VIRTIO_STATUS_ACK | VIRTIO_STATUS_DRIVER);

                // Negotiate features
                let features = transport_read_features(transport);
                log::info!("virtio-net: host features = 0x{:x}", features);
                let guest_features = features & VIRTIO_NET_F_MAC;
                transport_write_features(transport, guest_features);

                // Features OK
                transport_write_status(transport, VIRTIO_STATUS_ACK | VIRTIO_STATUS_DRIVER | VIRTIO_STATUS_FEATURES_OK);

                // Allocate and setup queues
                let rx_queue = alloc_queue();
                let tx_queue = alloc_queue();
                let rx_buffers = alloc_buffers();
                let tx_buffers = alloc_buffers();

                transport_setup_queue(transport, 0, rx_queue);
                transport_setup_queue(transport, 1, tx_queue);

                let mut state = NetState {
                    transport,
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
                transport_write_status(transport, VIRTIO_STATUS_ACK | VIRTIO_STATUS_DRIVER | VIRTIO_STATUS_FEATURES_OK | VIRTIO_STATUS_DRIVER_OK);

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

    // ── aarch64: MMIO transport path ────────────────────────────────────
    #[cfg(target_arch = "aarch64")]
    {
        match mmio_find_virtio_net() {
            Some(mmio_base) => {
                log::info!("virtio-net MMIO: device at {:#x}", mmio_base);

                // Check if MAC feature is offered
                let transport = Transport::Mmio { mmio_base };

                // Read MAC from config space (always read, may be zero if feature not negotiated)
                let mac = mmio_read_mac(mmio_base);

                // Reset device
                mmio_write_status(mmio_base, VIRTIO_STATUS_RESET);
                // Wait for reset to take effect
                let mut retries = 0;
                while mmio_read_status(mmio_base) != VIRTIO_STATUS_RESET && retries < 100 {
                    retries += 1;
                }

                // Acknowledge
                mmio_write_status(mmio_base, VIRTIO_STATUS_ACK);
                // Driver present
                mmio_write_status(mmio_base, VIRTIO_STATUS_ACK | VIRTIO_STATUS_DRIVER);

                // Negotiate features
                let features = mmio_read_device_features(mmio_base);
                log::info!("virtio-net MMIO: host features = 0x{:x}", features);
                let guest_features = features & VIRTIO_NET_F_MAC;
                mmio_write_driver_features(mmio_base, guest_features);

                // Features OK
                mmio_write_status(mmio_base, VIRTIO_STATUS_ACK | VIRTIO_STATUS_DRIVER | VIRTIO_STATUS_FEATURES_OK);

                // Verify FEATURES_OK is still set (device may clear it if features are unacceptable)
                let status = mmio_read_status(mmio_base);
                if status & VIRTIO_STATUS_FEATURES_OK == 0 {
                    log::error!("virtio-net MMIO: device rejected features, status={:#x}", status);
                    return false;
                }

                // Allocate and setup queues
                let rx_queue = alloc_queue();
                let tx_queue = alloc_queue();
                let rx_buffers = alloc_buffers();
                let tx_buffers = alloc_buffers();

                // MMIO queue setup (writes descriptor/avail/used addresses)
                mmio_setup_queue(mmio_base, 0, rx_queue);
                mmio_setup_queue(mmio_base, 1, tx_queue);

                // Re-read MAC after feature negotiation (now that VIRTIO_NET_F_MAC is negotiated)
                let mac = if guest_features & VIRTIO_NET_F_MAC != 0 {
                    mmio_read_mac(mmio_base)
                } else {
                    mac
                };

                let mut state = NetState {
                    transport,
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
                mmio_write_status(mmio_base, VIRTIO_STATUS_ACK | VIRTIO_STATUS_DRIVER | VIRTIO_STATUS_FEATURES_OK | VIRTIO_STATUS_DRIVER_OK);

                // Enable IRQ for virtio-net (SPI 28 in QEMU virt, but varies)
                // On QEMU virt, virtio MMIO devices use IRQs starting from 48 (SPI 16 + 32)
                // The exact mapping depends on QEMU command line; we'll handle this later.

                *STATE.lock() = Some(state);
                log::info!("virtio-net MMIO: driver ready, MAC {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                    mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]);
                true
            }
            None => {
                log::warn!("virtio-net MMIO: no device found");
                false
            }
        }
    }

    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        log::info!("virtio-net: unsupported architecture");
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

    transport_write_queue_notify(state.transport, 1);
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
    transport_write_queue_notify(state.transport, 0);

    copy_len
}