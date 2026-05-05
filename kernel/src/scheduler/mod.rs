//! Cooperative round-robin scheduler with context switching and per-task page tables
use alloc::collections::vec_deque::VecDeque;
use spin::Mutex;

/// Size of a task stack in bytes
pub const STACK_SIZE: usize = 64 * 1024;

/// Saved CPU context for a task
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct Context {
    pub rsp: usize,
}

// Safety: Task stack pointers are used only by the scheduler which enforces
// exclusive access. All stack data is zero-initialized.
unsafe impl Send for Task {}

static TASKS: Mutex<VecDeque<Task>> = Mutex::new(VecDeque::new());
static CURRENT_TASK: Mutex<Option<usize>> = Mutex::new(None);
static TASK_COUNTER: Mutex<usize> = Mutex::new(0);

/// Task type: kernel or user-space
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskType {
    Kernel,
    User,
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    Ready,
    Running,
    Dead,
}

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
            // Stack grows down, so entry ends up at lowest address after all pushes
            // switch_context pops x19-x30 from low to high, so entry loads into x30
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

pub fn init() {
    log::info!("scheduler: initialized");
}

pub fn spawn(entry: extern "C" fn() -> !) -> usize {
    let mut counter = TASK_COUNTER.lock();
    let id = *counter;
    *counter += 1;
    drop(counter);

    let mut task = Task::new(id, entry as usize);
    task.init_context();
    TASKS.lock().push_back(task);
    id
}

/// Spawn a user-space task with its own page tables
pub fn spawn_user(entry: usize) -> usize {
    let mut counter = TASK_COUNTER.lock();
    let id = *counter;
    *counter += 1;
    drop(counter);

    let mut task = Task::new_user(id, entry);
    task.init_context();
    TASKS.lock().push_back(task);
    id
}

pub fn current_task_id() -> usize {
    *CURRENT_TASK.lock().as_ref().unwrap_or(&0)
}

/// Yield CPU to the next task
pub fn yield_cpu() {
    unsafe {
        do_switch_task();
    }
}

/// Proper context switch: save callee-saved regs, switch stack, restore
#[cfg(target_arch = "x86_64")]
unsafe fn do_switch_task() {
    let next_rsp: usize;
    let old_rsp_ptr: *mut usize;
    let next_pt_root: usize;

    // Determine next task and get its stack pointer
    {
        let mut tasks = TASKS.lock();
        let len = tasks.len();
        if len == 0 { return; }

        let cur_id = *CURRENT_TASK.lock();
        let mut cur_idx = None;
        for (i, t) in tasks.iter().enumerate() {
            if t.id == cur_id.unwrap_or(0) {
                cur_idx = Some(i);
                break;
            }
        }

        let mut next_idx = None;
        for i in 0..len {
            let idx = (cur_idx.unwrap_or(usize::MAX) + 1 + i) % len;
            if tasks[idx].state != TaskState::Dead {
                next_idx = Some(idx);
                break;
            }
        }

        let next_idx = match next_idx {
            Some(n) => n,
            None => return,
        };

        // Mark current as ready, next as running
        if let Some(ci) = cur_idx {
            if tasks[ci].state == TaskState::Running {
                tasks[ci].state = TaskState::Ready;
            }
            old_rsp_ptr = &mut tasks[ci].context.rsp as *mut usize;
        } else {
            // No current task (first yield from kernel_main)
            old_rsp_ptr = core::ptr::null_mut();
        }

        next_rsp = tasks[next_idx].context.rsp;
        next_pt_root = tasks[next_idx].page_table_root();
        tasks[next_idx].state = TaskState::Running;
        *CURRENT_TASK.lock() = Some(tasks[next_idx].id);
    }

    // Switch page tables
    crate::arch::x86_64::load_page_tables(next_pt_root);

    // Do the actual context switch in assembly
    switch_context(old_rsp_ptr, next_rsp);
}

/// Assembly context switch - saves callee-saved regs to old task, restores from new task
#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(
    ".global switch_context",
    "switch_context:",
    // rdi = old_rsp_ptr, rsi = new_rsp
    "test rdi, rdi",
    "jz 1f", // if old_rsp_ptr is null (first task), skip saving
    // Push callee-saved registers onto current stack
    "push r15",
    "push r14",
    "push r13",
    "push r12",
    "push rbp",
    "push rbx",
    // Get current RSP (after pushes) and save to old task's context
    "mov [rdi], rsp",
    "1:",
    // Switch to new stack
    "mov rsp, rsi",
    // Pop callee-saved registers
    "pop rbx",
    "pop rbp",
    "pop r12",
    "pop r13",
    "pop r14",
    "pop r15",
    // Return to the task
    "ret",
);

