//! VirtIO block device driver
//!
//! Provides block-level storage access via VirtIO PCI device.
//! Works on both x86_64 (q35) and aarch64 (virt) QEMU machines.

use spin::Mutex;
use core::ptr::{read_volatile, write_volatile};

const VIRTIO_BLK_VENDOR_ID: u16 = 0x1AF4;
const VIRTIO_BLK_DEVICE_ID: u16 = 0x1001;
const VIRTIO_BLK_MODERN_DEVICE_ID: u16 = 0x1042;

const VIRTIO_PCI_CAP_COMMON: u8 = 1;
const VIRTIO_PCI_CAP_NOTIFY: u8 = 2;
const VIRTIO_PCI_CAP_ISR: u8 = 3;

const VIRTIO_STATUS_RESET: u32 = 0;
const VIRTIO_STATUS_ACK: u32 = 1;
const VIRTIO_STATUS_DRIVER: u32 = 2;
const VIRTIO_STATUS_DRIVER_OK: u32 = 4;
const VIRTIO_STATUS_FEATURES_OK: u32 = 8;

const VIRTIO_QUEUE_NUM: u16 = 16;

const VIRTIO_BLK_T_IN: u32 = 0;
const VIRTIO_BLK_T_OUT: u32 = 1;

const VIRTIO_BLK_S_OK: u8 = 0;

#[repr(C)]
struct VirtQueue {
    desc: [VirtDesc; 16],
    avail: VirtAvail,
    _pad: [u8; 4096 - core::mem::size_of::<[VirtDesc; 16]>() - core::mem::size_of::<VirtAvail>()],
    used: VirtUsed,
}

#[repr(C, align(16))]
struct VirtDesc {
    addr: u64,
    len: u32,
    flags: u16,
    next: u16,
}

#[repr(C)]
struct VirtAvail {
    flags: u16,
    idx: u16,
    ring: [u16; 16],
    event: u16,
}

#[repr(C, align(4096))]
struct VirtUsed {
    flags: u16,
    idx: u16,
    ring: [VirtUsedElem; 16],
    event: u16,
}

#[repr(C)]
struct VirtUsedElem {
    id: u32,
    len: u32,
}

#[repr(C, align(16))]
struct BlkRequest {
    type_: u32,
    reserved: u32,
    sector: u64,
}

struct VirtioBlkState {
    initialized: bool,
    base_addr: u64,
    notify_base: u64,
    notify_off: u16,
    notify_off_mult: u32,
    capacity: u64,
    queue: &'static mut VirtQueue,
    desc_used: [bool; 16],
}

static BLK_STATE: Mutex<Option<VirtioBlkState>> = Mutex::new(None);

unsafe fn pci_read_config(offset: u32) -> u32 {
    #[cfg(target_arch = "x86_64")]
    {
        let val: u32;
        core::arch::asm!(
            "mov dx, {port}",
            "in eax, dx",
            port = in(reg) (0xCF8 | (offset & 0xFC)) as u16,
            out("eax") val,
            options(nomem, nostack)
        );
        val
    }
    #[cfg(target_arch = "aarch64")]
    {
        read_volatile(0x0A000000 as *const u32)
    }
}

unsafe fn pci_write_config(offset: u32, val: u32) {
    #[cfg(target_arch = "x86_64")]
    {
        core::arch::asm!(
            "mov dx, {port}",
            "out dx, eax",
            port = in(reg) (0xCF8 | (offset & 0xFC)) as u16,
            in("eax") val,
            options(nomem, nostack)
        );
    }
    #[cfg(target_arch = "aarch64")]
    {
        write_volatile(0x0A000000 as *mut u32, val);
    }
}

