//! Cooperative round-robin scheduler with context switching and per-task page tables
//! Also provides process management (fork, exec, exit, wait, PID allocation)
use alloc::vec::Vec;
use spin::Mutex;

/// Size of a task stack in bytes
pub const STACK_SIZE: usize = 64 * 1024;

/// Saved CPU context for a task
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct Context {
    pub rsp: usize,
}

/// Task type: kernel or user-space
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskType {
    Kernel,
    User,
}

/// Task state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    Ready,
    Running,
    Dead,
}

/// Task structure
#[derive(Clone)]
pub struct Task {
    pub id: usize,
    pub stack: *mut u8,
    pub context: Context,
    pub entry: usize,
    pub state: TaskState,
    pub task_type: TaskType,
    #[cfg(target_arch = "x86_64")]
    pub page_tables: Option<crate::arch::x86_64::TaskPageTables>,
    #[cfg(target_arch = "aarch64")]
    pub page_tables: Option<crate::arch::aarch64::TaskPageTables>,
}

// Safety: Task stack pointers are used only by the scheduler which enforces
// exclusive access. All stack data is zero-initialized.
unsafe impl Send for Task {}

impl Task {
    pub fn new(id: usize, entry: usize) -> Self {
        let stack_layout = alloc::alloc::Layout::from_size_align(STACK_SIZE, 16).unwrap();
        let stack_ptr = unsafe { alloc::alloc::alloc(stack_layout) };
        if stack_ptr.is_null() {
            panic!("Failed to allocate task stack");
        }
        unsafe { core::ptr::write_bytes(stack_ptr, 0, STACK_SIZE); }
        Self {
            id,
            stack: stack_ptr,
            context: Context { rsp: 0 },
            entry,
            state: TaskState::Ready,
            task_type: TaskType::Kernel,
            page_tables: None,
        }
    }

    /// Create a user-space task
    pub fn new_user(id: usize, entry: usize) -> Self {
        let mut task = Self::new(id, entry);
        task.task_type = TaskType::User;

        // Set up per-task page tables
        #[cfg(target_arch = "x86_64")]
        {
            let (k_phys, k_virt) = crate::arch::x86_64::kernel_page_tables();
            task.page_tables = Some(crate::arch::x86_64::create_task_page_tables(k_phys, k_virt));
        }
        #[cfg(target_arch = "aarch64")]
        {
            let (k_phys, k_virt) = crate::arch::aarch64::kernel_page_tables();
            task.page_tables = Some(crate::arch::aarch64::create_task_page_tables(k_phys, k_virt));
        }

        task
    }

    /// Map a user-space page in this task's page tables
    pub fn map_user_page(&mut self, vaddr: u64, writable: bool) -> Option<usize> {
        let pt = self.page_tables.as_mut()?;
        #[cfg(target_arch = "x86_64")]
        {
            crate::arch::x86_64::map_user_page(pt, vaddr, writable)
        }
        #[cfg(target_arch = "aarch64")]
        {
            crate::arch::aarch64::map_user_page(pt, vaddr, writable)
        }
    }

    pub fn stack_top(&self) -> usize {
        self.stack as usize + STACK_SIZE
    }

    pub fn init_context(&mut self) {
        let mut sp = self.stack_top();
        unsafe {
            // aarch64: push entry FIRST (highest address), then 11 dummies (x19-x29)
            #[cfg(target_arch = "aarch64")]
            {
                sp -= core::mem::size_of::<usize>();
                *(sp as *mut usize) = self.entry;
                for _ in 0..11 {
                    sp -= core::mem::size_of::<usize>();
                    *(sp as *mut usize) = 0;
                }
            }

            // x86_64: push entry as return address, then 6 dummy callee-saved regs
            #[cfg(target_arch = "x86_64")]
            {
                sp -= core::mem::size_of::<usize>();
                *(sp as *mut usize) = self.entry;
                for _ in 0..6 {
                    sp -= core::mem::size_of::<usize>();
                    *(sp as *mut usize) = 0;
                }
            }
        }
        self.context.rsp = sp;
    }

    /// Get the page table root for this task (for context switching)
    #[cfg(target_arch = "x86_64")]
    pub fn page_table_root(&self) -> usize {
        match self.task_type {
            TaskType::Kernel => {
                let (phys, _) = crate::arch::x86_64::kernel_page_tables();
                phys
            }
            TaskType::User => {
                self.page_tables.map(|pt| pt.pml4_phys).unwrap_or_else(|| {
                    let (phys, _) = crate::arch::x86_64::kernel_page_tables();
                    phys
                })
            }
        }
    }

