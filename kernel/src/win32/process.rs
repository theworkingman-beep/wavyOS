//! NT process abstraction for Windows binaries.

use super::objects::Handle;

/// A Windows process under Aperture OS.
pub struct Process {
    pub pid: u64,
    pub peb_base: u64,
    pub teb_base: u64,
    pub root_handle: Handle,
    /// Base address where the PE image is mapped.
    pub image_base: u64,
    /// Size of the mapped PE image.
    pub image_size: u64,
    /// Absolute entry point address inside the mapped image.
    pub entry_point: u64,
    /// Physical address of the top-level page table (CR3) for this process,
    /// or 0 if the architecture does not use per-process page tables yet.
    pub page_table_root: u64,
}

impl Process {
    pub fn new(pid: u64) -> Self {
        Self {
            pid,
            peb_base: 0,
            teb_base: 0,
            root_handle: Handle(0),
            image_base: 0,
            image_size: 0,
            entry_point: 0,
            page_table_root: 0,
        }
    }
}
