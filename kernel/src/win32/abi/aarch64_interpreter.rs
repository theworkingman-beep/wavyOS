//! AArch64 instruction interpreter for running ARM64 Windows PEs on x86_64 hosts.
//!
//! This is the ARM→x86 half of the architecture translation layer. It reuses
//! the `aarch64-decode` crate to decode guest instructions and maintains an
//! AArch64 register file in the current thread.

use crate::win32::scheduler;
use aarch64_decode::{decode, Instruction};

/// Minimal self-test: decode a few AArch64 opcodes and log the results.
/// This is exercised on x86_64 builds to verify the decoder is linked and
/// the ARM→x86 translation layer is wired into the kernel.
pub fn self_test() {
    let opcodes = [
        0xD5_03_20_1Fu32, // NOP
        0xD6_5F_03_C0,    // RET
        0xD4_00_00_01,    // SVC #0
        0x94_00_00_00,    // BL .+0
    ];
    for opcode in opcodes {
        let instr = decode(opcode);
        crate::logln!("aarch64-interpreter: decoded {:#010x} -> {:?}", opcode, instr);
    }
}

/// Run an AArch64 guest code stream starting at the current thread's entry
/// point. This is a skeleton: it decodes and logs instructions but does not
/// yet emulate register state or syscalls.
pub unsafe fn run_aarch64_loop(entry: u64) -> ! {
    crate::logln!("aarch64-interpreter: entering at {:#x}", entry);

    scheduler::with_current_thread(|t| {
        t.user_rip = entry;
    });

    loop {
        let pc = scheduler::with_current_thread(|t| t.user_rip).unwrap_or(0);
        // Read up to one AArch64 instruction (4 bytes) from guest memory.
        let code = core::slice::from_raw_parts(pc as *const u8, 4);
        let opcode = u32::from_le_bytes([code[0], code[1], code[2], code[3]]);
        let instr = decode(opcode);

        crate::logln!("aarch64-interpreter: {:#x}: {:#010x} -> {:?}", pc, opcode, instr);

        match instr {
            Instruction::Nop => {
                scheduler::with_current_thread(|t| t.user_rip = pc.wrapping_add(4));
            }
            Instruction::Ret => {
                crate::logln!("aarch64-interpreter: guest returned");
                crate::hlt();
            }
            Instruction::Svc { imm } => {
                crate::logln!("aarch64-interpreter: SVC #{}", imm);
                scheduler::with_current_thread(|t| t.user_rip = pc.wrapping_add(4));
            }
            _ => {
                crate::logln!("aarch64-interpreter: unsupported instruction at {:#x}", pc);
                crate::hlt();
            }
        }
    }
}
