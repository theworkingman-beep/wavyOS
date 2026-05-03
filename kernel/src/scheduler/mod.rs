//! Round-robin cooperative scheduler
use alloc::collections::vec_deque::VecDeque;
use spin::Mutex;

static TASKS: Mutex<VecDeque<Task>> = Mutex::new(VecDeque::new());
static CURRENT_TASK: Mutex<Option<TaskId>> = Mutex::new(None);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskId(usize);

// Task function pointer type: extern "C" fn() -> !
pub type EntryPoint = extern "C" fn() -> !;

pub struct Task {
    pub id: TaskId,
    pub stack_top: usize,
    pub state: TaskState,
    pub entry: EntryPoint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    Ready,
    Running,
    Blocked,
    Dead,
}

pub fn init() {
    // scheduler init
}

pub fn spawn(entry: EntryPoint, stack_top: Option<usize>) {
    let id = TaskId(TASKS.lock().len());
    let top = match stack_top {
        Some(p) => p,
        None => {
            // allocate a stack via mm allocator
            // SAFETY: only called during kernel init, no concurrency here
            unsafe { crate::mm::allocate_stack_top() }
        }
    };
    let task = Task {
        id,
        stack_top: top,
        state: TaskState::Ready,
        entry,
    };
    TASKS.lock().push_back(task);
}

pub fn current_task_id() -> TaskId {
    CURRENT_TASK.lock().clone().unwrap_or(TaskId(0))
}

pub fn run_first_task() -> ! {
    loop {
        if let Some(mut task) = TASKS.lock().pop_front() {
            task.state = TaskState::Running;
            *CURRENT_TASK.lock() = Some(task.id);
            // Jump to the task entry using inline asm
            unsafe {
                #[cfg(target_arch = "x86_64")]
                core::arch::asm!("call {}", in(reg) task.entry as usize, options(noreturn));
                #[cfg(target_arch = "aarch64")]
                core::arch::asm!("blr {}", in(reg) task.entry as usize, options(noreturn));
            }
            // If the entry returns (shouldn't for !), mark dead and continue
            task.state = TaskState::Dead;
        }
        // idle loop for now
        #[cfg(target_arch = "x86_64")]
        crate::arch::x86_64::halt_loop();
        #[cfg(target_arch = "aarch64")]
        crate::arch::aarch64::halt_loop();
    }
}
