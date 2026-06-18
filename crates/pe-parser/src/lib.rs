//! Minimal no_std PE/COFF parser.
//!
//! Supports x86 (`IMAGE_FILE_MACHINE_I386`), x86_64 (`IMAGE_FILE_MACHINE_AMD64`),
//! and AArch64 (`IMAGE_FILE_MACHINE_ARM64`) PE images. This crate is used both by
//! the Aperture OS kernel and by host-side tooling/tests.

#![no_std]

#[cfg(test)]
extern crate std;

/// PE machine types relevant to Aperture OS.
#[repr(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MachineType {
    I386 = 0x014C,
    Amd64 = 0x8664,
    Arm64 = 0xAA64,
}

/// Result of parsing a PE image header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PeImage {
    pub machine: MachineType,
    pub entry_point: u64,
    pub image_base: u64,
    pub image_size: u64,
    pub is_dll: bool,
    pub num_sections: u16,
    pub section_table_offset: usize,
}

/// Parse a PE image in memory. Returns `None` if the header is invalid.
pub fn parse_pe(data: &[u8]) -> Option<PeImage> {
    if data.len() < 0x40 {
        return None;
    }
    if &data[0..2] != b"MZ" {
        return None;
    }
    let pe_offset = u32::from_le_bytes([data[0x3C], data[0x3D], data[0x3E], data[0x3F]]) as usize;
    if pe_offset + 24 > data.len() || &data[pe_offset..pe_offset + 4] != b"PE\0\0" {
        return None;
    }

    let machine = u16::from_le_bytes([data[pe_offset + 4], data[pe_offset + 5]]);
    let num_sections = u16::from_le_bytes([data[pe_offset + 6], data[pe_offset + 7]]);
    let opt_header_size = u16::from_le_bytes([data[pe_offset + 20], data[pe_offset + 21]]) as usize;
    let characteristics = u16::from_le_bytes([data[pe_offset + 22], data[pe_offset + 23]]);

    let machine = match machine {
        0x8664 => MachineType::Amd64,
        0xAA64 => MachineType::Arm64,
        0x014C => MachineType::I386,
        _ => return None,
    };

    let is_dll = (characteristics & 0x2000) != 0;

    let opt_offset = pe_offset + 24;
    if opt_offset + 8 > data.len() {
        return None;
    }
    let magic = u16::from_le_bytes([data[opt_offset], data[opt_offset + 1]]);
    let image_base = if magic == 0x20B {
        // PE32+ optional header
        u64::from_le_bytes(data[opt_offset + 24..opt_offset + 32].try_into().ok()?)
    } else if magic == 0x10B {
        // PE32 optional header
        u32::from_le_bytes(data[opt_offset + 28..opt_offset + 32].try_into().ok()?) as u64
    } else {
        return None;
    };

    let entry_point =
        u32::from_le_bytes(data[opt_offset + 16..opt_offset + 20].try_into().ok()?) as u64;
    let image_size =
        u32::from_le_bytes(data[opt_offset + 56..opt_offset + 60].try_into().ok()?) as u64;

    let section_table_offset = opt_offset + opt_header_size;

    Some(PeImage {
        machine,
        entry_point: image_base + entry_point,
        image_base,
        image_size,
        is_dll,
        num_sections,
        section_table_offset,
    })
}

/// Parse a single section header at `offset`.
pub fn parse_section_header(data: &[u8], offset: usize) -> Option<SectionHeader> {
    if offset + 40 > data.len() {
        return None;
    }
    let mut name = [0u8; 8];
    name.copy_from_slice(&data[offset..offset + 8]);
    let virtual_size = u32::from_le_bytes(data[offset + 8..offset + 12].try_into().ok()?);
    let virtual_address = u32::from_le_bytes(data[offset + 12..offset + 16].try_into().ok()?);
    let raw_size = u32::from_le_bytes(data[offset + 16..offset + 20].try_into().ok()?);
    let raw_offset = u32::from_le_bytes(data[offset + 20..offset + 24].try_into().ok()?);

    Some(SectionHeader {
        name,
        virtual_size,
        virtual_address,
        raw_size,
        raw_offset,
    })
}

/// PE section header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SectionHeader {
    pub name: [u8; 8],
    pub virtual_size: u32,
    pub virtual_address: u32,
    pub raw_size: u32,
    pub raw_offset: u32,
}

impl SectionHeader {
    /// Return the section name as a `&str`, trimming trailing NUL bytes.
    pub fn name_str(&self) -> &str {
        let len = self.name.iter().position(|&b| b == 0).unwrap_or(8);
        core::str::from_utf8(&self.name[..len]).unwrap_or("")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::vec;

    /// Build a minimal valid PE32+ executable header.
    fn make_pe64() -> vec::Vec<u8> {
        let mut data = vec![0u8; 0x200];
        // DOS header
        data[0..2].copy_from_slice(b"MZ");
        data[0x3C..0x40].copy_from_slice(&0x40u32.to_le_bytes());
        // PE signature
        data[0x40..0x44].copy_from_slice(b"PE\0\0");
        // COFF header
        data[0x44..0x46].copy_from_slice(&0x8664u16.to_le_bytes()); // machine
        data[0x46..0x48].copy_from_slice(&1u16.to_le_bytes()); // sections
        data[0x54..0x56].copy_from_slice(&0xF0u16.to_le_bytes()); // optional header size
        // Optional header
        data[0x58..0x5A].copy_from_slice(&0x20Bu16.to_le_bytes()); // magic PE32+
        data[0x68..0x6C].copy_from_slice(&0x1000u32.to_le_bytes()); // entry point RVA
        data[0x70..0x74].copy_from_slice(&0x10000u32.to_le_bytes()); // image base low
        // Actually image base is 64-bit at offset 0x58+24 = 0x70
        data[0x70..0x78].copy_from_slice(&0x1_4000_0000u64.to_le_bytes()); // image base
        // image size at optional header offset 0x38 -> 0x58+0x38 = 0x90
        data[0x90..0x94].copy_from_slice(&0x2000u32.to_le_bytes());
        data
    }

    #[test]
    fn rejects_too_small() {
        assert!(parse_pe(&[0u8; 10]).is_none());
    }

    #[test]
    fn rejects_invalid_mz() {
        let mut data = vec![0u8; 0x100];
        data[0..2].copy_from_slice(b"XX");
        assert!(parse_pe(&data).is_none());
    }

    #[test]
    fn parses_minimal_pe64() {
        let image = parse_pe(&make_pe64()).expect("valid PE64");
        assert_eq!(image.machine, MachineType::Amd64);
        assert_eq!(image.image_base, 0x1_4000_0000);
        assert_eq!(image.entry_point, 0x1_4000_1000);
        assert_eq!(image.image_size, 0x2000);
        assert!(!image.is_dll);
        assert_eq!(image.num_sections, 1);
    }
}
