//! AArch64 guest translator skeleton.
//!
//! On x86_64 hosts, AArch64 Windows apps are translated through an instruction
//! interpreter + baseline JIT. This module is a placeholder for the real
//! decoder.

use super::{GuestArch, TranslationUnit};

/// Decode an AArch64 instruction and produce a placeholder translation unit.
pub fn translate_block(guest_pc: u64, code: &[u8]) -> Option<TranslationUnit> {
    if code.len() < 4 {
        return None;
    }
    Some(TranslationUnit {
        guest_entry: guest_pc,
        host_entry: code.as_ptr() as *const (),
        guest_arch: GuestArch::Aarch64,
    })
}
