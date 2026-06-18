//! x86_64 SYSCALL/SYSRET setup for user-mode NT system calls.
//!
//! We configure the LSTAR MSR with a handler that captures the user-mode
//! register state and dispatches the NT syscall. This is the kernel side of the
//! trap; user-mode code executes `syscall` with the syscall number in RAX.

use core::sync::atomic::{AtomicU64, Ordering};
use x86_64::registers::model_specific::{Efer, EferFlags, LStar, Star};
use x86_64::registers::segmentation::SegmentSelector;
use x86_64::PrivilegeLevel;

/// Top of the kernel stack to use when a SYSCALL arrives from ring-3.
///
/// The scheduler updates this to the current thread's kernel stack before
/// entering user mode. Single-core bring-up only; on SMP this must be per-CPU.
static SYSCALL_RSP: AtomicU64 = AtomicU64::new(0);

/// Set the kernel stack pointer that SYSCALL will use for this thread.
pub fn set_syscall_rsp(rsp: u64) {
    SYSCALL_RSP.store(rsp, Ordering::Relaxed);
}

/// Initialize the SYSCALL machinery.
///
/// # Safety
/// Must be called once from a valid x86_64 context with a working IDT and
/// supervisor stack. Enabling `syscall`/`sysret` without correct segment and
/// stack setup can crash the system.
pub unsafe fn init() {
    let syscall_cs = SegmentSelector::new(1, PrivilegeLevel::Ring0);
    let syscall_ss = SegmentSelector::new(2, PrivilegeLevel::Ring0);
    let sysret_cs = SegmentSelector::new(3, PrivilegeLevel::Ring3);
    let sysret_ss = SegmentSelector::new(4, PrivilegeLevel::Ring3);
    let _ = Star::write(sysret_cs, sysret_ss, syscall_cs, syscall_ss);
    LStar::write(x86_64::VirtAddr::new(syscall_entry as *const () as u64));
    Efer::update(|efer| {
        efer.insert(EferFlags::SYSTEM_CALL_EXTENSIONS);
    });
}

/// Return to ring-3 using SYSRET with the given RIP and RSP.
///
/// # Safety
/// Must only be called from kernel mode with valid ring-3 selectors and a
/// valid user stack. Does not return.
#[cfg(feature = "arch_x86_64")]
#[unsafe(naked)]
pub unsafe extern "C" fn sysret_to_user(rip: u64, rsp: u64) -> ! {
    core::arch::naked_asm!(
        "mov rsp, rsi",   // user RSP
        "mov rcx, rdi",   // user RIP -> RCX for sysret
        "mov r11, 0x202", // user RFLAGS (IF set)
        "sysretq",
    );
}

/// Naked entry point for SYSCALL.
///
/// On entry:
///   RAX = syscall number
///   RCX = user-mode RIP
///   R11 = user-mode RFLAGS
///   RDI, RSI, RDX, R10, R8, R9 = arguments
///
/// We save the remaining registers on the kernel stack pointed to by
/// SYSCALL_RSP, build a 16-element argument array, call the NT dispatch,
/// restore registers, and SYSRET back to RCX/R11.
#[cfg(feature = "arch_x86_64")]
#[unsafe(naked)]
pub unsafe extern "C" fn syscall_entry() {
    core::arch::naked_asm!(
        // Stash user RSP (RSP is still the user stack) in r12 so we can
        // restore it before SYSRET.
        "mov r12, rsp",
        // Switch to the kernel SYSCALL stack. The address of SYSCALL_RSP is a
        // static in the kernel image; load it with a RIP-relative LEA and then
        // read the stored value.
        "lea r13, [rip + {syscall_rsp} - 7]",
        "mov rsp, [r13]",
        "push rbp",
        "push rbx",
        "push r13",
        "push r14",
        "push r15",
        "push r12",          // user RSP
        "push rcx",          // user RIP (SYSCALL saved it in RCX)
        "push r11",          // user RFLAGS (SYSCALL saved it in R11)
        // Build args[0..6] from ABI registers before we overwrite them.
        "mov r13, rsi",      // r13 = arg1
        // rdi = arg0, rdx = arg2, r10 = arg3, r8 = arg4, r9 = arg5
        "sub rsp, 128",
        "mov [rsp + 0*8], rdi",
        "mov [rsp + 1*8], r13",
        "mov [rsp + 2*8], rdx",
        "mov [rsp + 3*8], r10",
        "mov [rsp + 4*8], r8",
        "mov [rsp + 5*8], r9",
        "mov qword ptr [rsp + 6*8], 0",
        "mov qword ptr [rsp + 7*8], 0",
        "mov qword ptr [rsp + 8*8], 0",
        "mov qword ptr [rsp + 9*8], 0",
        "mov qword ptr [rsp + 10*8], 0",
        "mov qword ptr [rsp + 11*8], 0",
        "mov qword ptr [rsp + 12*8], 0",
        "mov qword ptr [rsp + 13*8], 0",
        "mov qword ptr [rsp + 14*8], 0",
        "mov qword ptr [rsp + 15*8], 0",
        // First argument to dispatch: syscall number (RAX).
        "mov rdi, rax",
        // Second argument: pointer to argument array.
        "mov rsi, rsp",
        "call {dispatch}",
        "add rsp, 128",
        "pop r11",           // user RFLAGS
        "pop rcx",           // user RIP
        "pop r12",           // user RSP
        "pop r15",
        "pop r14",
        "pop r13",
        "pop rbx",
        "pop rbp",
        "mov rsp, r12",
        "sysretq",
        syscall_rsp = sym SYSCALL_RSP,
        dispatch = sym crate::win32::nt::dispatch,
    );
}
