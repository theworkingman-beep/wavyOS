//! AArch64 hardware abstraction layer (stub for cross-architecture build).

/// Initialize AArch64-specific hardware.
pub fn init() {
    // TODO: configure GIC, timers, MMU.
}

/// Output a single byte to the debug UART.
pub fn debug_putchar(_byte: u8) {
    // TODO: PL011 or semihosting output.
}

/// Halt the CPU.
pub fn hlt() -> ! {
    loop {
        unsafe { core::arch::asm!("wfe", options(nomem, nostack)) };
    }
}
