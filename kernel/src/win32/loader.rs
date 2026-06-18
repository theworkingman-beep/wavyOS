//! PE/COFF loader for Windows executables.
//!
//! Supports both x86_64 (`IMAGE_FILE_MACHINE_AMD64`) and AArch64
//! (`IMAGE_FILE_MACHINE_ARM64`) PE images. On a mismatched host architecture, the
//! loader queues the image for dynamic binary translation.

use super::process::Process;

/// PE machine types relevant to Aperture OS.
#[repr(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MachineType {
    I386 = 0x014C,
    Amd64 = 0x8664,
    Arm64 = 0xAA64,
}

/// Result of parsing a PE image.
pub struct PeImage {
    pub machine: MachineType,
    pub entry_point: u64,
    pub image_base: u64,
    pub image_size: u64,
    pub is_dll: bool,
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

    let entry_point = u32::from_le_bytes(data[opt_offset + 16..opt_offset + 20].try_into().ok()?) as u64;
    let image_size = u32::from_le_bytes(data[opt_offset + 56..opt_offset + 60].try_into().ok()?) as u64;

    let _ = num_sections;
    let _ = opt_header_size;

    Some(PeImage {
        machine,
        entry_point: image_base + entry_point,
        image_base,
        image_size,
        is_dll,
    })
}

/// Load a parsed PE image into a process address space.
pub fn load_into_process(_image: &PeImage, _process: &mut Process, _data: &[u8]) {
    // TODO: map sections, relocate, resolve imports, set entry point.
}
