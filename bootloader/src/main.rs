//! Vibe Coded OS UEFI Bootloader
//!
//! Supports both x86_64 and aarch64 UEFI boot.
//! On x86_64: kernel is loaded at its linked virtual address (no page tables needed).
//! On aarch64: kernel is loaded anywhere, page tables map virtual→physical.

#![no_std]
#![no_main]
#![cfg_attr(target_arch = "x86_64", feature(abi_efiapi))]

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

/// Write a byte to COM1 UART (x86_64 only)
#[cfg(target_arch = "x86_64")]
unsafe fn uart_putc(c: u8) {
    let mut val: u8;
    loop {
        core::arch::asm!("in al, dx", out("al") val, in("dx") 0x3FDu16);
        if val & 0x20 != 0 { break; }
    }
    core::arch::asm!("out dx, al", in("dx") 0x3F8u16, in("al") c);
}

#[cfg(target_arch = "aarch64")]
unsafe fn uart_putc(_c: u8) {
    // UART output not implemented for aarch64
}

/// Load kernel ELF into memory. On x86_64, allocates at the linked virtual address.
/// On aarch64, allocates anywhere and sets up page tables to map virtual→physical.
/// Returns (phys_base, extra) where extra is (page_table_phys, entry_vaddr) on aarch64, (entry_vaddr, 0) on x86_64.
#[cfg(target_arch = "x86_64")]
fn load_kernel(st: &mut SystemTable<Boot>, phdrs: &[Elf64Phdr], kernel_raw: &[u8], entry_vaddr: u64) -> (u64, (u64, u64)) {
    let mut lowest_vaddr = u64::MAX;
    let mut highest_end: u64 = 0;
    for ph in phdrs {
        if ph.p_type != PT_LOAD { continue; }
        let vaddr = ph.p_vaddr & !0xFFF;
        let end = (ph.p_vaddr + ph.p_memsz + 0xFFF) & !0xFFF;
        if vaddr < lowest_vaddr { lowest_vaddr = vaddr; }
        if end > highest_end { highest_end = end; }
    }
    let total_pages = ((highest_end - lowest_vaddr) / 0x1000) as usize;

    let base_addr = st.boot_services().allocate_pages(AllocateType::Address(lowest_vaddr), MemoryType::LOADER_DATA, total_pages);
    let base_addr = match base_addr {
        Ok(a) => a,
        Err(e) => { println!("alloc failed at {:x}, {} pages: {:?}", lowest_vaddr, total_pages, e); return (0, (0, 0)); }
    };

    for ph in phdrs {
        if ph.p_type != PT_LOAD { continue; }
        let vaddr = ph.p_vaddr;
        let filesz = ph.p_filesz as usize;
        let memsz = ph.p_memsz as usize;
        let offset = ph.p_offset as usize;
        let dest = (base_addr + (vaddr - lowest_vaddr)) as *mut u8;
        unsafe {
            if offset + filesz <= kernel_raw.len() {
                ptr::copy_nonoverlapping(kernel_raw.as_ptr().add(offset), dest, filesz);
            }
            if filesz < memsz {
                ptr::write_bytes(dest.add(filesz), 0, memsz - filesz);
            }
        }
    }
    (base_addr, (entry_vaddr, 0)) // virt == phys on x86_64
}

