//! Virtio network driver (virtio-net)
//!
//! Implements a virtio-net driver for QEMU with PCI transport.

#![allow(dead_code)]

use spin::Mutex;

/// Virtio vendor and device IDs
const VIRTIO_VENDOR_ID: u16 = 0x1AF4;
const VIRTIO_NET_DEVICE_ID: u16 = 0x1000;

/// Global driver state
static MAC_ADDRESS: Mutex<[u8; 6]> = Mutex::new([0x52, 0x54, 0x00, 0x12, 0x34, 0x56]);
static INITIALIZED: Mutex<bool> = Mutex::new(false);

/// Initialize virtio-net driver
pub fn init() -> bool {
    log::info!("virtio-net: initializing");

    #[cfg(target_arch = "x86_64")]
    {
        match find_virtio_net_device() {
            Some((bus, device, function)) => {
                log::info!("virtio-net: found device at {}.{}.{}", bus, device, function);

                // Reset device
                pci_write_u8(bus, device, function, 0x1F, 0x00);
                // Set status to ACKNOWLEDGE | DRIVER
                pci_write_u8(bus, device, function, 0x1F, 0x03);

                // Read MAC address
                let mut mac = [0u8; 6];
                for i in 0..6 {
                    mac[i] = pci_read_u8(bus, device, function, 0x14 + i as u8);
                }
                *MAC_ADDRESS.lock() = mac;

                // Set status to DRIVER_OK
                pci_write_u8(bus, device, function, 0x1F, 0x07);

                *INITIALIZED.lock() = true;
            }
            None => {
                log::warn!("virtio-net: no device found");
                return false;
            }
        }
    }

    #[cfg(not(target_arch = "x86_64"))]
    {
        log::info!("virtio-net: aarch64 not yet supported");
        return false;
    }

    let mac = *MAC_ADDRESS.lock();
    log::info!("virtio-net: MAC = {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]);
    true
}

/// Find virtio-net PCI device
#[cfg(target_arch = "x86_64")]
fn find_virtio_net_device() -> Option<(u8, u8, u8)> {
    for bus in 0..=255 {
        for device in 0..=31 {
            for function in 0..=7 {
                let vendor_id = pci_read_u16(bus, device, function, 0x00);
                let device_id = pci_read_u16(bus, device, function, 0x02);

                if vendor_id == VIRTIO_VENDOR_ID && device_id == VIRTIO_NET_DEVICE_ID {
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
fn find_virtio_net_device() -> Option<(u8, u8, u8)> {
    None
}

/// Get MAC address
pub fn get_mac() -> [u8; 6] {
    *MAC_ADDRESS.lock()
}

/// Send a frame (stub)
pub fn send_frame(_dst_mac: [u8; 6], _ethertype: u16, _payload: &[u8]) -> bool {
    if !*INITIALIZED.lock() {
        return false;
    }
    log::debug!("virtio-net: send frame (stub)");
    true
}

/// Receive a frame (stub)
pub fn recv_frame(_buf: &mut [u8]) -> usize {
    if !*INITIALIZED.lock() {
        return 0;
    }
    0
}

/// Read PCI config (u8)
#[cfg(target_arch = "x86_64")]
fn pci_read_u8(bus: u8, device: u8, function: u8, offset: u8) -> u8 {
    let address = ((bus as u32) << 16) | ((device as u32) << 11) | ((function as u32) << 8) | (offset as u32) | 0x80000000;
    unsafe {
        let pci_config_addr: *mut u32 = 0xCF8 as *mut u32;
        let pci_config_data: *mut u8 = 0xCFC as *mut u8;
        core::ptr::write_volatile(pci_config_addr, address);
        core::ptr::read_volatile(pci_config_data)
    }
}

#[cfg(not(target_arch = "x86_64"))]
fn pci_read_u8(_bus: u8, _device: u8, _function: u8, _offset: u8) -> u8 {
    0
}

/// Read PCI config (u16)
#[cfg(target_arch = "x86_64")]
fn pci_read_u16(bus: u8, device: u8, function: u8, offset: u8) -> u16 {
    let address = ((bus as u32) << 16) | ((device as u32) << 11) | ((function as u32) << 8) | (offset as u32) | 0x80000000;
    unsafe {
        let pci_config_addr: *mut u32 = 0xCF8 as *mut u32;
        let pci_config_data: *mut u16 = 0xCFC as *mut u16;
        core::ptr::write_volatile(pci_config_addr, address);
        core::ptr::read_volatile(pci_config_data)
    }
}

#[cfg(not(target_arch = "x86_64"))]
fn pci_read_u16(_bus: u8, _device: u8, _function: u8, _offset: u8) -> u16 {
    0
}

/// Write PCI config (u8)
#[cfg(target_arch = "x86_64")]
fn pci_write_u8(bus: u8, device: u8, function: u8, offset: u8, value: u8) {
    let address = ((bus as u32) << 16) | ((device as u32) << 11) | ((function as u32) << 8) | (offset as u32) | 0x80000000;
    unsafe {
        let pci_config_addr: *mut u32 = 0xCF8 as *mut u32;
        let pci_config_data: *mut u8 = 0xCFC as *mut u8;
        core::ptr::write_volatile(pci_config_addr, address);
        core::ptr::write_volatile(pci_config_data, value);
    }
}

#[cfg(not(target_arch = "x86_64"))]
fn pci_write_u8(_bus: u8, _device: u8, _function: u8, _offset: u8, _value: u8) {
}
