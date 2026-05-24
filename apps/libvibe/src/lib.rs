#![no_std]

extern crate alloc;

// Syscall numbers matching kernel/src/syscalls/mod.rs
pub const SYS_EXIT: usize = 0;
pub const SYS_WRITE: usize = 1;
pub const SYS_READ: usize = 2;
pub const SYS_SPAWN: usize = 3;
pub const SYS_YIELD: usize = 4;
pub const SYS_FORK: usize = 5;
pub const SYS_WAIT: usize = 6;
pub const SYS_EXEC: usize = 7;
pub const SYS_IPC_SEND: usize = 8;
pub const SYS_IPC_RECV: usize = 9;
pub const SYS_SHM_CREATE: usize = 10;
pub const SYS_SHM_MAP: usize = 11;
pub const SYS_FRAMEBUFFER_MAP: usize = 12;
pub const SYS_INPUT_POLL: usize = 13;
pub const SYS_PTYS_OPEN: usize = 14;
pub const SYS_PTYS_READ: usize = 15;
pub const SYS_PTYS_WRITE: usize = 16;
pub const SYS_SPAWN_PTY_SHELL: usize = 17;

// New POSIX syscalls
pub const SYS_OPEN: usize = 18;
pub const SYS_CLOSE: usize = 19;
pub const SYS_READ_FD: usize = 20;
pub const SYS_WRITE_FD: usize = 21;
pub const SYS_SEEK: usize = 22;
pub const SYS_FSTAT: usize = 23;
pub const SYS_MKDIR: usize = 24;
pub const SYS_UNLINK: usize = 25;
pub const SYS_GETPID: usize = 26;
pub const SYS_DUP: usize = 27;
pub const SYS_PIPE: usize = 28;
pub const SYS_MMAP: usize = 29;
pub const SYS_MUNMAP: usize = 30;
pub const SYS_IOCTL: usize = 31;
pub const SYS_GETTIMEOFDAY: usize = 32;
pub const SYS_NANOSLEEP: usize = 33;

// Audio syscalls
pub const SYS_AUDIO_BEEP: usize = 34;
pub const SYS_AUDIO_PCM_WRITE: usize = 35;
pub const SYS_AUDIO_VOLUME: usize = 36;

// IPC payload size matching kernel/src/ipc.rs
pub const IPC_PAYLOAD_SIZE: usize = 64;

// WindowServer protocol message types (stored in first byte of IPC payload)
pub const MSG_CREATE_WINDOW: u8 = 1;
pub const MSG_DESTROY_WINDOW: u8 = 2;
pub const MSG_MOVE_WINDOW: u8 = 3;
pub const MSG_RESIZE_WINDOW: u8 = 4;
pub const MSG_WINDOW_READY: u8 = 5;
pub const MSG_FOCUS_WINDOW: u8 = 6;
pub const MSG_INPUT_EVENT: u8 = 7;
pub const MSG_CLOSE_WINDOW: u8 = 8;
pub const MSG_DOCK_CLICK: u8 = 9;

// Input event types (matching kernel serialization in syscall 13)
pub const INPUT_MOUSE_MOVE: u8 = 0;
pub const INPUT_MOUSE_DOWN: u8 = 1;
pub const INPUT_MOUSE_UP: u8 = 2;
pub const INPUT_KEY_PRESS: u8 = 3;

#[cfg(target_arch = "x86_64")]
pub unsafe fn syscall1(n: usize, a1: usize) -> usize {
    let ret: usize;
    core::arch::asm!(
        "int 0x80",
        inlateout("rax") n => ret,
        in("rdi") a1,
        options(nostack, preserves_flags)
    );
    ret
}

#[cfg(target_arch = "x86_64")]
pub unsafe fn syscall2(n: usize, a1: usize, a2: usize) -> usize {
    let ret: usize;
    core::arch::asm!(
        "int 0x80",
        inlateout("rax") n => ret,
        in("rdi") a1,
        in("rsi") a2,
        options(nostack, preserves_flags)
    );
    ret
}

#[cfg(target_arch = "x86_64")]
pub unsafe fn syscall3(n: usize, a1: usize, a2: usize, a3: usize) -> usize {
    let ret: usize;
    core::arch::asm!(
        "int 0x80",
        inlateout("rax") n => ret,
        in("rdi") a1,
        in("rsi") a2,
        in("rdx") a3,
        options(nostack, preserves_flags)
    );
    ret
}

