//! Minimal no_std AArch64 instruction decoder.
//!
//! AArch64 instructions are fixed at 4 bytes. This crate decodes the small
//! subset needed by the Aperture OS binary-translation layer:
//! NOP, RET, SVC, BL, MOVZ, and ADRP. Everything else is reported as
//! `Instruction::Unsupported`.

#![no_std]

#[cfg(test)]
extern crate std;

/// Decoded AArch64 instruction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Instruction {
    /// No-operation.
    Nop,
    /// Return to the link register.
    Ret,
    /// Supervisor call.
    Svc { imm: u16 },
    /// Branch with link.
    Bl { offset: i64 },
    /// Move 16-bit immediate into a register (optionally shifted).
    Movz { reg: u8, imm: u16, shift: u8 },
    /// Compute PC-relative page address.
    Adrp { reg: u8, offset: i64 },
    /// Any instruction we don't decode yet.
    Unsupported,
}

/// Decode a single 32-bit AArch64 opcode.
pub fn decode(opcode: u32) -> Instruction {
    // NOP: 0xD5_03_20_1F
    if opcode == 0xD5_03_20_1F {
        return Instruction::Nop;
    }

    // RET: bits 31..10 == 0b1101_0110_0100_1111_0000_11
    if (opcode & 0xFFFF_FC00) == 0xD65F_0000 {
        return Instruction::Ret;
    }

    // SVC: bits 31..5 == 0b1101_0100_0000_0000_0000_0000_001 (D400000?)
    // Encoding: 1101_0100_0000_0000_0000_0000_0001_????? -> 0xD4000001 base.
    if (opcode & 0xFFE0_0000) == 0xD400_0000 {
        let imm = ((opcode >> 5) & 0xFFFF) as u16;
        return Instruction::Svc { imm };
    }

    // BL: 0x9400_0000 | (imm26 << 0), sign bit at bit 25.
    if (opcode & 0xFC00_0000) == 0x9400_0000 {
        let imm26 = (opcode & 0x03FF_FFFF) as i32;
        let signed = (imm26 << 6) >> 6; // sign-extend 26-bit value to 32-bit.
        let offset = (signed as i64) * 4;
        return Instruction::Bl { offset };
    }

    // MOVZ (64-bit): 0xD2800000 | (shift << 21) | (imm16 << 5) | reg
    if (opcode & 0x7F80_0000) == 0x5280_0000 {
        let reg = (opcode & 0x1F) as u8;
        let imm = ((opcode >> 5) & 0xFFFF) as u16;
        let shift = ((opcode >> 21) & 0x3) as u8 * 16;
        return Instruction::Movz { reg, imm, shift };
    }

    // ADRP: 0x90000000 with sign bits etc.
    if (opcode & 0x9F00_0000) == 0x9000_0000 {
        let reg = (opcode & 0x1F) as u8;
        // imm: bits [23:5] and [30:29] -> 21-bit signed, scaled by 4096.
        let immlo = ((opcode >> 29) & 0x3) as i64;
        let immhi = ((opcode >> 5) & 0x7FFFF) as i64;
        let imm21 = (immhi << 2) | immlo;
        // Sign-extend 21-bit value.
        let imm21_signed = if (imm21 & (1 << 20)) != 0 {
            imm21 | !((1 << 21) - 1)
        } else {
            imm21
        };
        let offset = imm21_signed * 4096;
        return Instruction::Adrp { reg, offset };
    }

    Instruction::Unsupported
}

/// Decode the instruction at `pc` from `code` and return the next PC.
/// Returns `None` when the instruction is unsupported or the opcode is
/// truncated.
pub fn step(pc: u64, code: &[u8]) -> Option<u64> {
    let start = pc as usize;
    if start + 4 > code.len() {
        return None;
    }
    let opcode = u32::from_le_bytes([
        code[start],
        code[start + 1],
        code[start + 2],
        code[start + 3],
    ]);
    let instr = decode(opcode);
    match instr {
        Instruction::Nop | Instruction::Ret => Some(pc.wrapping_add(4)),
        Instruction::Bl { offset } => Some(pc.wrapping_add(4).wrapping_add(offset as u64)),
        Instruction::Svc { .. } => Some(pc.wrapping_add(4)),
        Instruction::Movz { .. } => Some(pc.wrapping_add(4)),
        Instruction::Adrp { .. } => Some(pc.wrapping_add(4)),
        Instruction::Unsupported => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_nop() {
        assert_eq!(decode(0xD5_03_20_1F), Instruction::Nop);
    }

    #[test]
    fn decode_ret() {
        assert_eq!(decode(0xD6_5F_03_C0), Instruction::Ret);
    }

    #[test]
    fn decode_svc() {
        assert_eq!(decode(0xD4_00_00_01), Instruction::Svc { imm: 0 });
        assert_eq!(decode(0xD4_00_04_21), Instruction::Svc { imm: 0x21 });
    }

    #[test]
    fn decode_bl() {
        // BL with zero offset -> next instruction.
        assert_eq!(decode(0x94_00_00_00), Instruction::Bl { offset: 0 });
        // BL with offset = 1 -> branch forward 4 bytes.
        assert_eq!(decode(0x94_00_00_01), Instruction::Bl { offset: 4 });
        // BL with negative offset: 0x97FFFFFE encodes imm26 = 0x3FFFFFE -> -2.
        assert_eq!(decode(0x97_FF_FF_FE), Instruction::Bl { offset: -8 });
    }

    #[test]
    fn decode_movz() {
        // MOVZ X0, #0x1234
        assert_eq!(
            decode(0xD2_82_46_80),
            Instruction::Movz {
                reg: 0,
                imm: 0x1234,
                shift: 0,
            }
        );
        // MOVZ X5, #0xABCD, LSL #16
        assert_eq!(
            decode(0xD2_B5_79_A5),
            Instruction::Movz {
                reg: 5,
                imm: 0xABCD,
                shift: 16,
            }
        );
    }

    #[test]
    fn decode_adrp() {
        // ADRP X0, #0 (all zero immediate)
        assert_eq!(
            decode(0x90_00_00_00),
            Instruction::Adrp { reg: 0, offset: 0 }
        );
    }

    #[test]
    fn step_advances_pc() {
        // AArch64 opcodes are little-endian in memory.
        let code = [0x1F, 0x20, 0x03, 0xD5, 0xC0, 0x03, 0x5F, 0xD6];
        assert_eq!(step(0, &code), Some(4));
        assert_eq!(step(4, &code), Some(8));
    }
}
