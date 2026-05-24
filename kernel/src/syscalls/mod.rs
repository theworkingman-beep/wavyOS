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
    Exec = 7,
    IpcSend = 8,
    IpcRecv = 9,
    ShmCreate = 10,
    ShmMap = 11,
    FramebufferMap = 12,
    InputPoll = 13,
    PtysOpen = 14,
    PtysRead = 15,
    PtysWrite = 16,
    SpawnPtyShell = 17,
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
            // exec(path_ptr, argv_ptr) — replace current process with new executable
            // path_ptr points to ELF binary in memory
            let elf_data = a1 as *const u8;
            let elf_size = a2;
            if elf_size == 0 || elf_data.is_null() {
                return 0;
            }
            let data_slice = unsafe { core::slice::from_raw_parts(elf_data, elf_size) };

            // Get current task
            let pid = crate::scheduler::current_task_id();
            let mut procs = crate::scheduler::PROCESSES.lock();
            if let Some(proc) = procs.iter_mut().find(|p| p.pid == pid) {
                // Load ELF into task's address space
                if let Some((entry, stack_top)) = crate::userland::loader::load_elf_for_task(data_slice, &mut proc.task) {
                    // Set up context for user-space execution
                    proc.task.entry = entry as usize;
                    proc.task.task_type = crate::scheduler::TaskType::User;
                    // Set stack pointer in context
                    // For x86_64, we need to set up the stack properly
                    // The context switch will handle jumping to user mode
                    log::info!("syscall: exec loaded ELF, entry={:#x}", entry);
                    return entry as usize;
                }
            }
            0
        }
        8 => {
            // ipc_send(target_pid, msg_ptr, msg_size)
            let target = a1;
            let msg_ptr = a2 as *const u8;
            let msg_len = a3;
            if msg_ptr.is_null() || msg_len < 1 {
                return 0;
            }
            // Copy message from user space and build IpcMessage
            let mut msg = crate::ipc::IpcMessage::new(
                crate::scheduler::current_task_id(),
                0, // msg_type from first byte
            );
            let copy_len = msg_len.min(crate::ipc::IPC_PAYLOAD_SIZE);
            unsafe {
                core::ptr::copy_nonoverlapping(
                    msg_ptr,
                    msg.payload.as_mut_ptr(),
                    copy_len,
                );
            }
            // Extract msg_type from the first byte of payload (convention)
            msg.msg_type = msg.payload[0];
            crate::ipc::send(target, msg);
            1 // success
        }
        9 => {
            // ipc_recv(msg_ptr, msg_size) — receive IPC message for current process
            let buf_ptr = a1 as *mut u8;
            let buf_len = a2;
            let my_pid = crate::scheduler::current_task_id();
            match crate::ipc::recv(my_pid) {
                Some(msg) => {
                    if !buf_ptr.is_null() && buf_len >= crate::ipc::IPC_PAYLOAD_SIZE {
                        unsafe {
                            core::ptr::copy_nonoverlapping(
                                msg.payload.as_ptr(),
                                buf_ptr,
                                crate::ipc::IPC_PAYLOAD_SIZE,
                            );
                        }
                    }
                    // Return sender PID in upper bits, msg_type in lower byte
                    (msg.sender << 8) | (msg.msg_type as usize)
                }
                None => {
                    0 // no message available
                }
            }
        }
        10 => {
            // shm_create(size) — create shared memory region
            match crate::shm::create(a1) {
                Some(id) => id,
                None => 0,
            }
        }
        11 => {
            // shm_map(id) — map shared memory region into address space
            match crate::shm::lookup(a1) {
                Some((start, _size)) => start,
                None => 0,
            }
        }
        12 => {
            // framebuffer_map — return framebuffer physical address and info
            if a1 != 0 {
                let fb_info = crate::drivers::fbcon::get_info();
                ptr::write(a1 as *mut crate::FramebufferInfo, fb_info);
                return 0;
            }
            crate::drivers::fbcon::get_phys_addr()
        }
        13 => {
            // input_poll(buf_ptr) — poll next input event, write to user buffer
            // Returns 1 if event available, 0 if none
            // Event is serialized as 8 bytes:
            //   byte 0: event type (0=MouseMove, 1=MouseDown, 2=MouseUp, 3=KeyPress)
            //   bytes 1-2: x (u16 LE)
            //   bytes 3-4: y (u16 LE)
            //   byte 5: buttons (for MouseMove) or button (for MouseDown/Up) or ascii (for KeyPress)
            //   bytes 6-7: reserved
            match crate::input::poll() {
                Some(event) => {
                    let buf = a1 as *mut u8;
                    if !buf.is_null() {
                        let mut data = [0u8; 8];
                        match event {
                            crate::input::InputEvent::MouseMove { x, y, buttons } => {
                                data[0] = 0;
                                data[1] = x as u8;
                                data[2] = (x >> 8) as u8;
                                data[3] = y as u8;
                                data[4] = (y >> 8) as u8;
                                data[5] = buttons;
                            }
                            crate::input::InputEvent::MouseDown { button, x, y } => {
                                data[0] = 1;
                                data[1] = x as u8;
                                data[2] = (x >> 8) as u8;
                                data[3] = y as u8;
                                data[4] = (y >> 8) as u8;
                                data[5] = button;
                            }
                            crate::input::InputEvent::MouseUp { button, x, y } => {
                                data[0] = 2;
                                data[1] = x as u8;
                                data[2] = (x >> 8) as u8;
                                data[3] = y as u8;
                                data[4] = (y >> 8) as u8;
                                data[5] = button;
                            }
                            crate::input::InputEvent::KeyPress { ascii } => {
                                data[0] = 3;
                                data[5] = ascii;
                            }
                        }
                        unsafe {
                            core::ptr::copy_nonoverlapping(data.as_ptr(), buf, 8);
                        }
                    }
                    1 // event available
                }
                None => 0,
            }
        }
        14 => {
            // ptys_open() — create a PTY master/slave pair
            // Returns PTY ID (>=1 on success, 0 on failure)
            let master_pid = crate::scheduler::current_task_id();
            crate::pty::pty_open(master_pid)
        }
        15 => {
            // ptys_read(pty_id, buf_ptr, buf_len) — read from PTY master (slave output)
            let pty_id = a1;
            let buf_ptr = a2 as *mut u8;
            let buf_len = a3;
            if buf_ptr.is_null() || buf_len == 0 { return 0; }
            let buf = unsafe { core::slice::from_raw_parts_mut(buf_ptr, buf_len) };
            // Check if caller is the master of this PTY
            let caller_pid = crate::scheduler::current_task_id();
            if crate::pty::pty_find_by_master_pid(caller_pid) != pty_id && pty_id != 0 {
                // Also allow slave to read (for self-reading, though unusual)
                if crate::pty::pty_find_by_slave_pid(caller_pid) != pty_id {
                    log::warn!("syscall: ptys_read denied for pid={} on pty={}", caller_pid, pty_id);
                    return 0;
                }
            }
            crate::pty::pty_master_read(pty_id, buf)
        }
        16 => {
            // ptys_write(pty_id, buf_ptr, buf_len) — write to PTY master (keyboard input to slave)
            let pty_id = a1;
            let buf_ptr = a2 as *const u8;
            let buf_len = a3;
            if buf_ptr.is_null() || buf_len == 0 { return 0; }
            let data = unsafe { core::slice::from_raw_parts(buf_ptr, buf_len) };
            let caller_pid = crate::scheduler::current_task_id();
            if crate::pty::pty_find_by_master_pid(caller_pid) != pty_id {
                // Also allow slave to write to its own master (slave->master direction)
                if crate::pty::pty_find_by_slave_pid(caller_pid) == pty_id {
                    return crate::pty::pty_slave_write(pty_id, data);
                }
                log::warn!("syscall: ptys_write denied for pid={} on pty={}", caller_pid, pty_id);
                return 0;
            }
            crate::pty::pty_master_write(pty_id, data)
        }
        17 => {
            // spawn_pty_shell(pty_id) — spawn the kernel shell as a child process
            // connected to the given PTY's slave side.
            // Returns the child PID on success, 0 on failure.
            let pty_id = a1;
            // Spawn a new kernel task running a shell connected to this PTY
            let child_pid = crate::scheduler::spawn_shell_with_pty(pty_id);
            if child_pid == 0 {
                log::warn!("syscall: spawn_pty_shell failed to spawn shell task");
                return 0;
            }
            // Assign the child process as the slave of this PTY
            crate::pty::pty_assign_slave(pty_id, child_pid);
            log::info!("syscall: spawn_pty_shell spawned shell pid={} on PTY id={}", child_pid, pty_id);
            child_pid
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