#[cfg(target_arch = "aarch64")]
pub unsafe fn syscall1(n: usize, a1: usize) -> usize {
    let ret: usize;
    core::arch::asm!(
        "svc #0",
        inlateout("x8") n => _,
        inlateout("x0") a1 => ret,
        options(nostack, preserves_flags)
    );
    ret
}

#[cfg(target_arch = "aarch64")]
pub unsafe fn syscall2(n: usize, a1: usize, a2: usize) -> usize {
    let ret: usize;
    core::arch::asm!(
        "svc #0",
        inlateout("x8") n => _,
        inlateout("x0") a1 => ret,
        in("x1") a2,
        options(nostack, preserves_flags)
    );
    ret
}

#[cfg(target_arch = "aarch64")]
pub unsafe fn syscall3(n: usize, a1: usize, a2: usize, a3: usize) -> usize {
    let ret: usize;
    core::arch::asm!(
        "svc #0",
        inlateout("x8") n => _,
        inlateout("x0") a1 => ret,
        in("x1") a2,
        in("x2") a3,
        options(nostack, preserves_flags)
    );
    ret
}

#[cfg(target_arch = "x86_64")]
pub unsafe fn syscall6(n: usize, a1: usize, a2: usize, a3: usize, a4: usize, a5: usize, a6: usize) -> usize {
    let ret: usize;
    core::arch::asm!(
        "int 0x80",
        inlateout("rax") n => ret,
        in("rdi") a1,
        in("rsi") a2,
        in("rdx") a3,
        in("rcx") a4,
        in("r8") a5,
        in("r9") a6,
        options(nostack, preserves_flags)
    );
    ret
}

#[cfg(target_arch = "aarch64")]
pub unsafe fn syscall6(n: usize, a1: usize, a2: usize, a3: usize, a4: usize, a5: usize, a6: usize) -> usize {
    let ret: usize;
    core::arch::asm!(
        "svc #0",
        inlateout("x8") n => _,
        inlateout("x0") a1 => ret,
        in("x1") a2,
        in("x2") a3,
        in("x3") a4,
        in("x4") a5,
        in("x5") a6,
        options(nostack, preserves_flags)
    );
    ret
}

/// Input event structure deserialized from kernel
#[derive(Debug, Clone, Copy)]
pub struct InputEvent {
    pub event_type: u8,
    pub x: u16,
    pub y: u16,
    pub extra: u8, // buttons, button, or ascii depending on event_type
}

impl InputEvent {
    pub fn from_bytes(data: &[u8; 8]) -> Self {
        InputEvent {
            event_type: data[0],
            x: u16::from_le_bytes([data[1], data[2]]),
            y: u16::from_le_bytes([data[3], data[4]]),
            extra: data[5],
        }
    }

    pub fn is_mouse_move(&self) -> bool {
        self.event_type == INPUT_MOUSE_MOVE
    }

    pub fn is_mouse_down(&self) -> bool {
        self.event_type == INPUT_MOUSE_DOWN
    }

    pub fn is_mouse_up(&self) -> bool {
        self.event_type == INPUT_MOUSE_UP
    }

    pub fn is_key_press(&self) -> bool {
        self.event_type == INPUT_KEY_PRESS
    }

    pub fn buttons(&self) -> u8 {
        self.extra
    }

    pub fn button(&self) -> u8 {
        self.extra
    }

    pub fn ascii(&self) -> u8 {
        self.extra
    }
}

/// Framebuffer info structure matching kernel common::FramebufferInfo
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct FramebufferInfo {
    pub addr: u64,
    pub width: u32,
    pub height: u32,
    pub pitch: u32,
    pub bpp: u8,
}

// ---- Syscall wrappers ----

/// Exit the current process
pub fn exit(code: i32) -> ! {
    unsafe { let _ = syscall1(SYS_EXIT, code as usize); }
    loop {}
}

/// Write bytes to fd (1=stdout, 2=stderr)
pub fn write(fd: usize, buf: &[u8]) -> usize {
    if buf.is_empty() { return 0; }
    unsafe { syscall3(SYS_WRITE, fd, buf.as_ptr() as usize, buf.len()) }
}

