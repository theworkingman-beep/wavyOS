#![no_std]
#![cfg_attr(test, no_main)]

pub mod arch;
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