extern "C" {
    fn switch_context(old_sp_ptr: *mut usize, new_sp: usize);
}

/// Yield CPU to the next task (aarch64 version — same logic as x86_64)
#[cfg(target_arch = "aarch64")]
unsafe fn do_switch_task() {
    let next_sp: usize;
    let old_sp_ptr: *mut usize;
    let next_pt_root: usize;

    {
        let mut tasks = TASKS.lock();
        let len = tasks.len();
        if len == 0 { return; }

        let cur_id = *CURRENT_TASK.lock();
        let mut cur_idx = None;
        for (i, t) in tasks.iter().enumerate() {
            if t.id == cur_id.unwrap_or(0) {
                cur_idx = Some(i);
                break;
            }
        }

        let mut next_idx = None;
        for i in 0..len {
            let idx = (cur_idx.unwrap_or(usize::MAX) + 1 + i) % len;
            if tasks[idx].state != TaskState::Dead {
                next_idx = Some(idx);
                break;
            }
        }

        let next_idx = match next_idx {
            Some(n) => n,
            None => return,
        };

        if let Some(ci) = cur_idx {
            if tasks[ci].state == TaskState::Running {
                tasks[ci].state = TaskState::Ready;
            }
            old_sp_ptr = &mut tasks[ci].context.rsp as *mut usize;
        } else {
            old_sp_ptr = core::ptr::null_mut();
        }

        next_sp = tasks[next_idx].context.rsp;
        next_pt_root = tasks[next_idx].page_table_root();
        tasks[next_idx].state = TaskState::Running;
        *CURRENT_TASK.lock() = Some(tasks[next_idx].id);
    }

    // Switch page tables
    crate::arch::aarch64::load_page_tables(next_pt_root);

    switch_context(old_sp_ptr, next_sp);
}

/// Assembly context switch for aarch64 — saves x19-x30 (12 callee-saved regs)
#[cfg(target_arch = "aarch64")]
core::arch::global_asm!(
    ".global switch_context",
    "switch_context:",
    // x0 = old_sp_ptr, x1 = new_sp
    "cbz x0, 1f",
    // Push callee-saved registers x19-x30 (12 registers = 96 bytes)
    "stp x29, x30, [sp, #-16]!",
    "stp x27, x28, [sp, #-16]!",
    "stp x25, x26, [sp, #-16]!",
    "stp x23, x24, [sp, #-16]!",
    "stp x21, x22, [sp, #-16]!",
    "stp x19, x20, [sp, #-16]!",
    // Save SP after pushes
    "mov x2, sp",
    "str x2, [x0]",
    "1:",
    // Switch to new stack
    "mov sp, x1",
    // Pop callee-saved registers
    "ldp x19, x20, [sp], #16",
    "ldp x21, x22, [sp], #16",
    "ldp x23, x24, [sp], #16",
    "ldp x25, x26, [sp], #16",
    "ldp x27, x28, [sp], #16",
    "ldp x29, x30, [sp], #16",
    // Return
    "ret",
);

/// Run the scheduler — starts the first task and never returns
pub fn run_scheduler() -> ! {
    let mut tasks = TASKS.lock();
    let len = tasks.len();
    if len == 0 {
        #[cfg(target_arch = "x86_64")]
        crate::arch::x86_64::halt_loop();
        #[cfg(target_arch = "aarch64")]
        crate::arch::aarch64::halt_loop();
    }

    // Start the first task
    let first_id = tasks[0].id;
    let first_rsp = tasks[0].context.rsp;
    let first_pt_root = tasks[0].page_table_root();
    tasks[0].state = TaskState::Running;
    *CURRENT_TASK.lock() = Some(first_id);
    drop(tasks);

    unsafe {
        // Load page tables for first task
        #[cfg(target_arch = "x86_64")]
        crate::arch::x86_64::load_page_tables(first_pt_root);
        #[cfg(target_arch = "aarch64")]
        crate::arch::aarch64::load_page_tables(first_pt_root);

        switch_context(core::ptr::null_mut(), first_rsp);
    }

    // Should never reach here
    loop {
        #[cfg(target_arch = "x86_64")]
        crate::arch::x86_64::halt_loop();
        #[cfg(target_arch = "aarch64")]
        crate::arch::aarch64::halt_loop();
    }
}
