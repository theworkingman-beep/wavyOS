//! Minimal `no_std` x86/x86_64 instruction-length decoder.
//!
//! This crate is used by the Aperture OS kernel to step through guest code in
//! the interpreter and baseline JIT. It intentionally avoids tables that would
//! bloat the kernel; only the most common instruction encodings are supported.

#![no_std]

#[cfg(test)]
extern crate std;

/// Result of decoding a single x86/x86_64 instruction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DecodedInsn {
    /// Total length of the instruction in bytes.
    pub len: usize,
    /// True if the instruction is a RET (near).
    pub is_ret: bool,
    /// True if the instruction is a relative JMP.
    pub is_jmp: bool,
    /// True if the instruction is a NOP.
    pub is_nop: bool,
    /// True if the instruction is a SYSCALL.
    pub is_syscall: bool,
    /// Relative jump offset, if `is_jmp` is true and the offset could be read.
    pub jmp_offset: Option<i64>,
}

impl DecodedInsn {
    pub const fn invalid() -> Self {
        Self {
            len: 0,
            is_ret: false,
            is_jmp: false,
            is_nop: false,
            is_syscall: false,
            jmp_offset: None,
        }
    }
}

/// Decode the instruction at the start of `code`.
///
/// Returns `DecodedInsn::invalid()` with `len == 0` if the encoding is not
/// recognized or the buffer is too short.
pub fn decode(code: &[u8]) -> DecodedInsn {
    if code.is_empty() {
        return DecodedInsn::invalid();
    }

    let mut pos = 0usize;

    // Skip legacy prefixes.
    while pos < code.len() {
        match code[pos] {
            0x66 | 0x67 | 0x2E | 0x3E | 0x26 | 0x64 | 0x65 | 0x36 | 0xF0 | 0xF2 | 0xF3 => {
                pos += 1;
            }
            _ => break,
        }
    }

    if pos >= code.len() {
        return DecodedInsn::invalid();
    }

    let opcode = code[pos];
    pos += 1;

    match opcode {
        // NOP (XCHG rAX, rAX in 64-bit mode is effectively NOP)
        0x90 => DecodedInsn {
            len: pos,
            is_nop: true,
            ..DecodedInsn::invalid()
        },
        // RET near
        0xC3 => DecodedInsn {
            len: pos,
            is_ret: true,
            ..DecodedInsn::invalid()
        },
        // JMP rel8
        0xEB => {
            if pos >= code.len() {
                return DecodedInsn::invalid();
            }
            let offset = code[pos] as i8 as i64;
            DecodedInsn {
                len: pos + 1,
                is_jmp: true,
                jmp_offset: Some(offset),
                ..DecodedInsn::invalid()
            }
        }
        // JMP rel16/32: opcode E9 + disp16/32. In 64-bit mode the operand is
        // sign-extended to 64 bits. We assume 32-bit displacement.
        0xE9 => {
            if pos + 4 > code.len() {
                return DecodedInsn::invalid();
            }
            let offset = i32::from_le_bytes([
                code[pos],
                code[pos + 1],
                code[pos + 2],
                code[pos + 3],
            ]) as i64;
            DecodedInsn {
                len: pos + 4,
                is_jmp: true,
                jmp_offset: Some(offset),
                ..DecodedInsn::invalid()
            }
        }
        // Two-byte opcode escape.
        0x0F => {
            if pos >= code.len() {
                return DecodedInsn::invalid();
            }
            let sub = code[pos];
            pos += 1;
            match sub {
                // SYSCALL (0F 05)
                0x05 => DecodedInsn {
                    len: pos,
                    is_syscall: true,
                    ..DecodedInsn::invalid()
                },
                _ => DecodedInsn::invalid(),
            }
        }
        _ => DecodedInsn::invalid(),
    }
}

/// Advance `pc` by one instruction, returning the updated PC.
///
/// Returns `None` if the instruction could not be decoded.
pub fn step(pc: u64, code: &[u8]) -> Option<u64> {
    let insn = decode(code);
    if insn.len == 0 {
        return None;
    }
    Some(pc.wrapping_add(insn.len as u64))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_nop() {
        let insn = decode(&[0x90]);
        assert_eq!(insn.len, 1);
        assert!(insn.is_nop);
    }

    #[test]
    fn decode_ret() {
        let insn = decode(&[0xC3]);
        assert_eq!(insn.len, 1);
        assert!(insn.is_ret);
    }

    #[test]
    fn decode_jmp_rel8() {
        let insn = decode(&[0xEB, 0xFE]);
        assert_eq!(insn.len, 2);
        assert!(insn.is_jmp);
        assert_eq!(insn.jmp_offset, Some(-2));
    }

    #[test]
    fn decode_jmp_rel32() {
        let insn = decode(&[0xE9, 0x00, 0x01, 0x00, 0x00]);
        assert_eq!(insn.len, 5);
        assert!(insn.is_jmp);
        assert_eq!(insn.jmp_offset, Some(0x100));
    }

    #[test]
    fn decode_syscall() {
        let insn = decode(&[0x0F, 0x05]);
        assert_eq!(insn.len, 2);
        assert!(insn.is_syscall);
    }

    #[test]
    fn decode_prefix_then_nop() {
        let insn = decode(&[0x66, 0x90]);
        assert_eq!(insn.len, 2);
        assert!(insn.is_nop);
    }

    #[test]
    fn step_advances_pc() {
        assert_eq!(step(0x1000, &[0x90, 0xC3]), Some(0x1001));
    }
}