/// Yield CPU to next task
pub fn yield_cpu() {
    unsafe { let _ = syscall1(SYS_YIELD, 0); }
}

/// Send IPC message to target_pid. Payload is 64 bytes.
/// First byte is the message type.
/// Returns 1 on success, 0 on failure.
pub fn ipc_send(target_pid: usize, msg: &[u8; IPC_PAYLOAD_SIZE]) -> usize {
    unsafe { syscall3(SYS_IPC_SEND, target_pid, msg.as_ptr() as usize, IPC_PAYLOAD_SIZE) }
}

/// Receive an IPC message into buf (64 bytes).
/// Returns sender info (sender_pid << 8 | msg_type) if message available, 0 if none.
pub fn ipc_recv(buf: &mut [u8; IPC_PAYLOAD_SIZE]) -> usize {
    unsafe { syscall3(SYS_IPC_RECV, buf.as_mut_ptr() as usize, IPC_PAYLOAD_SIZE, 0) }
}

/// Create shared memory region of given size. Returns SHM ID, or 0 on failure.
pub fn shm_create(size: usize) -> usize {
    unsafe { syscall1(SYS_SHM_CREATE, size) }
}

/// Map shared memory region by ID. Returns pointer, or null on failure.
pub fn shm_map(id: usize) -> *mut u8 {
    unsafe { syscall1(SYS_SHM_MAP, id) as *mut u8 }
}

/// Map the framebuffer. If info_ptr is non-null, writes FramebufferInfo there.
/// Returns framebuffer physical address.
pub fn framebuffer_map(info_ptr: *mut FramebufferInfo) -> usize {
    unsafe { syscall1(SYS_FRAMEBUFFER_MAP, info_ptr as usize) }
}

/// Poll for the next input event. Returns Some(InputEvent) if available, None otherwise.
pub fn input_poll() -> Option<InputEvent> {
    let mut data = [0u8; 8];
    let result = unsafe { syscall1(SYS_INPUT_POLL, data.as_mut_ptr() as usize) };
    if result != 0 {
        Some(InputEvent::from_bytes(&data))
    } else {
        None
    }
}

/// Spawn a new user-space task at the given entry point. Returns PID.
pub fn spawn(entry: usize) -> usize {
    unsafe { syscall1(SYS_SPAWN, entry) }
}

// ---- WindowServer protocol helpers ----

/// Build a CreateWindow IPC message
pub fn msg_create_window(x: u16, y: u16, w: u16, h: u16) -> [u8; IPC_PAYLOAD_SIZE] {
    let mut msg = [0u8; IPC_PAYLOAD_SIZE];
    msg[0] = MSG_CREATE_WINDOW;
    msg[1] = x as u8; msg[2] = (x >> 8) as u8;
    msg[3] = y as u8; msg[4] = (y >> 8) as u8;
    msg[5] = w as u8; msg[6] = (w >> 8) as u8;
    msg[7] = h as u8; msg[8] = (h >> 8) as u8;
    msg
}

/// Build a DestroyWindow IPC message (window_id in bytes 1-2)
pub fn msg_destroy_window(window_id: u16) -> [u8; IPC_PAYLOAD_SIZE] {
    let mut msg = [0u8; IPC_PAYLOAD_SIZE];
    msg[0] = MSG_DESTROY_WINDOW;
    msg[1] = window_id as u8;
    msg[2] = (window_id >> 8) as u8;
    msg
}

/// Build a MoveWindow IPC message
pub fn msg_move_window(window_id: u16, x: u16, y: u16) -> [u8; IPC_PAYLOAD_SIZE] {
    let mut msg = [0u8; IPC_PAYLOAD_SIZE];
    msg[0] = MSG_MOVE_WINDOW;
    msg[1] = window_id as u8; msg[2] = (window_id >> 8) as u8;
    msg[3] = x as u8; msg[4] = (x >> 8) as u8;
    msg[5] = y as u8; msg[6] = (y >> 8) as u8;
    msg
}

