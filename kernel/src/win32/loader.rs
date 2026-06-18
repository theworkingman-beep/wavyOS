//! PE/COFF loader for Windows executables.
//!
//! Reuses the architecture-independent `pe-parser` crate and adds process
//! address-space loading support.

pub use pe_parser::{parse_pe, parse_section_header, MachineType, PeImage, SectionHeader};

use super::process::Process;

/// Load a parsed PE image into a process address space.
pub fn load_into_process(_image: &PeImage, _process: &mut Process, _data: &[u8]) {
    // TODO: map sections, relocate, resolve imports, set entry point.
    for i in 0.._image.num_sections as usize {
        let offset = _image.section_table_offset + i * 40;
        if let Some(_section) = parse_section_header(_data, offset) {
            map_section(_process, _image, &_section, _data);
        }
    }
}

fn map_section(_process: &mut Process, _image: &PeImage, _section: &SectionHeader, _data: &[u8]) {
    // Placeholder: allocate frames and copy raw section data.
    let _ = (_process, _image, _section, _data);
}

/// Determine if a guest PE architecture can run natively on the host.
pub fn requires_translation(guest: MachineType) -> bool {
    let host = host_machine();
    guest != host
}

fn host_machine() -> MachineType {
    #[cfg(target_arch = "x86_64")]
    {
        MachineType::Amd64
    }
    #[cfg(target_arch = "aarch64")]
    {
        MachineType::Arm64
    }
    #[cfg(target_arch = "x86")]
    {
        MachineType::I386
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64", target_arch = "x86")))]
    {
        MachineType::Amd64
    }
}
