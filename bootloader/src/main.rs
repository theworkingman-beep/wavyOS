//! UEFI bootloader for Vibe Coded OS
#![no_std]
#![no_main]
#![feature(abi_efiapi)]

extern crate alloc;

use uefi::prelude::*;
use uefi::proto::console::gop::GraphicsOutput;
use uefi_services::{init, println};

#[entry]
fn main(_handle: Handle, mut system_table: SystemTable<Boot>) -> Status {
    init(&mut system_table).unwrap();
    println!("Vibe Coded OS Bootloader");

    // Minimal memory map
    let mut buffer = alloc::vec![0u8; 4096 * 4];
    let mem_map = system_table.boot_services().memory_map(&mut buffer).unwrap();
    let mut mem_regions: alloc::vec::Vec<u64> = alloc::vec![];
    for desc in mem_map.entries() {
        mem_regions.push(desc.phys_start as u64);
    }
    println!("Memory regions: {}", mem_regions.len());

    // Locate GOP via open_protocol_exclusive
    let gop_handle = system_table
        .boot_services()
        .get_handle_for_protocol::<GraphicsOutput>();
    if let Ok(handle) = gop_handle {
        let gop = system_table
            .boot_services()
            .open_protocol_exclusive::<GraphicsOutput>(handle); // updated in uefi 0.27
        if let Ok(mut gop) = gop {
            let mode = gop.current_mode_info();
            println!(
                "FB: {}x{} @ {:?}",
                mode.resolution().0,
                mode.resolution().1,
                gop.frame_buffer().as_mut_ptr()
            );
        }
    }

    // TODO: load kernel ELF, parse and jump to entry
    println!("Halting — kernel handoff not yet implemented.");
    loop {
        #[cfg(target_arch = "x86_64")]
        unsafe { core::arch::asm!("hlt") };
        #[cfg(target_arch = "aarch64")]
        unsafe { core::arch::asm!("wfe") };
    }
}
