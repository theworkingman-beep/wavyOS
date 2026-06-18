//! AArch64 interrupt stubs.
//!
//! A real implementation will set up the GIC, timer, and install a
//! vectored exception table. These stubs exist so portable kernel code can
//! call `read_char` and `mouse_position` on both architectures.

/// No keyboard on AArch64 yet; always returns None.
pub fn read_char() -> Option<char> {
    None
}

/// No mouse on AArch64 yet; always returns a default position.
pub fn mouse_position() -> (i32, i32) {
    (0, 0)
}

/// No mouse buttons on AArch64 yet.
pub fn mouse_buttons() -> u8 {
    0
}

/// No scancode on AArch64 yet.
pub fn read_scancode() -> Option<u8> {
    None
}

/// Halt until the next interrupt.
pub fn halt_once() {
    unsafe {
        core::arch::asm!("wfi", options(nomem, nostack, preserves_flags));
    }
}
