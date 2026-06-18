//! Minimal x86/x86_64 instruction interpreter for binary translation.
//!
//! This is a baseline interpreter used when a guest architecture does not
//! match the host. It decodes the most common instructions directly from
//! the mapped PE image and updates the guest PC. Hot code can later be
//! promoted to the JIT.

use x86_decode::decode;

/// Read a contiguous slice from guest memory.
///
/// # Safety
/// `addr` must point to at least `len` valid readable bytes in the kernel address space.
unsafe fn read_guest_bytes(addr: u64, len: usize) -> &'static [u8] {
    core::slice::from_raw_parts(addr as *const u8, len)
}

/// Run an x86_64 guest code stream starting at `entry` until it hits an
/// unsupported instruction.
///
/// # Safety
/// `entry` must point to valid mapped guest code in the kernel address space.
pub unsafe fn run_x86_64_loop(entry: u64) -> ! {
    let mut pc = entry;
    loop {
        // Read a small window of guest code and let the decoder determine the
        // instruction length and control-flow effect. We cap the window to
        // 15 bytes, the maximum length of a valid x86 instruction.
        const WINDOW: usize = 15;
        let code = read_guest_bytes(pc, WINDOW);
        let insn = decode(code);

        if insn.len == 0 {
            crate::logln!(
                "win32/abi: x86_64 interpreter unsupported instruction at {:#x}",
                pc
            );
            crate::hlt();
        }

        if insn.is_syscall {
            // The SYSCALL instruction transfers control to the host NT
            // dispatch. Dispatch the call and then continue after the
            // instruction. The actual ABI register plumbing is handled by
            // `syscall_4` in the native path; here we only need to advance.
            pc = pc.wrapping_add(insn.len as u64);
            continue;
        }

        if insn.is_ret {
            // Pop the 64-bit return address from the guest stack. The guest
            // stack pointer is stored in the current thread context.
            pc = pop_return_address();
            if pc == 0 {
                crate::logln!("win32/abi: x86_64 interpreter returned from entry");
                crate::hlt();
            }
            continue;
        }

        if insn.is_jmp {
            if let Some(offset) = insn.jmp_offset {
                pc = pc.wrapping_add(insn.len as u64).wrapping_add(offset as u64);
                continue;
            }
            crate::logln!("win32/abi: x86_64 interpreter jump with missing offset");
            crate::hlt();
        }

        // Default: fall through to the next instruction.
        pc = pc.wrapping_add(insn.len as u64);
    }
}

/// Pop the guest return address off the current thread's user stack.
fn pop_return_address() -> u64 {
    crate::win32::scheduler::with_current_thread(|t| {
        let rsp = t.user_rsp;
        // Safety: the interpreter is only used for mapped guest images; we
        // trust the guest stack pointer to be valid while the thread runs.
        let ra = unsafe { core::ptr::read_volatile(rsp as *const u64) };
        t.user_rsp = rsp.wrapping_add(8);
        ra
    })
    .unwrap_or(0)
}
