//! Minimal cooperative thread scheduler for Windows threads.
//!
//! A single-core round-robin scheduler that switches between the kernel idle
//! context and ready Windows threads. Threads are stored in a static array,
//! and the ready queue holds indices into that array.

use super::thread::{Thread, ThreadState};
use crate::mm::page_table::{page_table_root, PAGE_PRESENT, PAGE_USER, PAGE_WRITABLE};
use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

const MAX_THREADS: usize = 16;
const MAX_READY: usize = MAX_THREADS;
const USER_STACK_SIZE: usize = 64 * 1024; // 64 KiB user stacks

// Static, pinned storage for thread control blocks. The usage bitmap is
// protected by a separate Mutex so we can hand out &'static mut references.
static mut THREAD_STORAGE: [MaybeUninit<Thread>; MAX_THREADS] =
    [const { MaybeUninit::uninit() }; MAX_THREADS];
static THREAD_USED: Mutex<[bool; MAX_THREADS]> = Mutex::new([false; MAX_THREADS]);

static READY_QUEUE: Mutex<[Option<usize>; MAX_READY]> = Mutex::new([const { None }; MAX_READY]);
static CURRENT_THREAD: Mutex<Option<usize>> = Mutex::new(None);
static mut IDLE_RSP: u64 = 0;
static NEXT_TID: AtomicU64 = AtomicU64::new(1);

/// Return the index of a free thread slot.
fn alloc_thread_slot() -> Option<usize> {
    let used = THREAD_USED.lock();
    used.iter().position(|&in_use| !in_use)
}

/// Create a new thread for `pid` starting at `entry_point`.
///
/// The thread owns both a kernel stack (for scheduler context) and a user stack
/// (for ring-3 execution). `page_table_root` is the physical CR3 value that
/// must be loaded before this thread runs natively.
pub fn create_thread(
    pid: u64,
    entry_point: u64,
    cr3: u64,
) -> Option<usize> {
    let slot = alloc_thread_slot()?;

    // Kernel stack used for cooperative context switching inside the kernel.
    let kstack_size = crate::arch::context_switch::stack_size();
    let kstack_base = crate::mm::alloc_early(kstack_size, 16)? as u64;
    let kstack_top = kstack_base + kstack_size as u64;
    let initial_rsp = crate::arch::context_switch::initial_stack(entry_point, kstack_top);

    // User stack used when the thread runs in ring-3. For native threads map
    // it at a fixed virtual address in the per-process page table; for
    // interpreter threads the kernel accesses the guest stack directly as a
    // physical address.
    const USER_STACK_VIRT: u64 = 0x0000_0000_0007_0000;
    let ustack_base = crate::mm::alloc_early(USER_STACK_SIZE, 4096)? as u64;
    let ustack_top = ustack_base + USER_STACK_SIZE as u64;
    let user_rsp = if cr3 == 0 {
        ustack_top
    } else {
        if let Some(mut pt) = unsafe { page_table_root(cr3) } {
            let pages = USER_STACK_SIZE / 4096;
            let flags = PAGE_PRESENT | PAGE_USER | PAGE_WRITABLE;
            unsafe {
                pt.map_region(USER_STACK_VIRT, ustack_base, pages, flags);
            }
        }
        USER_STACK_VIRT + USER_STACK_SIZE as u64
    };

    let tid = NEXT_TID.fetch_add(1, Ordering::Relaxed);
    let mut thread = Thread::new(tid, pid, entry_point);
    thread.stack_base = kstack_base;
    thread.stack_limit = kstack_base;
    thread.rsp = initial_rsp;
    thread.user_rsp = user_rsp;
    thread.user_rip = entry_point;
    thread.process_page_table_root = cr3;
    thread.state = ThreadState::Ready;

    unsafe {
        THREAD_STORAGE[slot].write(thread);
    }
    {
        let mut used = THREAD_USED.lock();
        used[slot] = true;
    }
    enqueue_thread(slot)?;
    Some(slot)
}

/// Add `slot` to the tail of the ready queue.
fn enqueue_thread(slot: usize) -> Option<()> {
    let mut ready = READY_QUEUE.lock();
    let index = ready.iter().position(|s| s.is_none())?;
    ready[index] = Some(slot);
    Some(())
}

/// Return the next ready thread slot, or `None` if the queue is empty.
fn dequeue_thread() -> Option<usize> {
    let mut ready = READY_QUEUE.lock();
    let head = ready.iter().position(|s| s.is_some())?;
    let slot = ready[head].take()?;
    // Shift remaining entries down to keep the queue compact.
    for i in head..MAX_READY - 1 {
        ready[i] = ready[i + 1];
    }
    ready[MAX_READY - 1] = None;
    Some(slot)
}