/// aarch64: allocates kernel at 0x40000000 (identity-mapped), sets up simple identity page tables.
#[cfg(target_arch = "aarch64")]
fn load_kernel(st: &mut SystemTable<Boot>, phdrs: &[Elf64Phdr], kernel_raw: &[u8], _entry_vaddr: u64) -> (u64, (u64, u64)) {
    const KERNEL_VA: u64 = 0x40000000; // Kernel linked at this address (identity-mapped)

    let mut lowest_vaddr = u64::MAX;
    let mut highest_end: u64 = 0;
    for ph in phdrs {
        if ph.p_type != PT_LOAD { continue; }
        let vaddr = ph.p_vaddr & !0xFFF;
        let end = (ph.p_vaddr + ph.p_memsz + 0xFFF) & !0xFFF;
        if vaddr < lowest_vaddr { lowest_vaddr = vaddr; }
        if end > highest_end { highest_end = end; }
    }
    let total_pages = ((highest_end - lowest_vaddr) / 0x1000) as usize;

    // Allocate kernel at fixed address 0x40000000
    let phys_base = st.boot_services().allocate_pages(AllocateType::Address(KERNEL_VA), MemoryType::LOADER_DATA, total_pages);
    let phys_base = match phys_base {
        Ok(a) => a,
        Err(e) => { println!("alloc failed at {:x}, {} pages: {:?}", KERNEL_VA, total_pages, e); return (0, (0, 0)); }
    };
    println!("kernel: phys={:x} ({} pages)", phys_base, total_pages);

    // Copy segments
    for ph in phdrs {
        if ph.p_type != PT_LOAD { continue; }
        let filesz = ph.p_filesz as usize;
        let memsz = ph.p_memsz as usize;
        let offset = ph.p_offset as usize;
        let dest = (phys_base + (ph.p_vaddr - lowest_vaddr)) as *mut u8;
        unsafe {
            if offset + filesz <= kernel_raw.len() {
                ptr::copy_nonoverlapping(kernel_raw.as_ptr().add(offset), dest, filesz);
            }
            if filesz < memsz {
                ptr::write_bytes(dest.add(filesz), 0, memsz - filesz);
            }
        }
    }

    // Simple identity page tables using only L1 block entries
    // Allocate 1 page for L0 + L1 combined (L0 has 1 entry, L1 has 512 entries = 4096 bytes)
    let pt_base = st.boot_services().allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, 2).unwrap_or(0);
    if pt_base == 0 {
        println!("failed to allocate page tables");
        return (0, (0, 0));
    }
    let l0_table = pt_base;
    let l1_table = pt_base + 0x1000;

    unsafe {
        ptr::write_bytes(l0_table as *mut u8, 0, 0x2000);
    }

    // L0[0] -> L1
    unsafe { *(l0_table as *mut u64) = l1_table | 0x3; }

    // L1: identity-map entire 512GB space with 1GB blocks
    // Only need entries 0-3 to cover 4GB (enough for virt machine)
    let l1 = l1_table as *mut u64;
    let block_attr = 0x743u64; // Valid + Block + AF + AP(01) + SH(11)
    unsafe {
        // Map first 4GB: L1[0]=0x0, L1[1]=0x40000000, L1[2]=0x80000000, L1[3]=0xC0000000
        for i in 0..4u64 {
            *l1.add(i as usize) = (i << 30) | block_attr;
        }
        // Map up to 512GB for completeness
        for i in 4..512u64 {
            *l1.add(i as usize) = (i << 30) | block_attr;
        }
    }

    println!("page tables: L0={:x}, identity map", l0_table);

    // Entry point is at the ELF's entry address (which is now 0x40000000 + offset)
    let entry_va = _entry_vaddr;

    (phys_base, (l0_table, entry_va))
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

    let (entry_vaddr, phdrs) = match kernel_raw {
        Some(ref data) => match parse_elf(data) {
            Some((e, p)) => { println!("entry={:x}", e); (e, p) }
            None => { println!("bad ELF"); return Status::LOAD_ERROR; }
        }
        None => { println!("no kernel"); return Status::LOAD_ERROR; }
    };

    let (kernel_phys_base, extra) = load_kernel(&mut st, phdrs, kernel_raw.as_ref().unwrap(), entry_vaddr);
    if kernel_phys_base == 0 {
        return Status::LOAD_ERROR;
    }

    println!("kernel loaded");

    #[cfg(target_arch = "aarch64")]
    let (page_table_phys, kernel_entry_vaddr) = (extra.0, extra.1);
    #[cfg(target_arch = "x86_64")]
    let (kernel_entry_vaddr, _unused) = (entry_vaddr, extra);

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

    // Allocate a kernel stack (16 KB) before exiting boot services
    let stack_phys = st.boot_services().allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, 4)
        .unwrap_or(0);
    let stack_top = stack_phys + 4 * 4096;
    println!("kernel stack: {:x}", stack_top);

    unsafe { uart_putc(b'E'); }
    let (_st_runtime, _) = st.exit_boot_services(MemoryType::LOADER_DATA);
    unsafe { uart_putc(b'X'); }

    // Compute kernel entry point (virtual address)
    let kernel_entry = if kernel_phys_base != 0 {
        // On aarch64, entry is at virtual address 0x10000000 (where the kernel is linked)
        // On x86_64, kernel_phys_base == 0x10000000 (allocated at linked address)
        0x10000000u64
    } else {
        return Status::LOAD_ERROR;
    };

    #[cfg(target_arch = "x86_64")]
    unsafe {
        core::arch::asm!(
            "mov rsp, {stack}",
            "mov rdi, {bi}",
            "jmp {entry}",
            entry = in(reg) kernel_entry_vaddr,
            stack = in(reg) stack_top,
            bi = in(reg) bi_ptr,
        );
    }
    #[cfg(target_arch = "aarch64")]
    unsafe {
        // Jump to kernel at identity-mapped address (0x40000000)
        core::arch::asm!(
            "mov x0, {bi}",
            "mov sp, {stack}",
            "br {entry}",
            entry = in(reg) kernel_entry_vaddr,
            bi = in(reg) bi_ptr,
            stack = in(reg) stack_top,
        );
    }
    unreachable!();
}
