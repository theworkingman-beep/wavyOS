//! Vibe Coded OS — Kernel Entry Point
#![no_std]
#![no_main]
#![feature(asm_const)]
#![feature(naked_functions)]

extern crate alloc;

use core::panic::PanicInfo;
extern crate log;

#[cfg(target_arch = "x86_64")]
extern "C" fn init_task() -> ! {
    log::info!("init: Vibe Coded OS running");
    loop { hal::x86_64::halt(); }
}

#[cfg(target_arch = "aarch64")]
extern "C" fn init_task() -> ! {
    log::info!("init: Vibe Coded OS running");
    loop { hal::aarch64::halt(); }
}

mod arch;
mod mm;
mod scheduler;
mod syscalls;
mod compat;

#[cfg(target_arch = "x86_64")]
use arch::x86_64 as arch_impl;

#[cfg(target_arch = "aarch64")]
use arch::aarch64 as arch_impl;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub enum MemoryRegionKind {
    Usable,
    Reserved,
    Bootloader,
    Kernel,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct MemoryRegion {
    pub base: u64,
    pub length: u64,
    pub kind: MemoryRegionKind,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FramebufferInfo {
    pub addr: u64,
    pub width: u32,
    pub height: u32,
    pub pitch: u32,
    pub bpp: u8,
}

#[repr(C)]
pub struct BootInfo {
    pub memory_map_ptr: *const MemoryRegion,
    pub memory_map_len: usize,
    pub framebuffer: *const FramebufferInfo,
    pub rsdp: u64,
    pub device_tree: u64,
}

#[no_mangle]
pub extern "C" fn kernel_main(boot_info: *mut BootInfo) -> ! {
    if boot_info.is_null() {
        loop {
            #[cfg(target_arch = "x86_64")]
            unsafe { core::arch::asm!("hlt"); }
            #[cfg(target_arch = "aarch64")]
            unsafe { core::arch::asm!("wfe"); }
        }
    }
    let bi = unsafe { &*boot_info };
    let mem_map = if bi.memory_map_ptr.is_null() || bi.memory_map_len == 0 {
        &[] as &[MemoryRegion]
    } else {
        unsafe { core::slice::from_raw_parts(bi.memory_map_ptr, bi.memory_map_len) }
    };

    arch_impl::init(unsafe { &mut *boot_info });
    mm::init(mem_map);
    scheduler::init();
    compat::init();
    syscalls::init();

    log::info!("Vibe Coded OS kernel initialized. Spawning init task.");
    scheduler::spawn(init_task, None);
    scheduler::run_first_task();
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    log::error!("KERNEL PANIC: {}", info);
    arch_impl::halt_loop();
}
