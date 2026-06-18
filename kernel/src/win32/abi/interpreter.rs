//! Minimal x86/x86_64 instruction interpreter for binary translation.
//!
//! This is a baseline interpreter used when a guest architecture does not
//! match the host. It decodes the most common instructions directly from
//! the mapped PE image and updates the guest PC. Hot code can later be
//! promoted to the JIT.

/// Run an x86_64 guest code stream starting at `entry` until it hits an
/// unsupported instruction.
///
/// # Safety
/// `entry` must point to valid mapped guest code in the kernel address space.
pub unsafe fn run_x86_64_loop(entry: u64) -> ! {
    let mut pc = entry;
    loop {
        let opcode = core::ptr::read_volatile(pc as *const u8);
        match opcode {
            0x90 => {
                // NOP
                pc = pc.wrapping_add(1);
            }
            0xEB => {
                // JMP rel8
                let rel = core::ptr::read_volatile(pc.wrapping_add(1) as *const i8) as i64;
                pc = pc.wrapping_add(2).wrapping_add(rel as u64);
            }
            _ => {
                crate::logln!(
                    "win32/abi: x86_64 interpreter hit unsupported opcode {:#04x} at {:#x}",
                    opcode,
                    pc
                );
                crate::hlt();
            }
        }
    }
}
