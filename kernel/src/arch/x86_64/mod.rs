//! x86_64 hardware abstraction layer.

use x86_64::instructions::port::Port;

/// Initialize x86_64-specific hardware.
pub fn init() {
    // TODO: configure PIC/APIC, enable MMU, load GDT/IDT.
}

/// Output a single byte to the debug serial port (0x3F8 COM1).
pub fn debug_putchar(byte: u8) {
    unsafe {
        let mut port: Port<u8> = Port::new(0x3F8);
        port.write(byte);
    }
}

/// Halt the CPU.
pub fn hlt() -> ! {
    loop {
        x86_64::instructions::hlt();
    }
}