/// Return an immutable reference to the thread in `slot`.
pub fn thread(slot: usize) -> Option<&'static Thread> {
    if slot >= MAX_THREADS {
        return None;
    }
    unsafe { Some(&*THREAD_STORAGE[slot].as_ptr()) }
}

/// Return a mutable reference to the thread in `slot`.
fn thread_mut(slot: usize) -> Option<&'static mut Thread> {
    if slot >= MAX_THREADS {
        return None;
    }
    // SAFETY: THREAD_STORAGE is a static array of pinned MaybeUninit cells.
    unsafe { Some(&mut *THREAD_STORAGE[slot].as_mut_ptr()) }
}

/// Run `f` on the currently running thread, if any.
pub fn with_current_thread<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&mut Thread) -> R,
{
    let slot = *CURRENT_THREAD.lock();
    let slot = slot?;
    let t = thread_mut(slot)?;
    Some(f(t))
}

/// Start running `slot` under the x86_64 interpreter.
///
/// # Safety
/// Must only be used for threads whose guest architecture differs from the
/// host. Does not return.
pub unsafe fn enter_interpreter(slot: usize) -> ! {
    let Some(t) = thread_mut(slot) else {
        crate::logln!("scheduler: cannot enter interpreter for missing slot {}", slot);
        crate::hlt();
    };
    t.state = ThreadState::Running;
    *CURRENT_THREAD.lock() = Some(slot);
    crate::win32::abi::interpreter::run_x86_64_loop(t.entry_point);
}

/// Switch to the next ready thread, saving the current context.
///
/// # Safety
/// Must be called with interrupts disabled or from a state where the scheduler
/// data structures will not be re-entered. This function does not return when
/// switching away; it returns when another thread switches back to the caller.
pub unsafe fn schedule() {
    let Some(next_slot) = dequeue_thread() else {
        return;
    };
    let Some(next) = thread_mut(next_slot) else {
        return;
    };

    let new_rsp = next.rsp;
    next.state = ThreadState::Running;

    let current_slot = *CURRENT_THREAD.lock();
    let old_rsp: *mut u64 = match current_slot {
        Some(slot) => {
            let cur = thread_mut(slot).expect("current thread missing");
            if cur.state != ThreadState::Exited {
                cur.state = ThreadState::Ready;
                let _ = enqueue_thread(slot);
            }
            &mut cur.rsp
        }
        None => core::ptr::addr_of_mut!(IDLE_RSP),
    };

    *CURRENT_THREAD.lock() = Some(next_slot);

    crate::arch::context_switch::switch(old_rsp, new_rsp);
}

/// Enter ring-3 for the first time by running `slot` with its per-process page
/// table and user stack.
///
/// # Safety
/// Must only be used to start a process's initial thread. Does not return on
/// success; the SYSCALL handler is responsible for re-entering the kernel.
#[cfg(feature = "arch_x86_64")]
pub unsafe fn enter_user_mode(slot: usize) -> ! {
    let Some(t) = thread_mut(slot) else {
        crate::logln!("scheduler: cannot enter user mode for missing slot {}", slot);
        crate::hlt();
    };

    t.state = ThreadState::Running;
    *CURRENT_THREAD.lock() = Some(slot);

    let cr3 = t.process_page_table_root;
    let user_rip = t.user_rip;
    let user_rsp = t.user_rsp;

    if cr3 != 0 {
        core::arch::asm!(
            "mov cr3, {cr3}",
            cr3 = in(reg) cr3,
        );
    }

    let kstack_top = t.stack_base + crate::arch::context_switch::stack_size() as u64;
    crate::arch::x86_64::syscall::set_syscall_rsp(kstack_top);

    // Push a return address onto the user stack so that when the PE's entry
    // function returns it lands in thread_exit instead of jumping to garbage.
    let user_rsp = user_rsp.wrapping_sub(core::mem::size_of::<u64>() as u64);
    unsafe {
        core::ptr::write(user_rsp as *mut u64, thread_exit as *const () as u64);
    }

    crate::arch::x86_64::syscall::sysret_to_user(user_rip, user_rsp);
}

/// Entry point placed on every new thread stack. Called if a thread function
/// ever returns.
#[no_mangle]
pub extern "C" fn thread_exit() -> ! {
    {
        if let Some(slot) = *CURRENT_THREAD.lock() {
            if let Some(cur) = thread_mut(slot) {
                cur.state = ThreadState::Exited;
            }
        }
    }
    unsafe { schedule() };
    crate::hlt();
}