    #[cfg(target_arch = "aarch64")]
    pub fn page_table_root(&self) -> usize {
        match self.task_type {
            TaskType::Kernel => {
                let (phys, _) = crate::arch::aarch64::kernel_page_tables();
                phys
            }
            TaskType::User => {
                self.page_tables.map(|pt| pt.ttbr0_phys).unwrap_or_else(|| {
                    let (phys, _) = crate::arch::aarch64::kernel_page_tables();
                    phys
                })
            }
        }
    }
}

/// Process exit status
#[derive(Debug, Clone, Copy)]
pub struct ExitStatus {
    pub code: i32,
}

/// Process table entry
#[derive(Clone)]
pub struct Process {
    pub pid: usize,
    pub task: Task,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    pub exit_status: Option<ExitStatus>,
    pub waiters: Vec<usize>,
}

unsafe impl Send for Process {}

/// Process table (indexed by PID)
static PROCESSES: Mutex<Vec<Process>> = Mutex::new(Vec::new());
/// Current running PID
static CURRENT_TASK: Mutex<Option<usize>> = Mutex::new(None);
/// PID counter (next PID to allocate)
static PID_COUNTER: Mutex<usize> = Mutex::new(1); // PID 0 = kernel

pub fn init() {
    log::info!("scheduler: initialized");
}

/// Allocate a new PID
fn alloc_pid() -> usize {
    let mut counter = PID_COUNTER.lock();
    let pid = *counter;
    *counter += 1;
    pid
}

/// Spawn a kernel task
pub fn spawn(entry: extern "C" fn() -> !) -> usize {
    let pid = alloc_pid();
    let mut task = Task::new(pid, entry as usize);
    task.init_context();
    let proc = Process {
        pid,
        task,
        parent: None,
        children: Vec::new(),
        exit_status: None,
        waiters: Vec::new(),
    };
    PROCESSES.lock().push(proc);
    pid
}

/// Spawn a user-space task with its own page tables
pub fn spawn_user(entry: usize) -> usize {
    let pid = alloc_pid();
    let mut task = Task::new_user(pid, entry);
    task.init_context();
    let proc = Process {
        pid,
        task,
        parent: *CURRENT_TASK.lock(),
        children: Vec::new(),
        exit_status: None,
        waiters: Vec::new(),
    };
    PROCESSES.lock().push(proc);
    pid
}

/// Get current PID
pub fn current_task_id() -> usize {
    *CURRENT_TASK.lock().as_ref().unwrap_or(&0)
}

/// Yield CPU to the next task
pub fn yield_cpu() {
    unsafe {
        do_switch_task();
    }
}

/// Fork current process (creates a child with copy of page tables)
pub fn fork() -> usize {
    let cur_pid = current_task_id();
    let procs = PROCESSES.lock();
    let cur_proc = procs.iter().find(|p| p.pid == cur_pid).cloned();
    drop(procs);

    if let Some(mut parent_proc) = cur_proc {
        let child_pid = alloc_pid();

        // Create new task with copied page tables
        let child_task = Task::new_user(child_pid, parent_proc.task.entry);
        // Copy parent's page tables to child
        // (In a real fork, we'd COW the pages)

        let child_proc = Process {
            pid: child_pid,
            task: child_task,
            parent: Some(cur_pid),
            children: Vec::new(),
            exit_status: None,
            waiters: Vec::new(),
        };

        let mut procs = PROCESSES.lock();
        procs.push(child_proc);
        if let Some(parent) = procs.iter_mut().find(|p| p.pid == cur_pid) {
            parent.children.push(child_pid);
        }

        // Return child PID to parent, 0 to child (for now, just return child PID)
        child_pid
    } else {
        0
    }
}

