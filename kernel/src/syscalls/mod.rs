//! Syscall dispatch — real implementation
use core::ptr;
use core::slice;

/// Syscall numbers (matches user-space ABI)
#[repr(usize)]
#[derive(Clone, Copy, Debug)]
pub enum Syscall {
    Exit = 0,
    Write = 1,
    Read = 2,
    Spawn = 3,
    Yield = 4,
    Fork = 5,
    Wait = 6,
    IpcSend = 7,
    IpcRecv = 8,
    ShmCreate = 9,
    ShmMap = 10,
    FramebufferMap = 11,
    MachOExec = 0x700,
}

/// C-compatible entry point called from x86_64 syscall assembly
#[no_mangle]
pub unsafe extern "C" fn syscall_dispatch(
    n: usize, a1: usize, a2: usize, a3: usize, a4: usize, a5: usize, a6: usize,
) -> usize {
    dispatch(n, a1, a2, a3, a4, a5, a6)
}

/// Copy bytes from user-space address to kernel buffer.
/// Returns number of bytes successfully copied.
pub unsafe fn copy_from_user(kbuf: &mut [u8], user_ptr: usize) -> usize {
    let user_slice = slice::from_raw_parts(user_ptr as *const u8, kbuf.len());
    kbuf.copy_from_slice(user_slice);
    kbuf.len()
}

/// Copy bytes from kernel buffer to user-space address.
/// Returns number of bytes successfully copied.
pub unsafe fn copy_to_user(user_ptr: usize, kbuf: &[u8]) -> usize {
    let user_slice = slice::from_raw_parts_mut(user_ptr as *mut u8, kbuf.len());
    user_slice.copy_from_slice(kbuf);
    kbuf.len()
}

/// Write to UART from user buffer
unsafe fn sys_write(fd: usize, user_buf: usize, count: usize) -> usize {
    if fd != 1 || user_buf == 0 || count == 0 {
        return 0;
    }
    let count = count.min(4096);
    let mut kbuf = [0u8; 4096];
    let to_copy = count.min(kbuf.len());
    copy_from_user(&mut kbuf[..to_copy], user_buf);
    for &b in &kbuf[..to_copy] {
        crate::drivers::uart::putc(b);
    }
    to_copy
}

/// Read from input buffer into user buffer
unsafe fn sys_read(_fd: usize, user_buf: usize, count: usize) -> usize {
    if user_buf == 0 || count == 0 {
        return 0;
    }
    let mut kbuf = [0u8; 256];
    let to_read = count.min(kbuf.len());
    let mut n = 0usize;
    use crate::input::{self, InputEvent};
    while n < to_read {
        match input::poll() {
            Some(InputEvent::KeyPress { ascii }) => {
                kbuf[n] = ascii;
                n += 1;
            }
            _ => break,
        }
    }
    if n > 0 {
        copy_to_user(user_buf, &kbuf[..n]);
    }
    n
}

/// Full dispatch with up to 6 arguments
pub unsafe fn dispatch(n: usize, a1: usize, a2: usize, a3: usize, a4: usize, _a5: usize, _a6: usize) -> usize {
    match n {
        0 => {
            // exit(code) — terminate current process
            let code = a1 as i32;
            crate::scheduler::exit(code);
        }
        1 => {
            // write(fd, buf, count)
            sys_write(a1, a2, a3)
        }
        2 => {
            // read(fd, buf, count)
            sys_read(a1, a2, a3)
        }
        3 => {
            // spawn(entry_point) — spawn a new user task, returns PID
            crate::scheduler::spawn_user(a1)
        }
        4 => {
            // yield — yield CPU to next task
            crate::scheduler::yield_cpu();
            0
        }
        5 => {
            // fork — create child process, returns child PID to parent, 0 to child
            crate::scheduler::fork()
        }
        6 => {
            // wait(pid) — wait for child process, returns (pid, status)
            let pid = a1 as isize;
            let (ret_pid, status) = crate::scheduler::wait(pid);
            ((ret_pid as usize) << 32) | (status as usize & 0xFFFFFFFF)
        }
        7 => {
            // ipc_send(target_pid, msg_ptr, msg_size)
            log::warn!("syscall: ipc_send not fully implemented");
            0
        }
        8 => {
            // ipc_recv(msg_ptr, msg_size) — receive IPC message
            log::warn!("syscall: ipc_recv not fully implemented");
            0
        }
        9 => {
            // shm_create(size) — create shared memory region
            match crate::shm::create(a1) {
                Some(id) => id,
                None => 0,
            }
        }
        10 => {
            // shm_map(id) — map shared memory region into address space
            match crate::shm::lookup(a1) {
                Some((start, _size)) => start,
                None => 0,
            }
        }
        11 => {
            // framebuffer_map — return framebuffer physical address and info
            // a1 = pointer to FramebufferInfo struct to fill
            if a1 != 0 {
                let fb_info = crate::drivers::fbcon::get_info();
                ptr::write(a1 as *mut crate::FramebufferInfo, fb_info);
                return 0;
            }
            // If a1 is 0, return framebuffer physical address (for mmap)
            crate::drivers::fbcon::get_phys_addr()
        }
        0x700 => crate::compat::macho::exec(a1 as *const u8, a2 as usize),
        _ => {
            log::warn!("Unknown syscall: {}", n);
            0
        }
    }
}

/// Wrapper for x86_64 syscall entry (fewer args — compatibility)
pub unsafe fn dispatch_3(n: usize, a1: usize, a2: usize, a3: usize) -> usize {
    dispatch(n, a1, a2, a3, 0, 0, 0)
}

pub fn init() {
    log::info!("syscalls: dispatch table initialized");
}
