//! NT thread abstraction for Windows binaries.

/// Lifecycle state of a thread.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThreadState {
    Ready,
    Running,
    Blocked,
    Exited,
}

/// Guest x86_64 register indices for the interpreter register file.
#[repr(usize)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Register {
    Rax = 0,
    Rcx = 1,
    Rdx = 2,
    Rbx = 3,
    Rsp = 4,
    Rbp = 5,
    Rsi = 6,
    Rdi = 7,
    R8 = 8,
    R9 = 9,
    R10 = 10,
    R11 = 11,
    R12 = 12,
    R13 = 13,
    R14 = 14,
    R15 = 15,
}

/// A Windows thread inside a process.
#[derive(Clone, Copy)]
pub struct Thread {
    pub tid: u64,
    pub pid: u64,
    pub entry_point: u64,
    /// Kernel stack base and limit (used for scheduler context).
    pub stack_base: u64,
    pub stack_limit: u64,
    /// Kernel RSP used by the cooperative context switch.
    pub rsp: u64,
    /// User-mode stack pointer. For native threads this is the initial RSP
    /// passed to sysret/iret; for interpreted threads it is the physical top
    /// of the guest stack.
    pub user_rsp: u64,
    /// User-mode instruction pointer. Cached from entry_point for clarity.
    pub user_rip: u64,
    /// Physical address of the owning process's top-level page table (CR3).
    pub process_page_table_root: u64,
    pub state: ThreadState,
    /// Guest x86_64 general-purpose registers, used by the interpreter.
    pub regs: [u64; 16],
}

impl Thread {
    pub fn read_reg(&self, reg: Register) -> u64 {
        self.regs[reg as usize]
    }

    pub fn write_reg(&mut self, reg: Register, value: u64) {
        self.regs[reg as usize] = value;
    }
}

impl Thread {
    pub fn new(tid: u64, pid: u64, entry_point: u64) -> Self {
        Self {
            tid,
            pid,
            entry_point,
            stack_base: 0,
            stack_limit: 0,
            rsp: 0,
            user_rsp: 0,
            user_rip: entry_point,
            process_page_table_root: 0,
            state: ThreadState::Ready,
            regs: [0; 16],
        }
    }
}

unsafe impl Send for Thread {}
