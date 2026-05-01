#![no_std]
#![no_main]
#![feature(abi_efiapi)]

extern crate alloc;

use core::ptr;
use core::slice;
use uefi::prelude::*;
use uefi_services::{init, println};
use uefi::proto::console::gop::GraphicsOutput;
use uefi::proto::media::file::{Directory, File, FileAttribute, FileMode, RegularFile};
use uefi::table::boot::{AllocateType, MemoryType};

use common::{BootInfo, FramebufferInfo, MemoryRegion, MemoryRegionKind};

const ELFMAG: [u8;4] = *b"\x7fELF";
const PT_LOAD: u32 = 1;

#[repr(C)]
struct Elf64Ehdr { e_ident:[u8;16], e_type:u16, e_machine:u16, e_version:u32, e_entry:u64, e_phoff:u64, e_shoff:u64, e_flags:u32, e_ehsize:u16, e_phentsize:u16, e_phnum:u16, e_shentsize:u16, e_shnum:u16, e_shstrndx:u16 }

#[repr(C)]
struct Elf64Phdr { p_type:u32, p_flags:u32, p_offset:u64, p_vaddr:u64, p_paddr:u64, p_filesz:u64, p_memsz:u64, p_align:u64 }

fn kind_from_efi(ty: MemoryType) -> MemoryRegionKind {
    match ty {
        MemoryType::CONVENTIONAL => MemoryRegionKind::Usable,
        MemoryType::LOADER_CODE | MemoryType::LOADER_DATA => MemoryRegionKind::Bootloader,
        MemoryType::BOOT_SERVICES_CODE | MemoryType::BOOT_SERVICES_DATA => MemoryRegionKind::Bootloader,
        MemoryType::RUNTIME_SERVICES_CODE | MemoryType::RUNTIME_SERVICES_DATA => MemoryRegionKind::Reserved,
        MemoryType::ACPI_RECLAIM | MemoryType::ACPI_NON_VOLATILE => MemoryRegionKind::Reserved,
        _ => MemoryRegionKind::Reserved,
    }
}

fn cstr16(s: &str) -> alloc::vec::Vec<u16> {
    let mut v: alloc::vec::Vec<u16> = s.encode_utf16().collect();
    v.push(0); v
}

fn read_file(mut root: Directory, path: &[u16]) -> Option<alloc::vec::Vec<u8>> {
    let name = uefi::data_types::CStr16::from_u16_with_nul(path).unwrap();
    let mut file = match root.open(name, FileMode::Read, FileAttribute::empty()) {
        Ok(handle) => unsafe { RegularFile::new(handle) },
        Err(_) => return None,
    };
    let mut buf = alloc::vec::Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match file.read(&mut chunk) {
            Ok(0) => break, Ok(n) => buf.extend_from_slice(&chunk[..n]), Err(_) => break,
        }
    }
    file.close();
    if buf.is_empty() { None } else { Some(buf) }
}

fn parse_elf(data: &[u8]) -> Option<(u64, &[Elf64Phdr])> {
    if data.len() < core::mem::size_of::<Elf64Ehdr>() { return None; }
    let hdr = unsafe { &*(data.as_ptr() as *const Elf64Ehdr) };
    if hdr.e_ident[..4] != ELFMAG || hdr.e_ident[4] != 2 || hdr.e_ident[5] != 1 || hdr.e_type != 2 { return None; }
    let entry = hdr.e_entry;
    let phoff = hdr.e_phoff as usize;
    let phnum = hdr.e_phnum as usize;
    let phentsize = hdr.e_phentsize as usize;
    if data.len() < phoff + phnum * phentsize { return None; }
    let phdrs = unsafe { slice::from_raw_parts(data.as_ptr().add(phoff) as *const Elf64Phdr, phnum) };
    Some((entry, phdrs))
}

