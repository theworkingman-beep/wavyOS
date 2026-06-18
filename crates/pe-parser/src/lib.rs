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

/// A single data directory entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DataDirectory {
    pub rva: u32,
    pub size: u32,
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
    pub data_directory_offset: usize,
    pub num_data_directories: u32,
}

/// Import directory descriptor (one per imported DLL).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImportDescriptor {
    pub int_rva: u32,
    pub iat_rva: u32,
    pub name_rva: u32,
}

const MAX_IMPORT_DESCRIPTORS: usize = 32;
const MAX_IMPORT_THUNKS: usize = 64;

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

    let num_data_directories = if opt_offset + 0x6C + 4 <= data.len() {
        u32::from_le_bytes(data[opt_offset + 0x68..opt_offset + 0x6C].try_into().ok()?)
    } else {
        0
    };
    let data_directory_offset = opt_offset + 0x6C;

    Some(PeImage {
        machine,
        entry_point: image_base + entry_point,
        image_base,
        image_size,
        is_dll,
        num_sections,
        section_table_offset,
        data_directory_offset,
        num_data_directories,
    })
}

/// Parse a single data directory at `index` (0-based). Returns `None` if the
/// directory does not exist or is truncated.
pub fn data_directory(data: &[u8], image: &PeImage, index: usize) -> Option<DataDirectory> {
    if index as u32 >= image.num_data_directories {
        return None;
    }
    let offset = image.data_directory_offset + index * 8;
    if offset + 8 > data.len() {
        return None;
    }
    let rva = u32::from_le_bytes(data[offset..offset + 4].try_into().ok()?);
    let size = u32::from_le_bytes(data[offset + 4..offset + 8].try_into().ok()?);
    Some(DataDirectory { rva, size })
}

/// Parse the import directory and return up to `MAX_IMPORT_DESCRIPTORS`
/// entries. The returned array is terminated by the first `None` slot.
pub fn parse_import_directory(data: &[u8], image: &PeImage) -> [Option<ImportDescriptor>; MAX_IMPORT_DESCRIPTORS] {
    let mut out = [None; MAX_IMPORT_DESCRIPTORS];
    let Some(dir) = data_directory(data, image, 1) else {
        return out;
    };
    if dir.rva == 0 || dir.size == 0 {
        return out;
    }

    let dir_offset = rva_to_file_offset(data, image, dir.rva as u64).unwrap_or(0);
    for i in 0..MAX_IMPORT_DESCRIPTORS {
        let entry_offset = dir_offset + i * 20;
        if entry_offset + 20 > data.len() {
            break;
        }
        let int_rva = u32::from_le_bytes(data[entry_offset..entry_offset + 4].try_into().unwrap());
        let iat_rva = u32::from_le_bytes(data[entry_offset + 12..entry_offset + 16].try_into().unwrap());
        let name_rva = u32::from_le_bytes(data[entry_offset + 16..entry_offset + 20].try_into().unwrap());
        // The terminator has zero fields.
        if int_rva == 0 && iat_rva == 0 && name_rva == 0 {
            break;
        }
        out[i] = Some(ImportDescriptor {
            int_rva,
            iat_rva,
            name_rva,
        });
    }
    out
}

/// Parse import thunks (PE32+ IAT/INT entries) at `rva`. Each entry is an
/// 8-byte RVA or ordinal. The array terminates at the first zero entry.
pub fn parse_import_thunks(data: &[u8], image: &PeImage, rva: u32) -> [Option<u64>; MAX_IMPORT_THUNKS] {
    let mut out = [None; MAX_IMPORT_THUNKS];
    let Some(offset) = rva_to_file_offset(data, image, rva as u64) else {
        return out;
    };
    for i in 0..MAX_IMPORT_THUNKS {
        let entry_offset = offset + i * 8;
        if entry_offset + 8 > data.len() {
            break;
        }
        let value = u64::from_le_bytes(data[entry_offset..entry_offset + 8].try_into().unwrap());
        if value == 0 {
            break;
        }
        out[i] = Some(value);
    }
    out
}

/// Read a null-terminated ASCII string at `rva` from the raw file.
pub fn read_rva_string<'a>(data: &'a [u8], image: &PeImage, rva: u32) -> Option<&'a str> {
    let offset = rva_to_file_offset(data, image, rva as u64)?;
    let mut len = 0usize;
    while offset + len < data.len() && data[offset + len] != 0 {
        len += 1;
    }
    core::str::from_utf8(&data[offset..offset + len]).ok()
}