/// Build a ResizeWindow IPC message
pub fn msg_resize_window(window_id: u16, w: u16, h: u16) -> [u8; IPC_PAYLOAD_SIZE] {
    let mut msg = [0u8; IPC_PAYLOAD_SIZE];
    msg[0] = MSG_RESIZE_WINDOW;
    msg[1] = window_id as u8; msg[2] = (window_id >> 8) as u8;
    msg[3] = w as u8; msg[4] = (w >> 8) as u8;
    msg[5] = h as u8; msg[6] = (h >> 8) as u8;
    msg
}

/// Parse window ID from a WindowReady response (bytes 1-2)
pub fn parse_window_id(msg: &[u8; IPC_PAYLOAD_SIZE]) -> u16 {
    u16::from_le_bytes([msg[1], msg[2]])
}

/// Parse x from IPC payload bytes 1-2 (u16 LE)
pub fn parse_u16_at(msg: &[u8; IPC_PAYLOAD_SIZE], offset: usize) -> u16 {
    if offset + 1 < IPC_PAYLOAD_SIZE {
        u16::from_le_bytes([msg[offset], msg[offset + 1]])
    } else {
        0
    }
}

// ---- PTY Syscall wrappers ----

/// Open a new PTY master/slave pair. Returns PTY ID (>=1 on success, 0 on failure).
pub fn ptys_open() -> usize {
    unsafe { syscall1(SYS_PTYS_OPEN, 0) }
}

/// Read from PTY master (read output from slave/shell).
/// Returns number of bytes read.
pub fn ptys_read(pty_id: usize, buf: &mut [u8]) -> usize {
    if buf.is_empty() { return 0; }
    unsafe { syscall3(SYS_PTYS_READ, pty_id, buf.as_mut_ptr() as usize, buf.len()) }
}

/// Write to PTY master (send keyboard input to slave/shell).
/// Returns number of bytes written.
pub fn ptys_write(pty_id: usize, data: &[u8]) -> usize {
    if data.is_empty() { return 0; }
    unsafe { syscall3(SYS_PTYS_WRITE, pty_id, data.as_ptr() as usize, data.len()) }
}

/// Spawn the kernel shell connected to a PTY. Returns child PID (0 on failure).
pub fn spawn_pty_shell(pty_id: usize) -> usize {
    unsafe { syscall1(SYS_SPAWN_PTY_SHELL, pty_id) }
}

// ---- Terminal IPC message types ----

pub const MSG_TERMINAL_READY: u8 = 10;

/// Build a TerminalReady IPC message (sends window_id back)
pub fn msg_terminal_ready(window_id: u16) -> [u8; IPC_PAYLOAD_SIZE] {
    let mut msg = [0u8; IPC_PAYLOAD_SIZE];
    msg[0] = MSG_TERMINAL_READY;
    msg[1] = window_id as u8;
    msg[2] = (window_id >> 8) as u8;
    msg
}

// ---- Audio syscalls ----

/// Audio volume commands (for SYS_AUDIO_VOLUME)
pub const AUDIO_VOL_GET: usize = 0;
pub const AUDIO_VOL_SET: usize = 1;
pub const AUDIO_VOL_STOP: usize = 2;

/// Play a beep at the given frequency (Hz) and duration (ms).
pub fn audio_beep(freq: u32, duration_ms: u32) {
    unsafe {
        let _ = syscall2(SYS_AUDIO_BEEP, freq as usize, duration_ms as usize);
    }
}

/// Write PCM samples (16-bit signed, mono, 8kHz) to the audio output buffer.
/// Returns the number of bytes written.
pub fn audio_pcm_write(samples: &[u8]) -> usize {
    if samples.is_empty() {
        return 0;
    }
    unsafe { syscall2(SYS_AUDIO_PCM_WRITE, samples.as_ptr() as usize, samples.len()) }
}

/// Get the master audio volume (0-255).
pub fn audio_get_volume() -> u8 {
    unsafe { syscall2(SYS_AUDIO_VOLUME, AUDIO_VOL_GET, 0) as u8 }
}

/// Set the master audio volume (0-255).
pub fn audio_set_volume(vol: u8) {
    unsafe {
        let _ = syscall2(SYS_AUDIO_VOLUME, AUDIO_VOL_SET, vol as usize);
    }
}

/// Stop all audio playback (clear buffers, silence speaker).
pub fn audio_stop() {
    unsafe {
        let _ = syscall2(SYS_AUDIO_VOLUME, AUDIO_VOL_STOP, 0);
    }
}