#[entry]
fn main(image_handle: Handle, mut st: SystemTable<Boot>) -> Status {
    init(&mut st).unwrap();
    println!("Bootloader...");

    let kernel_raw = {
        let bs = st.boot_services();
        let mut fs = bs.get_image_file_system(image_handle).unwrap();
        let root = unsafe { (*fs).open_volume().unwrap() };
        let name = cstr16("kernel");
        read_file(root, &name)
    };

    let (entry, phdrs) = match kernel_raw {
        Some(ref data) => match parse_elf(data) {
            Some((e, p)) => { println!("entry={:x}", e); (e, p) }
            None => { println!("bad ELF"); return Status::LOAD_ERROR; }
        }
        None => { println!("no kernel"); return Status::LOAD_ERROR; }
    };

    for ph in phdrs {
        if ph.p_type != PT_LOAD { continue; }
        let vaddr = ph.p_vaddr; let filesz = ph.p_filesz; let memsz = ph.p_memsz; let offset = ph.p_offset as usize;
        let paddr = vaddr & !0xFFF;
        let page_count = ((vaddr + memsz - paddr + 0xFFF) / 0x1000) as usize;
        let addr = st.boot_services().allocate_pages(AllocateType::Address(paddr), MemoryType::LOADER_DATA, page_count).unwrap();
        let dest = addr as *mut u8;
        unsafe {
            let off = (vaddr - paddr) as usize;
            if offset + filesz as usize <= kernel_raw.as_ref().unwrap().len() {
                ptr::copy_nonoverlapping(kernel_raw.as_ref().unwrap().as_ptr().add(offset), dest.add(off), filesz as usize);
            }
            if filesz < memsz {
                ptr::write_bytes(dest.add(off + filesz as usize), 0, (memsz - filesz) as usize);
            }
        }
    }

    let (fb_ptr, regions_ptr, regions_len, rsdp) = {
        let fb = {
            let bs = st.boot_services();
            let mut fb_opt = None;
            if let Ok(handle) = bs.get_handle_for_protocol::<GraphicsOutput>() {
                let gop = bs.open_protocol_exclusive::<GraphicsOutput>(handle);
                if let Ok(mut gop) = gop {
                    let mode = gop.current_mode_info();
                    let (w, h) = mode.resolution();
                    let mut fb_obj = gop.frame_buffer();
                    fb_opt = Some(FramebufferInfo {
                        addr: fb_obj.as_mut_ptr() as u64,
                        width: w as u32,
                        height: h as u32,
                        pitch: mode.stride() as u32 * 4,
                        bpp: 32,
                    });
                }
            }
            fb_opt
        };

        let mmap_size = st.boot_services().memory_map_size();
        let mut mmap_buf = alloc::vec![0u8; mmap_size.map_size + mmap_size.entry_size * 8];
        let mmap_iter = st.boot_services().memory_map(&mut mmap_buf).unwrap();

        let mut regions = alloc::vec::Vec::new();
        for desc in mmap_iter.entries() {
            regions.push(MemoryRegion {
                base: desc.phys_start,
                length: desc.page_count * 4096,
                kind: kind_from_efi(desc.ty),
            });
        }

        let fb_ptr = match fb {
            Some(fb) => alloc::boxed::Box::into_raw(alloc::boxed::Box::new(fb)),
            None => ptr::null(),
        };

        let mut rsdp = 0u64;
        for cfg in st.config_table() {
            if cfg.guid == uefi::table::cfg::ACPI2_GUID { rsdp = cfg.address as u64; }
        }

        let regions_ptr = regions.as_ptr(); let regions_len = regions.len();
        let _ = alloc::boxed::Box::leak(alloc::boxed::Box::new(regions));

        (fb_ptr, regions_ptr, regions_len, rsdp)
    };

    let bi = alloc::boxed::Box::new(BootInfo {
        memory_map_ptr: regions_ptr,
        memory_map_len: regions_len,
        framebuffer: fb_ptr,
        rsdp,
        device_tree: 0,
    });
    let bi_ptr = alloc::boxed::Box::into_raw(bi);

    println!("Exiting boot...");
    let (_st_runtime, _) = st.exit_boot_services(MemoryType::LOADER_DATA);
    unsafe {
        let f: extern "C" fn(*mut BootInfo) -> ! = core::mem::transmute(entry);
        f(bi_ptr);
    }
}
