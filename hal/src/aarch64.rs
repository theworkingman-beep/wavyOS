// aarch64 HAL stubs

pub fn init() {
    // GIC, timer, UART are handled in kernel/src/arch/aarch64.rs
}

/// Read the ARM Generic Timer CNTPCT (virtual counter).
/// On bare metal without EL1 access to CNTVCT, use CNTPCT via MRS.
pub fn monotonic_ticks() -> u64 {
    unsafe {
        let lo: u64;
        let hi: u64;
        core::arch::asm!(
            "mrs {0}, cntpct_el0",
            out(reg) lo,
        );
        lo
    }
}

/// Halt until the next interrupt.
pub fn halt() {
    unsafe { core::arch::asm!("wfe", options(nomem, nostack)); }
}

/// Signal end-of-interrupt to the GIC (placeholder).
pub fn eoi() {
}
