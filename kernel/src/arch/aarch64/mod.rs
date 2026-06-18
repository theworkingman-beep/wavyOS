//! AArch64 hardware abstraction layer (stub for cross-architecture build).

pub mod context_switch;
pub mod gdt;
pub mod interrupts;

/// Initialize AArch64-specific hardware.
pub fn init() {
    // TODO: configure GIC, timers, MMU.
}

/// Output a single byte to the debug UART.
pub fn debug_putchar(_byte: u8) {
    // TODO: PL011 or semihosting output.
}

/// Halt the CPU until the next interrupt, then return.
pub fn halt_once() {
    unsafe { core::arch::asm!("wfe", options(nomem, nostack)) };
}

/// Halt the CPU forever.
pub fn hlt() -> ! {
    loop {
        halt_once();
    }
}