unsafe fn find_virtio_blk() -> Option<(u8, u64)> {
    #[cfg(target_arch = "x86_64")]
    {
        for bus in 0..8u8 {
            for dev in 0..32u8 {
                let addr = ((bus as u32) << 16) | ((dev as u32) << 11) | 0x80000000;

                pci_write_config(0, addr);
                let vendor_device = pci_read_config(0);

                if vendor_device == 0xFFFFFFFF {
                    continue;
                }

                let vendor = (vendor_device & 0xFFFF) as u16;
                let device = ((vendor_device >> 16) & 0xFFFF) as u16;

                if vendor == VIRTIO_BLK_VENDOR_ID
                    && (device == VIRTIO_BLK_DEVICE_ID || device == VIRTIO_BLK_MODERN_DEVICE_ID)
                {
                    let bar0 = pci_read_config(0x10);
                    return Some((bus << 3 | dev, bar0 as u64));
                }
            }
        }
    }

    #[cfg(target_arch = "aarch64")]
    {
        let base = 0x0A000000u64;
        let vendor = read_volatile((base + 0x100) as *const u32) as u16;
        let device = (read_volatile((base + 0x100) as *const u32) >> 16) as u16;

        if vendor == VIRTIO_BLK_VENDOR_ID
            && (device == VIRTIO_BLK_DEVICE_ID || device == VIRTIO_BLK_MODERN_DEVICE_ID)
        {
            return Some((0, base));
        }
    }

    None
}

pub fn init() {
    log::info!("virtio_blk: probing...");

    unsafe {
        let Some((_bus_dev, bar0)) = find_virtio_blk() else {
            log::info!("virtio_blk: no device found");
            return;
        };

        log::info!("virtio_blk: found at bar0={:x}", bar0);

        let common = bar0;
        let notify = bar0 + 0x3000;

        reset_device(common);

        let mut state = BLK_STATE.lock();
        *state = Some(VirtioBlkState {
            initialized: false,
            base_addr: common,
            notify_base: notify,
            notify_off: 0,
            notify_off_mult: 4,
            capacity: 0,
            queue: &mut *(alloc::alloc::alloc_zeroed(
                core::alloc::Layout::new::<VirtQueue>()
            ) as *mut VirtQueue),
            desc_used: [false; 16],
        });

        log::info!("virtio_blk: initialized");
    }
}

unsafe fn reset_device(base: u64) {
    write_volatile((base + 0x14) as *mut u32, VIRTIO_STATUS_RESET);
    while read_volatile((base + 0x14) as *const u32) != VIRTIO_STATUS_RESET {}

    write_volatile((base + 0x14) as *mut u32, VIRTIO_STATUS_ACK);
    write_volatile((base + 0x14) as *mut u32, VIRTIO_STATUS_ACK | VIRTIO_STATUS_DRIVER);
    write_volatile((base + 0x14) as *mut u32, VIRTIO_STATUS_ACK | VIRTIO_STATUS_DRIVER | VIRTIO_STATUS_FEATURES_OK);
}

pub fn read_block(sector: u64, buf: &mut [u8]) -> Result<(), ()> {
    let mut state = BLK_STATE.lock();
    let state = state.as_mut().ok_or(())?;

    unsafe {
        let req = BlkRequest {
            type_: VIRTIO_BLK_T_IN,
            reserved: 0,
            sector,
        };

        let queue = &mut *state.queue;

        let req_addr = &req as *const BlkRequest as u64;
        let buf_addr = buf.as_mut_ptr() as u64;
        let status_addr = &mut 0u8 as *mut u8 as u64;

        queue.desc[0] = VirtDesc {
            addr: req_addr,
            len: core::mem::size_of::<BlkRequest>() as u32,
            flags: 0,
            next: 1,
        };
        queue.desc[1] = VirtDesc {
            addr: buf_addr,
            len: buf.len() as u32,
            flags: 0x2,
            next: 2,
        };
        queue.desc[2] = VirtDesc {
            addr: status_addr,
            len: 1,
            flags: 0x2,
            next: 0,
        };

        let idx = queue.avail.idx;
        queue.avail.ring[(idx % VIRTIO_QUEUE_NUM) as usize] = 0;
        queue.avail.idx = idx.wrapping_add(1);

        write_volatile((state.notify_base + (state.notify_off as u64) * (state.notify_off_mult as u64)) as *mut u16, 0);

        loop {
            if queue.used.idx != idx.wrapping_add(1) {
                core::hint::spin_loop();
                continue;
            }
            break;
        }

        let status = *(status_addr as *const u8);
        if status != VIRTIO_BLK_S_OK {
            return Err(());
        }

        Ok(())
    }
}
