//! AArch64 GDT/TSS placeholder.
//!
//! AArch64 does not use segmentation; this module only provides the
//! `set_rsp0` symbol used by shared scheduler code so x86_64-specific
//! helper calls compile for the AArch64 build.

/// No-op on AArch64: EL1 stack selection is handled by SP_EL1.
pub unsafe fn set_rsp0(_rsp: u64) {}
