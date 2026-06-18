//! NT thread abstraction for Windows binaries.

/// A Windows thread inside a process.
pub struct Thread {
    pub tid: u64,
    pub pid: u64,
    pub entry_point: u64,
    pub stack_base: u64,
    pub stack_limit: u64,
}

impl Thread {
    pub fn new(tid: u64, pid: u64, entry_point: u64) -> Self {
        Self {
            tid,
            pid,
            entry_point,
            stack_base: 0,
            stack_limit: 0,
        }
    }
}
