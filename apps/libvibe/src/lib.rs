#![no_std]

pub const SYS_IPC_SEND: usize = 5;
pub const SYS_IPC_RECV: usize = 6;
pub const SYS_SHM_CREATE: usize = 7;
pub const SYS_SHM_MAP: usize = 8;
pub const SYS_FRAMEBUFFER_MAP: usize = 9;

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

pub fn ipc_send(target: usize, msg: &[u8; 64]) {
    unsafe { let _ = syscall3(SYS_IPC_SEND, target, msg.as_ptr() as usize, 0); }
}

pub fn ipc_recv() -> usize {
    unsafe { syscall3(SYS_IPC_RECV, 0, 0, 0) }
}

pub fn shm_create(size: usize) -> usize {
    unsafe { syscall3(SYS_SHM_CREATE, size, 0, 0) }
}

pub fn shm_map(id: usize) -> *mut u8 {
    unsafe { syscall3(SYS_SHM_MAP, id, 0, 0) as *mut u8 }
}
