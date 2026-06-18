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

/// Load a small synthetic PE image to verify the loader and memory subsystem.
///
/// Must be called after the physical frame allocator and early heap have been
/// initialized.
pub fn self_test() {
    let Some((handle, needs_translation)) = loader::load_pe(loader::MINIMAL_PE64, 1) else {
        crate::logln!("win32: PE loader self-test FAILED.");
        return;
    };

    let header = objects::lookup(handle);
    let Some(header) = header else {
        crate::logln!("win32: loaded process handle {} not found.", handle.0);
        return;
    };

    let process = unsafe { &*(header.data as *const process::Process) };
    crate::logln!(
        "win32: PE loader self-test OK. pid={} image_base={:#x} image_size={:#x} entry={:#x} translation={}",
        process.pid,
        process.image_base,
        process.image_size,
        process.entry_point,
        needs_translation
    );
}
