//! NT process abstraction for Windows binaries.

use super::objects::Handle;

/// A Windows process under Aperture OS.
pub struct Process {
    pub pid: u64,
    pub peb_base: u64,
    pub teb_base: u64,
    pub root_handle: Handle,
}

impl Process {
    pub fn new(pid: u64) -> Self {
        Self {
            pid,
            peb_base: 0,
            teb_base: 0,
            root_handle: Handle(0),
        }
    }
}