/// Convert a virtual address to a file offset using section headers.
fn rva_to_file_offset(data: &[u8], image: &PeImage, rva: u64) -> Option<usize> {
    for i in 0..image.num_sections as usize {
        let offset = image.section_table_offset + i * 40;
        let section = parse_section_header(data, offset)?;
        let start = section.virtual_address as u64;
        let end = start + section.raw_size as u64;
        if rva >= start && rva < end {
            return Some((rva - start + section.raw_offset as u64) as usize);
        }
    }
    None
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
        let mut data = vec![0u8; 0x2000];
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
        // NumberOfRvaAndSizes at offset 0x68 from optional header start
        data[0x58 + 0x68..0x58 + 0x6C].copy_from_slice(&16u32.to_le_bytes());
        // Data directories start at 0x58+0x6C = 0xC4. Leave all zero.
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

    /// Verify the kernel loader fixture parses and its single `.text` section
    /// is described correctly.
    #[test]
    fn parses_kernel_loader_fixture() {
        let data = include_bytes!("../../../kernel/src/win32/minimal_pe64.bin");
        let image = parse_pe(data).expect("valid fixture PE64");
        assert_eq!(image.machine, MachineType::Amd64);
        assert_eq!(image.image_base, 0x1_4000_0000);
        assert_eq!(image.entry_point, 0x1_4000_1000);
        assert_eq!(image.image_size, 0x2000);
        assert!(!image.is_dll);
        assert_eq!(image.num_sections, 1);

        let section = parse_section_header(data, image.section_table_offset).expect("text section");
        assert_eq!(section.name_str(), ".text");
        assert_eq!(section.virtual_address, 0x1000);
        assert_eq!(section.virtual_size, 0x1000);
        assert_eq!(section.raw_offset, 0x200);
        assert_eq!(section.raw_size, 512);
    }

    #[test]
    fn parses_data_directories() {
        let mut data = make_pe64();
        let image = parse_pe(&data).unwrap();
        assert_eq!(image.num_data_directories, 16);
        // By default all directories are zero/empty.
        for i in 0..16 {
            let dir = data_directory(&data, &image, i).expect("directory in bounds");
            assert_eq!(dir.rva, 0);
            assert_eq!(dir.size, 0);
        }

        // Set the import directory (index 1) to point to a descriptor at image
        // RVA 0x2000. Add a section mapping so the RVA resolves to a file offset.
        let import_rva = 0x2000u32;
        let import_size = 20u32;
        let dir_offset = image.data_directory_offset + 1 * 8;
        data[dir_offset..dir_offset + 4].copy_from_slice(&import_rva.to_le_bytes());
        data[dir_offset + 4..dir_offset + 8].copy_from_slice(&import_size.to_le_bytes());

        // Add a section mapping RVA 0x2000..0x3000 to file offset 0x300.
        let section_offset = image.section_table_offset;
        data[section_offset..section_offset + 8].copy_from_slice(b".idata\0\0");
        data[section_offset + 8..section_offset + 12].copy_from_slice(&0x1000u32.to_le_bytes()); // virtual size
        data[section_offset + 12..section_offset + 16].copy_from_slice(&import_rva.to_le_bytes()); // virtual address
        data[section_offset + 16..section_offset + 20].copy_from_slice(&0x1000u32.to_le_bytes()); // raw size
        data[section_offset + 20..section_offset + 24].copy_from_slice(&0x300u32.to_le_bytes()); // raw offset

        // Write one import descriptor at file offset 0x300.
        // INT=0, time=0, forwarder=0, IAT=0x2100, name=0x2200.
        let desc_offset = 0x300;
        let iat_rva = 0x2100u32;
        let name_rva = 0x2200u32;
        data[desc_offset + 12..desc_offset + 16].copy_from_slice(&iat_rva.to_le_bytes());
        data[desc_offset + 16..desc_offset + 20].copy_from_slice(&name_rva.to_le_bytes());

        // Write the DLL name at file offset 0x300 + 0x200 = 0x500.
        // name_rva 0x2200 maps to raw offset 0x500 in this section.
        let name_offset = 0x500;
        data[name_offset..name_offset + 12].copy_from_slice(b"kernel32.dll");

        let image = parse_pe(&data).unwrap();
        let dirs = parse_import_directory(&data, &image);
        assert!(dirs[0].is_some());
        let desc = dirs[0].unwrap();
        assert_eq!(desc.iat_rva, iat_rva);
        assert_eq!(desc.name_rva, name_rva);
        assert_eq!(read_rva_string(&data, &image, name_rva), Some("kernel32.dll"));
        assert!(dirs[1].is_none());
    }
}
