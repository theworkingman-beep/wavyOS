//! x86_64 hardware abstraction layer.

use x86_64::instructions::port::Port;

pub mod context_switch;
pub mod interrupts;
pub mod syscall;

/// Initialize x86_64-specific hardware.
pub fn init() {
    interrupts::init();
    unsafe {
        syscall::init();
    }
}

/// Output a single byte to the debug serial port (0x3F8 COM1).
pub fn debug_putchar(byte: u8) {
    unsafe {
        let mut port: Port<u8> = Port::new(0x3F8);
        port.write(byte);
    }
}

/// Halt the CPU until the next interrupt, then return.
pub fn halt_once() {
    x86_64::instructions::hlt();
}

/// Halt the CPU forever.
pub fn hlt() -> ! {
    loop {
        x86_64::instructions::hlt();
    }
}
