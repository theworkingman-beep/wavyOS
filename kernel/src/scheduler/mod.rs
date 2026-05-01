//! Round-robin cooperative scheduler
use alloc::collections::vec_deque::VecDeque;
use spin::Mutex;

static TASKS: Mutex<VecDeque<Task>> = Mutex::new(VecDeque::new());

#[derive(Debug, Clone, Copy)]
pub struct TaskId(usize);

pub struct Task {
    pub id: TaskId,
    pub stack: alloc::vec::Vec<u8>,
    pub state: TaskState,
    pub entry: fn(),
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

pub fn spawn(entry: fn()) {
    let id = TaskId(TASKS.lock().len());
    let task = Task {
        id,
        stack: alloc::vec![0u8; 4096 * 16], // 64KB stack
        state: TaskState::Ready,
        entry,
    };
    TASKS.lock().push_back(task);
}

pub fn run_first_task() -> ! {
    loop {
        if let Some(mut task) = TASKS.lock().pop_front() {
            task.state = TaskState::Running;
            (task.entry)();
            task.state = TaskState::Dead;
        }
        // idle loop for now
        #[cfg(target_arch = "x86_64")]
        crate::arch::x86_64::halt_loop();
        #[cfg(target_arch = "aarch64")]
        crate::arch::aarch64::halt_loop();
    }
}
