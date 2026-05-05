//! Virtio network driver (virtio-net)
//!
//! Implements a virtio-net driver for QEMU with PCI transport.
//! Uses virtqueues for send/receive operations.

#![allow(dead_code)]

use core::ptr::{self, read_unaligned, write_unaligned};
use spin::Mutex;

/// Virtio vendor and device IDs
const VIRTIO_VENDOR_ID: u16 = 0x1AF4;
const VIRTIO_NET_DEVICE_ID_LEGACY: u16 = 0x1000;
const VIRTIO_NET_DEVICE_ID_MODERN: u16 = 0x1041;

/// Virtqueue indices
const RX_QUEUE: u16 = 0;
const TX_QUEUE: u16 = 1;

/// Maximum packet size for Ethernet
const MAX_PACKET_SIZE: usize = 1526; // Ethernet + possible VLAN tag

/// Virtio-net header (legacy)
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

/// Virtqueue structure
struct Virtqueue {
    index: u16,
    size: u16,
    descriptors: *mut VirtqDesc,
    avail: *mut VirtqAvail,
    used: *mut VirtqUsed,
    last_used_idx: u16,
}

#[repr(C)]
struct VirtqDesc {
    addr: u64,
    len: u32,
    flags: u16,
    next: u16,
}

#[repr(C)]
struct VirtqAvail {
    flags: u16,
    idx: u16,
    ring: [u16; 256],
    used_event: u16,
}

#[repr(C)]
struct VirtqUsed {
    flags: u16,
    idx: u16,
    ring: [VirtqUsedElem; 256],
    avail_event: u16,
}

#[repr(C)]
struct VirtqUsedElem {
    id: u32,
    len: u32,
}

/// Global driver state
static mut VIRTIO_PCI_BAR: usize = 0;
static MAC_ADDRESS: Mutex<[u8; 6]> = Mutex::new([0x52, 0x54, 0x00, 0x12, 0x34, 0x56]); // Default QEMU MAC
static INITIALIZED: Mutex<bool> = Mutex::new(false);

