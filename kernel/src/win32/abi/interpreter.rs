//! Minimal x86/x86_64 instruction interpreter for binary translation.
//!
//! This is a baseline interpreter used when a guest architecture does not
//! match the host. It decodes the most common instructions directly from
//! the mapped PE image and updates the guest PC. Hot code can later be
//! promoted to the JIT.

use crate::win32::thread::{Register, Thread};
use crate::win32::nt::{self, SyscallNumber};

/// Read a contiguous slice from guest memory.
///
/// # Safety
/// `addr` must point to at least `len` valid readable bytes in the kernel address space.
unsafe fn read_guest_bytes(addr: u64, len: usize) -> &'static [u8] {
    core::slice::from_raw_parts(addr as *const u8, len)
}

/// Read a `u64` from guest memory.
unsafe fn read_u64(addr: u64) -> u64 {
    core::ptr::read_volatile(addr as *const u64)
}

/// Write a `u64` to guest memory.
unsafe fn write_u64(addr: u64, value: u64) {
    core::ptr::write_volatile(addr as *mut u64, value);
}

/// Run an x86_64 guest code stream starting at `entry` until it hits an
/// unsupported instruction.
///
/// # Safety
/// `entry` must point to valid mapped guest code in the kernel address space.
pub unsafe fn run_x86_64_loop(entry: u64) -> ! {
    crate::win32::scheduler::with_current_thread(|t| {
        t.user_rip = entry;
        t.write_reg(Register::Rsp, t.user_rsp);
    });

    loop {
        let npc = crate::win32::scheduler::with_current_thread(|t| {
            let pc = t.user_rip;
            const WINDOW: usize = 15;
            let code = read_guest_bytes(pc, WINDOW);
            execute_instruction(t, pc, code)
        });

        match npc {
            Some(Some(pc)) => {
                crate::win32::scheduler::with_current_thread(|t| {
                    t.user_rip = pc;
                });
            }
            Some(None) => {
                crate::logln!("win32/abi: interpreter stopped at {:#x}",
                    crate::win32::scheduler::with_current_thread(|t| t.user_rip).unwrap_or(0));
                crate::hlt();
            }
            None => {
                crate::logln!("win32/abi: no current thread in interpreter");
                crate::hlt();
            }
        }
    }
}

