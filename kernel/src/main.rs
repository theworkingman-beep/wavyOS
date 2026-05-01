//! Vibe Coded OS — Kernel Entry Point
#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![feature(asm_const)]
#![feature(naked_functions)]

extern crate alloc;

use core::panic::PanicInfo;
extern crate log;

mod arch;
mod mm;
mod scheduler;
mod syscalls;
mod compat;

#[cfg(target_arch = "x86_64")]
use arch::x86_64 as arch_impl;

#[cfg(target_arch = "aarch64")]
use arch::aarch64 as arch_impl;

/// Kernel entry point — called by the bootloader.
#[no_mangle]
pub extern "C" fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    arch_impl::init(boot_info);
    mm::init(boot_info.memory_map);
    scheduler::init();
    compat::init();
    syscalls::init();

    log::info!("Vibe Coded OS kernel initialized. Scheduling first task.");

    scheduler::run_first_task();
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    log::error!("KERNEL PANIC: {}", info);
    arch_impl::halt_loop();
}

/// Boot information passed by the bootloader.
#[derive(Debug, Clone, Copy)]
pub struct BootInfo {
    pub memory_map: &'static [MemoryRegion],
    pub framebuffer: Option<FramebufferInfo>,
    pub rsdp: Option<u64>,
    pub device_tree: Option<u64>,
}

#[derive(Debug, Clone, Copy)]
pub struct MemoryRegion {
    pub base: u64,
    pub length: u64,
    pub kind: MemoryRegionKind,
}

#[derive(Debug, Clone, Copy)]
pub enum MemoryRegionKind {
    Usable,
    Reserved,
    Bootloader,
    Kernel,
}

#[derive(Debug, Clone, Copy)]
pub struct FramebufferInfo {
    pub addr: u64,
    pub width: u32,
    pub height: u32,
    pub pitch: u32,
    pub bpp: u8,
}
