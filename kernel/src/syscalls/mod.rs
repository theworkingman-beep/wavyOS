//! Syscall dispatch table with full implementation

use core::ptr;

pub fn init() {
    log::info!("syscalls: initialized");
}

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
    Fork = 5,
    Wait = 6,
    IpcSend = 7,
    IpcRecv = 8,
    ShmCreate = 9,
    ShmMap = 10,
    FramebufferMap = 11,
    MachOExec = 0x700,
}

/// Full dispatch with up to 6 arguments
pub unsafe fn dispatch(n: usize, a1: usize, a2: usize, a3: usize, a4: usize, a5: usize, a6: usize) -> usize {
    match n {
        0 => {
            // exit(code) — terminate current task
            let code = a1 as i32;
            log::info!("syscall: exit({})", code);
            crate::scheduler::exit(code);
        }
        1 => {
            // write(fd, buf, count) — write to UART for fd 1/2, return bytes written
            let fd = a1;
            let buf = a2 as *const u8;
            let count = a3;
            if count == 0 { return 0; }
            match fd {
                1 | 2 => {
                    // stdout/stderr — write to UART
                    let mut written = 0;
                    for i in 0..count {
                        let byte = ptr::read(buf.add(i));
                        if byte == 0 { break; }
                        crate::drivers::uart::putc(byte);
                        written += 1;
                    }
                    written
                }
                _ => {
                    log::warn!("syscall: write to unsupported fd {}", fd);
                    0
                }
            }
        }
        2 => {
            // read(fd, buf, count) — read from input ring buffer for fd 0
            let fd = a1;
            let buf = a2 as *mut u8;
            let count = a3;
            if fd != 0 {
                log::warn!("syscall: read from unsupported fd {}", fd);
                return 0;
            }
            // Read from input subsystem
            let mut bytes_read = 0;
            for _ in 0..count {
                if let Some(key) = crate::input::try_recv_key() {
                    ptr::write(buf.add(bytes_read), key as u8);
                    bytes_read += 1;
                } else {
                    break;
                }
            }
            bytes_read
        }
        3 => {
            // spawn(entry_point) — spawn a new user task, returns PID
            let pid = crate::scheduler::spawn_user(a1);
            pid
        }
        4 => {
            // yield — yield CPU to next task
            crate::scheduler::yield_cpu();
            0
        }
        5 => {
            // fork — create child process, returns child PID to parent, 0 to child
            let child_pid = crate::scheduler::fork();
            child_pid
        }
        6 => {
            // wait(pid) — wait for child process, returns (pid, status)
            let pid = a1 as isize;
            let (ret_pid, status) = crate::scheduler::wait(pid);
            // Pack pid into upper bits, status into lower 32 bits
            ((ret_pid as usize) << 32) | (status as usize & 0xFFFFFFFF)
        }
        7 => {
            // ipc_send(target_pid, msg_ptr, msg_size)
            // Stub for now — need to implement IPC properly
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
            // For now, return the physical address
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
                return 0; // Success
            }
            // If a1 is 0, return framebuffer physical address (for mmap)
            crate::drivers::fbcon::get_phys_addr()
        }
        0x700 => {
            // Mach-O exec
            crate::compat::macho::exec(a1 as *const u8, a2 as usize)
        }
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