/// Exit current process with given status code
pub fn exit(code: i32) -> ! {
    let pid = current_task_id();
    let (waiters, children) = {
        let mut procs = PROCESSES.lock();
        let mut waiters = Vec::new();
        let mut children = Vec::new();

        if let Some(idx) = procs.iter().position(|p| p.pid == pid) {
            procs[idx].task.state = TaskState::Dead;
            procs[idx].exit_status = Some(ExitStatus { code });

            // Collect waiters
            core::mem::swap(&mut waiters, &mut procs[idx].waiters);
            // Collect children
            core::mem::swap(&mut children, &mut procs[idx].children);
        }
        (waiters, children)
    };

    // Notify waiters (outside the lock to avoid borowing issues)
    for wpid in waiters {
        let mut procs = PROCESSES.lock();
        if let Some(waiter) = procs.iter_mut().find(|p| p.pid == wpid) {
            if waiter.task.state == TaskState::Dead {
                waiter.task.state = TaskState::Ready;
            }
        }
    }

    // Reparent children
    for child_pid in children {
        let mut procs = PROCESSES.lock();
        if let Some(child) = procs.iter_mut().find(|p| p.pid == child_pid) {
            child.parent = None;
        }
    }

    // Switch to next task (we're dying)
    unsafe {
        let _ = core::mem::take(&mut *CURRENT_TASK.lock());
    }
    yield_cpu();

    // Should not reach here
    #[cfg(target_arch = "x86_64")]
    loop { unsafe { core::arch::asm!("hlt") } }
    #[cfg(target_arch = "aarch64")]
    loop { unsafe { core::arch::asm!("wfe") } }
}

/// Wait for a child process to exit
pub fn wait(pid: isize) -> (usize, i32) {
    let cur_pid = current_task_id();
    loop {
        // Check if child exists and has exited
        let (found_pid, exit_info) = {
            let procs = PROCESSES.lock();
            let mut found = None;
            for proc in procs.iter() {
                if proc.parent == Some(cur_pid) {
                    if pid < 0 || proc.pid == pid as usize {
                        if let Some(status) = proc.exit_status {
                            found = Some((proc.pid, status.code));
                        } else {
                            found = Some((proc.pid, -1)); // Not exited yet
                        }
                        break;
                    }
                }
            }
            match found {
                Some((pid, -1)) => (Some(pid), None), // Not exited
                Some((pid, code)) => (Some(pid), Some((pid, code))),
                None => return (0, 0), // No child
            }
        };

        if let Some((child_pid, code)) = exit_info {
            // Child exited — remove from table
            let mut procs = PROCESSES.lock();
            if let Some(idx) = procs.iter().position(|p| p.pid == child_pid) {
                procs.remove(idx);
            }
            return (child_pid, code);
        }

        // Child hasn't exited yet — register as waiter and sleep
        {
            let mut procs = PROCESSES.lock();
            if let Some(child) = procs.iter_mut().find(|p| p.pid == found_pid.unwrap()) {
                child.waiters.push(cur_pid);
            }
        }
        // Mark self as dead (sleeping) and yield
        {
            let mut procs = PROCESSES.lock();
            if let Some(self_proc) = procs.iter_mut().find(|p| p.pid == cur_pid) {
                self_proc.task.state = TaskState::Dead;
            }
        }
        yield_cpu();
    }
}

/// Proper context switch: save callee-saved regs, switch stack, restore
#[cfg(target_arch = "x86_64")]
unsafe fn do_switch_task() {
    let next_rsp: usize;
    let old_rsp_ptr: *mut usize;
    let next_pt_root: usize;

    // Determine next task
    {
        let mut procs = PROCESSES.lock();
        let cur_pid = *CURRENT_TASK.lock();
        let len = procs.len();
        if len == 0 { return; }

        let mut cur_idx = None;
        for (i, p) in procs.iter().enumerate() {
            if Some(p.pid) == cur_pid {
                cur_idx = Some(i);
                break;
            }
        }

        let mut next_idx = None;
        for i in 0..len {
            let idx = (cur_idx.unwrap_or(usize::MAX) + 1 + i) % len;
            if procs[idx].task.state != TaskState::Dead {
                next_idx = Some(idx);
                break;
            }
        }

        let next_idx = match next_idx {
            Some(n) => n,
            None => return,
        };

        if let Some(ci) = cur_idx {
            if procs[ci].task.state == TaskState::Running {
                procs[ci].task.state = TaskState::Ready;
            }
            old_rsp_ptr = &mut procs[ci].task.context.rsp as *mut usize;
        } else {
            old_rsp_ptr = core::ptr::null_mut();
        }

        next_rsp = procs[next_idx].task.context.rsp;
        next_pt_root = procs[next_idx].task.page_table_root();
        procs[next_idx].task.state = TaskState::Running;
        *CURRENT_TASK.lock() = Some(procs[next_idx].pid);
    }

    crate::arch::x86_64::load_page_tables(next_pt_root);
    switch_context(old_rsp_ptr, next_rsp);
}

