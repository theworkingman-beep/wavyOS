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
pub mod scheduler;
pub mod thread;
pub mod win32k;

/// Initialize the Windows subsystem (no-op until the first PE is loaded).
pub fn init() {
    objects::init();
    registry::init();
    nt::init();
    nt::init_syscall_table();
}

/// Load a small synthetic PE image from the VFS to verify the loader, VFS,
/// and NT process creation path.
///
/// Must be called after the physical frame allocator and early heap have been
/// initialized.
pub fn self_test() {
    // Ensure /bin/test.exe exists and contains the minimal PE fixture.
    let bin = crate::vfs::lookup("/bin").unwrap_or_else(|| {
        crate::vfs::create(crate::vfs::NodeId(0), "bin", crate::vfs::NodeKind::Directory)
            .expect("create /bin")
    });
    let test_exe = crate::vfs::create(bin, "test.exe", crate::vfs::NodeKind::File)
        .unwrap_or_else(|| crate::vfs::lookup("/bin/test.exe").expect("/bin/test.exe node"));
    let file = crate::vfs::open(test_exe, true).expect("open /bin/test.exe");
    let written = crate::vfs::write(file, loader::MINIMAL_PE64).expect("write fixture");
    assert_eq!(written, loader::MINIMAL_PE64.len());
    crate::vfs::close(file);

    let Some((handle, needs_translation)) = nt::create_user_process("/bin/test.exe") else {
        crate::logln!("win32: NtCreateUserProcess self-test FAILED.");
        return;
    };

    let header = objects::lookup(handle);
    let Some(header) = header else {
        crate::logln!("win32: created process handle {} not found.", handle.0);
        return;
    };

    let proc = unsafe { &*(header.data as *const process::Process) };
    crate::logln!(
        "win32: NtCreateUserProcess self-test OK. pid={} image_base={:#x} image_size={:#x} entry={:#x} translation={}",
        proc.pid,
        proc.image_base,
        proc.image_size,
        proc.entry_point,
        needs_translation
    );

    let slot = scheduler::create_thread(proc.pid, proc.entry_point, proc.page_table_root)
        .expect("create initial thread for loaded process");
    let thread = scheduler::thread(slot).expect("scheduled thread");
    crate::logln!(
        "win32: created thread tid={} entry={:#x} slot={} cr3={:#x} translation={}",
        thread.tid,
        thread.entry_point,
        slot,
        thread.process_page_table_root,
        needs_translation
    );

    if needs_translation {
        crate::logln!("win32: guest architecture differs from host; running interpreter.");
        unsafe {
            scheduler::enter_interpreter(slot);
        }
    } else {
        #[cfg(feature = "arch_x86_64")]
        unsafe {
            scheduler::enter_user_mode(slot);
        }
        #[cfg(not(feature = "arch_x86_64"))]
        crate::hlt();
    }
}
