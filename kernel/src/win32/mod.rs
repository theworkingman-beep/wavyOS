//! Windows application compatibility subsystem.
//!
//! The design goal is first-class Windows binary compatibility, not a wrapper
//! around Wine. Aperture OS implements the core NT kernel ABI natively in Rust
//! and provides a clean Win32 subsystem on top of it.
//!
//! Architecture:
//!   - PE loader (`loader`)
//!   - NT system call dispatch table (`nt`)
//!   - Object manager / handle table (`objects`)
//!   - Process/thread model (`process`, `thread`)
//!   - Registry and filesystem shims (`registry`, `fs`)
//!   - User-mode Win32 API server (`win32k`)
//!   - x86-on-ARM and ARM-on-x86 dynamic binary translation (`abi::translate`)

pub mod abi;
pub mod fs;
pub mod loader;
pub mod nt;
pub mod objects;
pub mod process;
pub mod registry;
pub mod thread;
pub mod win32k;

/// Initialize the Windows subsystem (no-op until the first PE is loaded).
pub fn init() {
    objects::init();
    registry::init();
    nt::init();
}