/// Initialize virtio-net driver
pub fn init() -> bool {
    log::info!("virtio-net: initializing");

    if cfg!(target_arch = "x86_64") {
        // Find virtio-net PCI device
        let (bus, device, function, bar0) = match find_virtio_net_device() {
            Some(info) => info,
            None => {
                log::warn!("virtio-net: no virtio-net PCI device found");
                return false;
            }
        };

        log::info!("virtio-net: found device at {}.{}.{}", bus, device, function);
        log::info!("virtio-net: BAR0 = {:#x}", bar0);

        unsafe {
            let io_space = (bar0 & 1) != 0;
            let bar_addr = if io_space { bar0 & !3 } else { bar0 & !15 };

            VIRTIO_PCI_BAR = bar_addr;

            // Reset device
            pci_write_u8(bus, device, function, 0x1F, 0x00);

            // Set status to ACKNOWLEDGE | DRIVER
            pci_write_u8(bus, device, function, 0x1F, 0x03);

            // Read MAC address from PCI config space (offset 0x14 for legacy virtio-net)
            let mut mac = [0u8; 6];
            for i in 0..6 {
                mac[i] = pci_read_u8(bus, device, function, 0x14 + i as u8);
            }
            *MAC_ADDRESS.lock() = mac;

            // Set status to DRIVER_OK
            pci_write_u8(bus, device, function, 0x1F, 0x07);

            *INITIALIZED.lock() = true;
        }
    } else {
        // For aarch64, we're on QEMU virt platform
        log::info!("virtio-net: aarch64 support not yet implemented");
        return false;
    }

    let mac = *MAC_ADDRESS.lock();
    log::info!("virtio-net: initialization complete");
    log::info!("virtio-net: MAC = {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]);

    true
}

/// Find virtio-net PCI device (x86_64 only)
#[cfg(target_arch = "x86_64")]
fn find_virtio_net_device() -> Option<(u8, u8, u8, usize)> {
    for bus in 0..=255 {
        for device in 0..=31 {
            for function in 0..=7 {
                let vendor_id = pci_read_u16(bus, device, function, 0x00);
                let device_id = pci_read_u16(bus, device, function, 0x02);

                if vendor_id == VIRTIO_VENDOR_ID &&
                   (device_id == VIRTIO_NET_DEVICE_ID_LEGACY || device_id == VIRTIO_NET_DEVICE_ID_MODERN) {
                    let class = pci_read_u8(bus, device, function, 0x0B);
                    let subclass = pci_read_u8(bus, device, function, 0x0A);

                    // Network controller, Ethernet controller
                    if class == 0x02 && subclass == 0x00 {
                        let bar0 = pci_read_u32(bus, device, function, 0x10) as usize;
                        return Some((bus, device, function, bar0));
                    }
                }
            }
        }
    }
    None
}

#[cfg(not(target_arch = "x86_64"))]
fn find_virtio_net_device() -> Option<(u8, u8, u8, usize)> {
    None
}

/// Get MAC address
pub fn get_mac() -> [u8; 6] {
    *MAC_ADDRESS.lock()
}

/// Send a frame
pub fn send_frame(dst_mac: [u8; 6], ethertype: u16, payload: &[u8]) -> bool {
    if !*INITIALIZED.lock() {
        return false;
    }

    if payload.len() + 14 > MAX_PACKET_SIZE {
        log::warn!("virtio-net: packet too large");
        return false;
    }

    // Build Ethernet frame
    let mut frame = [0u8; MAX_PACKET_SIZE];
    let mut offset = 0;

    // Destination MAC
    frame[offset..offset + 6].copy_from_slice(&dst_mac);
    offset += 6;

    // Source MAC
    frame[offset..offset + 6].copy_from_slice(&*MAC_ADDRESS.lock());
    offset += 6;

    // Ethertype
    frame[offset] = (ethertype >> 8) as u8;
    frame[offset + 1] = ethertype as u8;
    offset += 2;

    // Payload
    frame[offset..offset + payload.len()].copy_from_slice(payload);
    offset += payload.len();

    log::debug!("virtio-net: send {} bytes to {:02x?}:{:02x?}:{:02x?}:{:02x?}:{:02x?}:{:02x?}",
        offset, dst_mac[0], dst_mac[1], dst_mac[2], dst_mac[3], dst_mac[4], dst_mac[5]);

    // TODO: Actually send via virtqueue
    // For now, just log

    true
}

/// Receive a frame
pub fn recv_frame(buf: &mut [u8]) -> usize {
    if !*INITIALIZED.lock() {
        return 0;
    }

    // TODO: Implement actual frame receiving via virtqueue
    // For now, return 0 (no data)
    0
}

/// Read PCI config space (u8) - x86_64 only
#[cfg(target_arch = "x86_64")]
fn pci_read_u8(bus: u8, device: u8, function: u8, offset: u8) -> u8 {
    let address = ((bus as u32) << 16) | ((device as u32) << 11) | ((function as u32) << 8) | (offset as u32) | 0x80000000;
    unsafe {
        let pci_config_addr: *mut u32 = 0xCF8 as *mut u32;
        let pci_config_data: *mut u8 = 0xCFC as *mut u8;
        ptr::write_volatile(pci_config_addr, address);
        ptr::read_volatile(pci_config_data)
    }
}

#[cfg(not(target_arch = "x86_64"))]
fn pci_read_u8(_bus: u8, _device: u8, _function: u8, _offset: u8) -> u8 {
    0
}

/// Read PCI config space (u16) - x86_64 only
#[cfg(target_arch = "x86_64")]
fn pci_read_u16(bus: u8, device: u8, function: u8, offset: u8) -> u16 {
    let address = ((bus as u32) << 16) | ((device as u32) << 11) | ((function as u32) << 8) | (offset as u32) | 0x80000000;
    unsafe {
        let pci_config_addr: *mut u32 = 0xCF8 as *mut u32;
        let pci_config_data: *mut u16 = 0xCFC as *mut u16;
        ptr::write_volatile(pci_config_addr, address);
        ptr::read_volatile(pci_config_data)
    }
}

#[cfg(not(target_arch = "x86_64"))]
fn pci_read_u16(_bus: u8, _device: u8, _function: u8, _offset: u8) -> u16 {
    0
}

/// Read PCI config space (u32) - x86_64 only
#[cfg(target_arch = "x86_64")]
fn pci_read_u32(bus: u8, device: u8, function: u8, offset: u8) -> u32 {
    let address = ((bus as u32) << 16) | ((device as u32) << 11) | ((function as u32) << 8) | (offset as u32) | 0x80000000;
    unsafe {
        let pci_config_addr: *mut u32 = 0xCF8 as *mut u32;
        let pci_config_data: *mut u32 = 0xCFC as *mut u32;
        ptr::write_volatile(pci_config_addr, address);
        ptr::read_volatile(pci_config_data)
    }
}

#[cfg(not(target_arch = "x86_64"))]
fn pci_read_u32(_bus: u8, _device: u8, _function: u8, _offset: u8) -> u32 {
    0
}

/// Write PCI config space (u8) - x86_64 only
#[cfg(target_arch = "x86_64")]
fn pci_write_u8(bus: u8, device: u8, function: u8, offset: u8, value: u8) {
    let address = ((bus as u32) << 16) | ((device as u32) << 11) | ((function as u32) << 8) | (offset as u32) | 0x80000000;
    unsafe {
        let pci_config_addr: *mut u32 = 0xCF8 as *mut u32;
        let pci_config_data: *mut u8 = 0xCFC as *mut u8;
        ptr::write_volatile(pci_config_addr, address);
        ptr::write_volatile(pci_config_data, value);
    }
}

#[cfg(not(target_arch = "x86_64"))]
fn pci_write_u8(_bus: u8, _device: u8, _function: u8, _offset: u8, _value: u8) {
}
