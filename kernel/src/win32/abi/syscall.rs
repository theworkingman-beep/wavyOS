//! User-mode NT system call helper for native x86_64 Windows binaries.
//!
//! This is the kernel-side helper that user-mode code would call. It places
//! the syscall number in RAX and executes the `syscall` instruction. For the
//! Aperture OS bring-up this is compiled into the kernel so that the synthetic
//! PE fixture can exercise the dispatch path without a real user-mode ring.

/// Invoke an NT system call with up to four arguments.
///
/// # Safety
/// Arguments must match the real NT syscall ABI for `number`.
#[cfg(all(feature = "arch_x86_64", target_arch = "x86_64"))]
pub unsafe fn syscall_4(number: usize, arg1: usize, arg2: usize, arg3: usize, arg4: usize) -> usize {
    let status: usize;
    core::arch::asm!(
        "mov r10, rdx",
        "syscall",
        inout("rax") number => status,
        in("rdi") arg1,
        in("rsi") arg2,
        in("rdx") arg3,
        in("r8") arg4,
        out("r10") _,
        out("rcx") _,
        out("r11") _,
        options(nostack),
    );
    status
}