#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(
    ".global switch_context",
    "switch_context:",
    "test rdi, rdi",
    "jz 1f",
    "push r15",
    "push r14",
    "push r13",
    "push r12",
    "push rbp",
    "push rbx",
    "mov [rdi], rsp",
    "1:",
    "mov rsp, rsi",
    "pop rbx",
    "pop rbp",
    "pop r12",
    "pop r13",
    "pop r14",
    "pop r15",
    "ret",
);

extern "C" {
    fn switch_context(old_sp_ptr: *mut usize, new_sp: usize);
}

/// Context switch for aarch64
#[cfg(target_arch = "aarch64")]
unsafe fn do_switch_task() {
    let next_sp: usize;
    let old_sp_ptr: *mut usize;
    let next_pt_root: usize;

    {
        let mut procs = PROCESSES.lock();
        let cur_pid = *CURRENT_TASK.lock();
        let len = procs.len();
        if len == 0 { return; }

        let mut cur_idx = None;
        for (i, p) in procs.iter().enumerate() {
            if Some(p.pid) == cur_pid {
                cur_idx = Some(i);
                break;
            }
        }

        let mut next_idx = None;
        for i in 0..len {
            let idx = (cur_idx.unwrap_or(usize::MAX) + 1 + i) % len;
            if procs[idx].task.state != TaskState::Dead {
                next_idx = Some(idx);
                break;
            }
        }

        let next_idx = match next_idx {
            Some(n) => n,
            None => return,
        };

        if let Some(ci) = cur_idx {
            if procs[ci].task.state == TaskState::Running {
                procs[ci].task.state = TaskState::Ready;
            }
            old_sp_ptr = &mut procs[ci].task.context.rsp as *mut usize;
        } else {
            old_sp_ptr = core::ptr::null_mut();
        }

        next_sp = procs[next_idx].task.context.rsp;
        next_pt_root = procs[next_idx].task.page_table_root();
        procs[next_idx].task.state = TaskState::Running;
        *CURRENT_TASK.lock() = Some(procs[next_idx].pid);
    }

    crate::arch::aarch64::load_page_tables(next_pt_root);
    switch_context(old_sp_ptr, next_sp);
}

#[cfg(target_arch = "aarch64")]
core::arch::global_asm!(
    ".global switch_context",
    "switch_context:",
    "cbz x0, 1f",
    "stp x29, x30, [sp, #-16]!",
    "stp x27, x28, [sp, #-16]!",
    "stp x25, x26, [sp, #-16]!",
    "stp x23, x24, [sp, #-16]!",
    "stp x21, x22, [sp, #-16]!",
    "stp x19, x20, [sp, #-16]!",
    "mov x2, sp",
    "str x2, [x0]",
    "1:",
    "mov sp, x1",
    "ldp x19, x20, [sp], #16",
    "ldp x21, x22, [sp], #16",
    "ldp x23, x24, [sp], #16",
    "ldp x25, x26, [sp], #16",
    "ldp x27, x28, [sp], #16",
    "ldp x29, x30, [sp], #16",
    "ret",
);

/// Run the scheduler — starts the first task and never returns
pub fn run_scheduler() -> ! {
    let procs = PROCESSES.lock();
    let len = procs.len();
    if len == 0 {
        #[cfg(target_arch = "x86_64")]
        crate::arch::x86_64::halt_loop();
        #[cfg(target_arch = "aarch64")]
        crate::arch::aarch64::halt_loop();
    }

    let first_pid = procs[0].pid;
    let first_rsp = procs[0].task.context.rsp;
    let first_pt_root = procs[0].task.page_table_root();
    drop(procs);

    unsafe {
        #[cfg(target_arch = "x86_64")]
        crate::arch::x86_64::load_page_tables(first_pt_root);
        #[cfg(target_arch = "aarch64")]
        crate::arch::aarch64::load_page_tables(first_pt_root);

        *CURRENT_TASK.lock() = Some(first_pid);
        let mut procs = PROCESSES.lock();
        if let Some(proc) = procs.iter_mut().find(|p| p.pid == first_pid) {
            proc.task.state = TaskState::Running;
        }
        drop(procs);

        switch_context(core::ptr::null_mut(), first_rsp);
    }

    loop {
        #[cfg(target_arch = "x86_64")]
        crate::arch::x86_64::halt_loop();
        #[cfg(target_arch = "aarch64")]
        crate::arch::aarch64::halt_loop();
    }
}
