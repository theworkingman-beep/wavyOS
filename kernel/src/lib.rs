#![no_std]
#![cfg_attr(all(feature = "arch_x86_64", test), no_main)]
#![cfg_attr(feature = "arch_x86_64", feature(abi_x86_interrupt))]

pub mod arch;
pub mod boot_info;
pub mod gui;
pub mod logger;
pub mod mm;
pub mod panic;
pub mod win32;

/// Kernel initialization entry common to all architectures.
pub fn init() {
    logger::init();
    arch::init();
    mm::init();
    gui::init();
}

/// Halt the CPU forever.
pub fn hlt() -> ! {
    arch::hlt()
}
