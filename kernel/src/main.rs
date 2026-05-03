//! Vibe Coded OS — Kernel Entry Point
#![no_std]
#![no_main]
#![feature(asm_const)]
#![feature(naked_functions)]

extern crate alloc;

use core::panic::PanicInfo;
extern crate log;

pub use common::{BootInfo, FramebufferInfo, MemoryRegion, MemoryRegionKind};

#[cfg(target_arch = "x86_64")]
extern "C" fn init_task() -> ! {
    log::info!("init: Vibe Coded OS running");
    userland::run_shell();
}

#[cfg(target_arch = "aarch64")]
extern "C" fn init_task() -> ! {
    log::info!("init: Vibe Coded OS running");
    userland::run_shell();
}

mod arch;
mod mm;
mod scheduler;
mod syscalls;
mod ipc;
mod compat;
mod drivers;
mod userland;

#[cfg(target_arch = "x86_64")]
use arch::x86_64 as arch_impl;

#[cfg(target_arch = "aarch64")]
use arch::aarch64 as arch_impl;

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

    drivers::uart::init();
    drivers::uart_logger::init();
    log::info!("kernel_main entered");

    if !bi.framebuffer.is_null() {
        unsafe { drivers::fbcon::init(&*bi.framebuffer); }
    }

    arch_impl::init(unsafe { &mut *boot_info });
    mm::init(mem_map);
    scheduler::init();
    ipc::init();
    compat::init();
    userland::init();
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
