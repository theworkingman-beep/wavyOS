//! Hardware abstraction layer.
//!
//! Aperture OS is designed to run natively on x86_64 and AArch64. Each
//! architecture implements this trait-like module so the rest of the kernel can
//! stay portable.

#[cfg(feature = "arch_x86_64")]
pub mod x86_64;
#[cfg(feature = "arch_x86_64")]
pub use x86_64::*;

#[cfg(feature = "arch_aarch64")]
pub mod aarch64;
#[cfg(feature = "arch_aarch64")]
pub use aarch64::*;
