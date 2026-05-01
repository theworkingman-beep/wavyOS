//! UEFI bootloader stub — minimal halting entry
#![no_std]
#![no_main]
#![feature(abi_efiapi)]

extern crate alloc;

use uefi::prelude::*;
use uefi_services::{init, println};
use uefi::proto::console::gop::GraphicsOutput;

#[entry]
fn main(_handle: Handle, mut system_table: SystemTable<Boot>) -> Status {
    init(&mut system_table).unwrap();
    println!("Vibe Coded OS Bootloader");

    let bs = system_table.boot_services();

    let gop_handle = bs.get_handle_for_protocol::<GraphicsOutput>();
    if let Ok(handle) = gop_handle {
        let gop = bs.open_protocol_exclusive::<GraphicsOutput>(handle);
        if let Ok(mut gop) = gop {
            let mode = gop.current_mode_info();
            let (w, h) = mode.resolution();
            let mut fb = gop.frame_buffer();
            println!("Framebuf addr={:?} stride={}", fb.as_mut_ptr(), mode.stride());
        }
    }

    println!("Kernel handoff not yet implemented.");
    loop {
        #[cfg(target_arch = "x86_64")]
        unsafe { core::arch::asm!("hlt") };
        #[cfg(target_arch = "aarch64")]
        unsafe { core::arch::asm!("wfe") };
    }
}