/// Execute the instruction at `pc` and return the next guest PC, or `None`
/// if the instruction is unsupported.
fn execute_instruction(t: &mut Thread, pc: u64, code: &[u8]) -> Option<u64> {
    if code.is_empty() {
        return None;
    }

    let mut pos = 0;
    let mut rex_w = false;
    let mut rex_r = false;
    let mut rex_x = false;
    let mut rex_b = false;

    // Consume a single REX prefix (0x40-0x4F) if present.
    if (code[pos] & 0xF0) == 0x40 {
        let rex = code[pos];
        rex_w = (rex & 0x08) != 0;
        rex_r = (rex & 0x04) != 0;
        rex_x = (rex & 0x02) != 0;
        rex_b = (rex & 0x01) != 0;
        pos += 1;
        if pos >= code.len() {
            return None;
        }
    }

    let opcode = code[pos];
    pos += 1;

    match opcode {
        // NOP
        0x90 => Some(pc.wrapping_add(pos as u64)),
        // RET near
        0xC3 => {
            let ra = unsafe { read_u64(t.read_reg(Register::Rsp)) };
            let rsp = t.read_reg(Register::Rsp).wrapping_add(8);
            t.write_reg(Register::Rsp, rsp);
            if ra == 0 {
                crate::logln!("win32/abi: interpreter thread returned from entry");
                None
            } else {
                Some(ra)
            }
        }
        // JMP rel8
        0xEB => {
            if pos >= code.len() {
                return None;
            }
            let offset = code[pos] as i8 as i64;
            Some(pc.wrapping_add(pos as u64 + 1).wrapping_add(offset as u64))
        }
        // JMP rel32
        0xE9 => {
            if pos + 4 > code.len() {
                return None;
            }
            let offset = i32::from_le_bytes([code[pos], code[pos + 1], code[pos + 2], code[pos + 3]]) as i64;
            Some(pc.wrapping_add(pos as u64 + 4).wrapping_add(offset as u64))
        }
        // CALL rel32
        0xE8 => {
            if pos + 4 > code.len() {
                return None;
            }
            let offset = i32::from_le_bytes([code[pos], code[pos + 1], code[pos + 2], code[pos + 3]]) as i64;
            let return_addr = pc.wrapping_add(pos as u64 + 4);
            let rsp = t.read_reg(Register::Rsp).wrapping_sub(8);
            t.write_reg(Register::Rsp, rsp);
            unsafe { write_u64(rsp, return_addr) };
            Some(pc.wrapping_add(pos as u64 + 4).wrapping_add(offset as u64))
        }
        // MOV r32/64, imm32/64
        0xB8..=0xBF => {
            let reg = gpr(opcode & 0x07, rex_b);
            if rex_w {
                if pos + 8 > code.len() {
                    return None;
                }
                let imm = u64::from_le_bytes([
                    code[pos], code[pos + 1], code[pos + 2], code[pos + 3],
                    code[pos + 4], code[pos + 5], code[pos + 6], code[pos + 7],
                ]);
                t.write_reg(reg, imm);
                Some(pc.wrapping_add(pos as u64 + 8))
            } else {
                if pos + 4 > code.len() {
                    return None;
                }
                let imm = u32::from_le_bytes([code[pos], code[pos + 1], code[pos + 2], code[pos + 3]]) as u64;
                t.write_reg(reg, imm);
                Some(pc.wrapping_add(pos as u64 + 4))
            }
        }
        // Group 11: MOV r/m64, imm32 (C7 /0 id)
        0xC7 => {
            if pos >= code.len() {
                return None;
            }
            let modrm = code[pos];
            pos += 1;
            let (mod_bits, reg_op, rm) = decode_modrm(modrm);
            if reg_op != 0 {
                return None; // not MOV /0
            }
            let (new_pos, addr_or_reg) = decode_modrm_operand(t, pc, pos, code, mod_bits, rm, rex_b, rex_x, rex_r)?;
            if pos + 4 > code.len() {
                return None;
            }
            let imm = i32::from_le_bytes([code[new_pos], code[new_pos + 1], code[new_pos + 2], code[new_pos + 3]]) as u64;
            let len = new_pos + 4;
            match addr_or_reg {
                Operand::Reg(r) => t.write_reg(r, imm),
                Operand::Addr(a) => unsafe { write_u64(a, imm) },
            }
            Some(pc.wrapping_add(len as u64))
        }
        // XOR r/m64, r64 (31 /r): destination is r/m, source is reg.
        0x31 => {
            if pos >= code.len() {
                return None;
            }
            let modrm = code[pos];
            pos += 1;
            let (mod_bits, reg_field, rm) = decode_modrm(modrm);
            let src = gpr(reg_field, rex_r);
            let (new_pos, dst) = decode_modrm_operand(t, pc, pos, code, mod_bits, rm, rex_b, rex_x, rex_r)?;
            let len = new_pos;
            match dst {
                Operand::Reg(r) => {
                    let val = t.read_reg(r) ^ t.read_reg(src);
                    t.write_reg(r, val);
                }
                Operand::Addr(a) => {
                    let val = unsafe { read_u64(a) } ^ t.read_reg(src);
                    unsafe { write_u64(a, val) };
                }
            }
            Some(pc.wrapping_add(len as u64))
        }
        // XOR r64, r/m64 (33 /r): destination is reg, source is r/m.
        0x33 => {
            if pos >= code.len() {
                return None;
            }
            let modrm = code[pos];
            pos += 1;
            let (mod_bits, reg_field, rm) = decode_modrm(modrm);
            let dst = gpr(reg_field, rex_r);
            let (new_pos, src) = decode_modrm_operand(t, pc, pos, code, mod_bits, rm, rex_b, rex_x, rex_r)?;
            let len = new_pos;
            match src {
                Operand::Reg(r) => {
                    let val = t.read_reg(dst) ^ t.read_reg(r);
                    t.write_reg(dst, val);
                }
                Operand::Addr(a) => {
                    let val = t.read_reg(dst) ^ unsafe { read_u64(a) };
                    t.write_reg(dst, val);
                }
            }
            Some(pc.wrapping_add(len as u64))
        }
        // LEA r64, m (8D /r)
        0x8D => {
            if pos >= code.len() {
                return None;
            }
            let modrm = code[pos];
            pos += 1;
            let (mod_bits, reg_field, rm) = decode_modrm(modrm);
            let dest = gpr(reg_field, rex_r);
            // We only support RIP-relative addressing (mod=00, r/m=101).
            if mod_bits != 0 || rm != 0b101 {
                return None;
            }
            if pos + 4 > code.len() {
                return None;
            }
            let disp = i32::from_le_bytes([code[pos], code[pos + 1], code[pos + 2], code[pos + 3]]) as i64;
            let len = pos + 4;
            let target = pc.wrapping_add(len as u64).wrapping_add(disp as u64);
            t.write_reg(dest, target);
            Some(pc.wrapping_add(len as u64))
        }
        // Two-byte opcode escape.
        0x0F => {
            if pos >= code.len() {
                return None;
            }
            let sub = code[pos];
            pos += 1;
            match sub {
                // SYSCALL (0F 05)
                0x05 => dispatch_syscall(t, pc.wrapping_add(pos as u64)),
                _ => None,
            }
        }
        _ => None,
    }
}

