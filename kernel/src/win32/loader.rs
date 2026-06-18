//! PE/COFF loader for Windows executables.
//!
//! Reuses the architecture-independent `pe-parser` crate and adds process
//! address-space loading support.

pub use pe_parser::{parse_pe, parse_section_header, MachineType, PeImage, SectionHeader};

use super::{objects, process::Process};
use core::ptr;

/// A minimal hand-crafted x86_64 PE executable used for loader self-tests.
///
/// It contains a single `.text` section with a two-byte `jmp $` loop mapped
/// at RVA 0x1000. The image base is 0x1_4000_0000 and the total image size is
/// 0x2000 bytes.
pub static MINIMAL_PE64: &[u8] = include_bytes!("minimal_pe64.bin");

/// Load a parsed PE image into a process address space.
pub fn load_into_process(image: &PeImage, process: &mut Process, data: &[u8]) -> bool {
    let total_pages = (image.image_size as usize + 4095) / 4096;
    let Some(base) = allocate_contiguous(total_pages) else {
        return false;
    };

    process.image_base = base;
    process.image_size = image.image_size;

    // Zero the allocated region before copying sections.
    unsafe {
        ptr::write_bytes(base as *mut u8, 0, total_pages * 4096);
    }

    // Copy each section from raw file offset to its virtual address.
    for i in 0..image.num_sections as usize {
        let offset = image.section_table_offset + i * 40;
        let Some(section) = parse_section_header(data, offset) else {
            return false;
        };
        map_section(process, &section, data, base);
    }

    // Record the absolute entry point inside the mapped image.
    let entry_rva = image.entry_point.saturating_sub(image.image_base);
    process.entry_point = base + entry_rva;

    true
}

fn map_section(process: &Process, section: &SectionHeader, data: &[u8], base: u64) {
    let dest = base + section.virtual_address as u64;
    let raw_size = section.raw_size as usize;
    let virtual_size = section.virtual_size as usize;

    let copy_size = raw_size.min(virtual_size);
    let src_offset = section.raw_offset as usize;

    if src_offset + copy_size <= data.len() {
        unsafe {
            ptr::copy_nonoverlapping(
                data.as_ptr().add(src_offset),
                dest as *mut u8,
                copy_size,
            );
        }
    }

    // Zero the remainder (BSS-style uninitialized data).
    if copy_size < virtual_size {
        unsafe {
            ptr::write_bytes((dest + copy_size as u64) as *mut u8, 0, virtual_size - copy_size);
        }
    }

    let _ = process; // Process metadata will be used for per-section permissions later.
}

fn allocate_contiguous(pages: usize) -> Option<u64> {
    if pages == 0 {
        return None;
    }
    // Allocate individual frames and verify they are contiguous.
    let first = crate::mm::frame_allocator::allocate()?;
    let mut last = first;
    for _ in 1..pages {
        let frame = crate::mm::frame_allocator::allocate()?;
        if frame != last + 4096 {
            // Not contiguous; simplistic fallback: fail.
            return None;
        }
        last = frame;
    }
    Some(first)
}

/// Parse `data`, allocate a fresh process, and map the image into it.
///
/// Returns the object handle for the new process and whether the guest
/// architecture requires binary translation on the host.
pub fn load_pe(data: &[u8], pid: u64) -> Option<(objects::Handle, bool)> {
    let image = parse_pe(data)?;

    let size = core::mem::size_of::<Process>();
    let align = core::mem::align_of::<Process>();
    let ptr = crate::mm::alloc_early(size, align)? as *mut Process;

    unsafe {
        ptr::write(ptr, Process::new(pid));
        if !load_into_process(&image, &mut *ptr, data) {
            return None;
        }
    }

    let needs_translation = requires_translation(image.machine);
    let handle = objects::allocate(objects::ObjectKind::Process, ptr as *mut ())?;
    Some((handle, needs_translation))
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
