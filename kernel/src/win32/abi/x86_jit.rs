//! x86/x86_64 guest translator skeleton.
//!
//! Implements a minimal decoder for common instructions and emits host code.
//! For Aperture OS bring-up this is a stub; the real JIT will be built around
//! the `iced-x86` decoder once the host allocator is mature.

use super::{GuestArch, TranslationUnit};

/// Decode the first few bytes of an x86/x86_64 instruction and produce a
/// placeholder translation unit.
pub fn translate_block(_guest_pc: u64, code: &[u8], _is_64bit: bool) -> Option<TranslationUnit> {
    if code.is_empty() {
        return None;
    }
    // Placeholder: return a unit pointing to the first byte so that an
    // interpreter can resume from the same PC.
    Some(TranslationUnit {
        guest_entry: _guest_pc,
        host_entry: code.as_ptr() as *const (),
        guest_arch: if _is_64bit { GuestArch::X86_64 } else { GuestArch::X86 },
    })
}
