//! x86_64 SYSCALL/SYSRET setup for user-mode NT system calls.
//!
//! We configure the LSTAR MSR with a handler that captures the user-mode
//! register state and dispatches the NT syscall. This is the kernel side of the
//! trap; user-mode code executes `syscall` with the syscall number in RAX.

use x86_64::registers::model_specific::{Efer, EferFlags, LStar, Star};
use x86_64::registers::segmentation::SegmentSelector;
use x86_64::PrivilegeLevel;

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

/// Naked entry point for SYSCALL.
///
/// On entry:
///   RAX = syscall number
///   RCX = user-mode RIP
///   R11 = user-mode RFLAGS
///   RDI, RSI, RDX, R10, R8, R9 = arguments
///
/// We save the remaining registers, build a 16-element argument array on the
/// stack, call the NT dispatch, restore registers, and SYSRET back to RCX/R11.
#[cfg(feature = "arch_x86_64")]
#[unsafe(naked)]
pub unsafe extern "C" fn syscall_entry() {
    core::arch::naked_asm!(
        "push rbp",
        "push rbx",
        "push r12",
        "push r13",
        "push r14",
        "push r15",
        // Build args[0..6] from ABI registers.
        "mov r12, rdi",          // r12 = arg0
        "mov r13, rsi",          // r13 = arg1
        // rdx = arg2, r10 = arg3, r8 = arg4, r9 = arg5
        // Allocate stack space for 16 qwords.
        "sub rsp, 128",
        "mov [rsp + 0*8], r12",
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
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop rbx",
        "pop rbp",
        "sysretq",
        dispatch = sym crate::win32::nt::dispatch,
    );
}
