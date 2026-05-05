# x86_64 syscall entry (called via `syscall` instruction from user space)
# Calling convention:
#   RAX = syscall number
#   RDI, RSI, RDX, R10, R8, R9 = arguments
#   RCX = return RIP (saved by syscall), R11 = return RFLAGS (saved by syscall)
# On entry: RAX=sysnum, RDI=a1, RSI=a2, RDX=a3, R10=a4, R8=a5, R9=a6

.global syscall_entry
syscall_entry:
    # Save user return info (already in RCX and R11 per syscall spec)
    # RCX = user RIP, R11 = user RFLAGS

    # We're already on the kernel stack (syscall doesn't switch stacks)
    # Save callee-saved registers we'll use
    push rbp
    mov rbp, rsp

    # Call the Rust dispatch function
    # Arguments: sysnum (RAX), a1 (RDI), a2 (RSI), a3 (RDX), a4 (R10), a5 (R8), a6 (R9)
    mov rdi, rax        # arg1: syscall number (RAX → RDI)
    # RSI already has a2
    # RDX already has a3
    # R10 already has a4
    # R8 already has a5
    # R9 already has a6
    call syscall_dispatch

    # RAX now has the return value from dispatch
    # Restore and return to user
    mov rsp, rbp
    pop rbp
    # RCX still has user RIP, R11 still has user RFLAGS (saved by syscall instruction)
    sysretq

# Rust function: syscall_dispatch(sysnum, a1, a2, a3, a4, a5, a6) -> usize
# This is implemented in Rust as: crate::syscalls::dispatch(sysnum, a1, a2, a3, a4, a5, a6)
# We need to make it visible with #[no_mangle]
.global syscall_dispatch
syscall_dispatch:
    jmp crate::syscalls::dispatch
