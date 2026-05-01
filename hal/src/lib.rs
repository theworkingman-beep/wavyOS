//! Hardware Abstraction Layer for Vibe Coded OS
#![no_std]

/// Platform-specific code lives in sub-modules here.
/// For now, minimal stubs so the workspace compiles.

#[cfg(target_arch = "x86_64")]
pub mod x86_64;

#[cfg(target_arch = "aarch64")]
pub mod aarch64;

pub fn platform_init() {
    #[cfg(target_arch = "x86_64")]
    x86_64::init();
    #[cfg(target_arch = "aarch64")]
    aarch64::init();
}
