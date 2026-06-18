//! Architecture translation layer for Windows binaries.
//!
//! Aperture OS supports running x86, x86_64, and ARM64 PE images on any host
//! CPU. When the host architecture differs from the image architecture, the
//! kernel uses one of the following strategies:
//!
//!   - **Interpreter**: high-correctness instruction interpreter for cold code.
//!   - **Baseline JIT**: simple block-at-a-time binary translator for hot code.
//!   - **Host-native thunk**: for syscalls and GUI callbacks, we jump directly
//!     into host-native code where possible.
//!
//! This module is intentionally architecture-agnostic. Submodules implement
//! the per-architecture translators.

pub mod aarch64_interpreter;
pub mod aarch64_jit;
pub mod interpreter;
pub mod syscall;
pub mod x86_jit;

/// Target architecture being emulated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuestArch {
    X86,
    X86_64,
    Aarch64,
}

/// A translation unit produced by a JIT or interpreter.
pub struct TranslationUnit {
    pub guest_entry: u64,
    pub host_entry: *const (),
    pub guest_arch: GuestArch,
}

impl TranslationUnit {
    /// # Safety
    /// The host entry must point to valid code and the caller must set up the
    /// correct guest register/memory state before invoking it.
    pub unsafe fn call(&self) {
        let f: extern "C" fn() = core::mem::transmute(self.host_entry);
        f();
    }
}

/// Translate a single guest instruction stream for `guest_arch`.
pub fn translate(_guest_arch: GuestArch, _guest_pc: u64, _code: &[u8]) -> Option<TranslationUnit> {
    // TODO: route to x86_jit or aarch64_jit based on guest/host combo.
    None
}
