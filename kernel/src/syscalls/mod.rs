//! Syscall dispatch table

pub fn init() {}

/// C-compatible entry point called from x86_64 syscall assembly
#[no_mangle]
pub unsafe extern "C" fn syscall_dispatch(
    n: usize, a1: usize, a2: usize, a3: usize, a4: usize, a5: usize, a6: usize,
) -> usize {
    dispatch(n, a1, a2, a3, a4, a5, a6)
}

#[repr(usize)]
pub enum Syscall {
    Exit = 0,
    Write = 1,
    Read = 2,
    Spawn = 3,
    Yield = 4,
    IpcSend = 5,
    IpcRecv = 6,
    ShmCreate = 7,
    ShmMap = 8,
    FramebufferMap = 9,
    MachOExec = 0x700,
}

/// Full dispatch with up to 6 arguments
pub unsafe fn dispatch(n: usize, a1: usize, a2: usize, a3: usize, a4: usize, a5: usize, a6: usize) -> usize {
    match n {
        0 => {
            // exit — terminate current task
            // In a real OS, this would clean up the task
            0
        }
        1 => {
            // write(fd, buf, count) — stub: just return count
            a3
        }
        2 => {
            // read(fd, buf, count) — stub
            0
        }
        3 => {
            // spawn(entry_point) — spawn a new user task
            crate::scheduler::spawn_user(a1);
            0
        }
        4 => {
            // yield — yield CPU to next task
            crate::scheduler::yield_cpu();
            0
        }
        5 => {
            // ipc_send(target_tid, msg_ptr)
            // Stub for now
            0
        }
        6 => {
            // ipc_recv
            // Stub for now
            0
        }
        7 => {
            // shm_create(size)
            match crate::shm::create(a1) {
                Some(id) => id,
                None => 0,
            }
        }
        8 => {
            // shm_map(id)
            match crate::shm::lookup(a1) {
                Some((start, _)) => start,
                None => 0,
            }
        }
        9 => {
            // framebuffer_map — return framebuffer info
            // a1 = pointer to FramebufferInfo to fill
            if a1 != 0 {
                // Fill user-provided FramebufferInfo struct
                // For now just return 0
            }
            0
        }
        0x700 => crate::compat::macho::exec(a1 as *const u8, a2 as usize),
        _ => {
            log::warn!("Unknown syscall: {}", n);
            0
        }
    }
}

/// Wrapper for x86_64 syscall entry (fewer args)
pub unsafe fn dispatch_3(n: usize, a1: usize, a2: usize, a3: usize) -> usize {
    dispatch(n, a1, a2, a3, 0, 0, 0)
}