#[derive(Clone, Copy)]
enum Operand {
    Reg(Register),
    Addr(u64),
}

/// Map a 3-bit register field (with optional REX extension bit) to a `Register`.
fn gpr(base: u8, rex_ext: bool) -> Register {
    let idx = (base & 0x07) as usize;
    let idx = if rex_ext { idx + 8 } else { idx };
    // SAFETY: idx is always 0..=15, matching the `Register` discriminant values.
    unsafe { core::mem::transmute(idx) }
}

/// Decode the ModR/M byte into (mod, reg, r/m).
fn decode_modrm(byte: u8) -> (u8, u8, u8) {
    let mod_bits = byte >> 6;
    let reg = (byte >> 3) & 0x07;
    let rm = byte & 0x07;
    (mod_bits, reg, rm)
}

/// Resolve a ModR/M operand. Returns the new code position after any
/// displacement/SIB bytes and the operand itself.
fn decode_modrm_operand(
    t: &Thread,
    pc: u64,
    pos: usize,
    code: &[u8],
    mod_bits: u8,
    rm: u8,
    rex_b: bool,
    _rex_x: bool,
    _rex_r: bool,
) -> Option<(usize, Operand)> {
    match mod_bits {
        // Register direct.
        0b11 => Some((pos, Operand::Reg(gpr(rm, rex_b)))),
        // Memory, no SIB, no displacement except RSP (r/m=100) and RIP-relative (r/m=101).
        0b00 => {
            if rm == 0b101 {
                // RIP-relative with 32-bit displacement.
                if pos + 4 > code.len() {
                    return None;
                }
                let disp = i32::from_le_bytes([code[pos], code[pos + 1], code[pos + 2], code[pos + 3]]) as i64;
                let len = pos + 4;
                let target = pc.wrapping_add(len as u64).wrapping_add(disp as u64);
                Some((len, Operand::Addr(target)))
            } else if rm == 0b100 {
                // SIB byte follows; unsupported in baseline interpreter.
                None
            } else {
                // [reg]
                if rm == 0b101 {
                    return None;
                }
                Some((pos, Operand::Addr(t.read_reg(gpr(rm, rex_b)))))
            }
        }
        0b01 => {
            // [reg + disp8]; unsupported for now.
            None
        }
        0b10 => {
            // [reg + disp32]; unsupported for now.
            None
        }
        _ => None,
    }
}

/// Build the NT syscall argument array from the current register state,
/// dispatch the syscall, store the status in RAX, and return the PC after
/// the SYSCALL instruction.
fn dispatch_syscall(t: &mut Thread, return_pc: u64) -> Option<u64> {
    let number = t.read_reg(Register::Rax) as usize;
    let mut args = [0usize; 16];
    args[0] = t.read_reg(Register::Rdi) as usize;
    args[1] = t.read_reg(Register::Rsi) as usize;
    args[2] = t.read_reg(Register::Rdx) as usize;
    args[3] = t.read_reg(Register::R10) as usize;
    args[4] = t.read_reg(Register::R8) as usize;
    args[5] = t.read_reg(Register::R9) as usize;

    let status = nt::dispatch(SyscallNumber::from(number), args) as u64;
    t.write_reg(Register::Rax, status);
    Some(return_pc)
}
