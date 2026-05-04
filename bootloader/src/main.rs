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

// Page table entry flags
const PAGE_PRESENT: u64 = 1 << 0;
const PAGE_WRITABLE: u64 = 1 << 1;
const PAGE_HUGE: u64 = 1 << 7;

#[repr(C)]
struct Elf64Ehdr { e_ident:[u8;16], e_type:u16, e_machine:u16, e_version:u32, e_entry:u64, e_phoff:u64, e_shoff:u64, e_flags:u32, e_ehsize:u16, e_phentsize:u16, e_phnum:u16, e_shentsize:u16, e_shnum:u16, e_shstrndx:u16 }

#[repr(C)]
struct Elf64Phdr { p_type:u32, p_flags:u32, p_offset:u64, p_vaddr:u64, p_paddr:u64, p_filesz:u64, p_memsz:u64, p_align:u64 }

/// Segment load info: virtual address and physical address
struct SegmentMap {
    vaddr: u64,
    paddr: u64,
    pages: usize,
}

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

/// Allocate a zeroed 4KB page for page table use
fn alloc_pt_page(bs: &uefi::table::boot::BootServices) -> Result<u64, uefi::Error> {
    bs.allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, 1)
}

/// Set up page tables mapping kernel virtual addresses to physical addresses
/// Also identity-maps all physical memory up to max_phys
/// Returns the physical address of the PML4 table (for CR3)
fn setup_page_tables(
    bs: &uefi::table::boot::BootServices,
    segments: &[SegmentMap],
    max_phys: u64,
) -> Result<u64, uefi::Error> {
    // Allocate PML4 (Level 4)
    let pml4_phys = alloc_pt_page(bs)?;
    let pml4 = unsafe { &mut *(pml4_phys as *mut [u64; 512]) };
    pml4.iter_mut().for_each(|e| *e = 0);

    // Identity-map all physical memory up to max_phys using 2MB huge pages
    // x86_64 layout: PML4[9] -> PDPT[9] -> PD[9] -> 2MB page
    // For addr P: PML4=(P>>39)&0x1FF, PDPT=(P>>30)&0x1FF, PD=(P>>21)&0x1FF
    let mut addr: u64 = 0;
    while addr < max_phys {
        let pml4_idx = ((addr >> 39) & 0x1FF) as usize;
        let pdpt_idx = ((addr >> 30) & 0x1FF) as usize;
        let pd_idx = ((addr >> 21) & 0x1FF) as usize;

        // Get or create PDPT
        if pml4[pml4_idx] & PAGE_PRESENT == 0 {
            let phys = alloc_pt_page(bs)?;
            unsafe { &mut *(phys as *mut [u64; 512]) }.iter_mut().for_each(|e| *e = 0);
            pml4[pml4_idx] = phys | PAGE_PRESENT | PAGE_WRITABLE;
        }
        let pdpt_phys = pml4[pml4_idx] & !0xFFF;
        let pdpt = unsafe { &mut *(pdpt_phys as *mut [u64; 512]) };

        // Get or create PD
        if pdpt[pdpt_idx] & PAGE_PRESENT == 0 {
            let phys = alloc_pt_page(bs)?;
            unsafe { &mut *(phys as *mut [u64; 512]) }.iter_mut().for_each(|e| *e = 0);
            pdpt[pdpt_idx] = phys | PAGE_PRESENT | PAGE_WRITABLE;
        }
        let pd_phys = pdpt[pdpt_idx] & !0xFFF;
        let pd = unsafe { &mut *(pd_phys as *mut [u64; 512]) };

        // Set 2MB huge page (skip if already set, e.g., by kernel mapping)
        if pd[pd_idx] & PAGE_PRESENT == 0 {
            pd[pd_idx] = addr | PAGE_PRESENT | PAGE_WRITABLE | PAGE_HUGE;
        }

        addr += 0x200000; // 2MB
    }

    // Map kernel virtual addresses to physical addresses using 4KB pages
    for seg in segments {
        let mut vpage = seg.vaddr >> 12;
        let mut ppage = seg.paddr >> 12;
        let end_vpage = vpage + seg.pages as u64;

        while vpage < end_vpage {
            let pml4_idx = ((vpage >> 27) & 0x1FF) as usize;
            let pdpt_idx = ((vpage >> 18) & 0x1FF) as usize;
            let pd_idx = ((vpage >> 9) & 0x1FF) as usize;
            let pt_idx = (vpage & 0x1FF) as usize;

            // Get or create PDPT (might already exist from identity mapping)
            let pdpt_phys = if pml4[pml4_idx] & PAGE_PRESENT != 0 {
                pml4[pml4_idx] & !0xFFF
            } else {
                let phys = alloc_pt_page(bs)?;
                unsafe { &mut *(phys as *mut [u64; 512]) }.iter_mut().for_each(|e| *e = 0);
                pml4[pml4_idx] = phys | PAGE_PRESENT | PAGE_WRITABLE;
                phys
            };

            let pdpt = unsafe { &mut *(pdpt_phys as *mut [u64; 512]) };

            let pd_phys = if pdpt[pdpt_idx] & PAGE_PRESENT != 0 {
                pdpt[pdpt_idx] & !0xFFF
            } else {
                let phys = alloc_pt_page(bs)?;
                unsafe { &mut *(phys as *mut [u64; 512]) }.iter_mut().for_each(|e| *e = 0);
                pdpt[pdpt_idx] = phys | PAGE_PRESENT | PAGE_WRITABLE;
                phys
            };

            let pd = unsafe { &mut *(pd_phys as *mut [u64; 512]) };

            // Check if this PD entry is a 2MB huge page (from identity mapping)
            // If so, replace it with a PT pointer
            if pd[pd_idx] & PAGE_HUGE != 0 {
                let phys = alloc_pt_page(bs)?;
                unsafe { &mut *(phys as *mut [u64; 512]) }.iter_mut().for_each(|e| *e = 0);
                pd[pd_idx] = phys | PAGE_PRESENT | PAGE_WRITABLE;
            }

            let pt_phys = if pd[pd_idx] & PAGE_PRESENT != 0 {
                pd[pd_idx] & !0xFFF
            } else {
                let phys = alloc_pt_page(bs)?;
                unsafe { &mut *(phys as *mut [u64; 512]) }.iter_mut().for_each(|e| *e = 0);
                pd[pd_idx] = phys | PAGE_PRESENT | PAGE_WRITABLE;
                phys
            };

            let pt = unsafe { &mut *(pt_phys as *mut [u64; 512]) };
            pt[pt_idx] = (ppage << 12) | PAGE_PRESENT | PAGE_WRITABLE;

            vpage += 1;
            ppage += 1;
        }
    }

    Ok(pml4_phys)
}

/// Write a byte to COM1 UART
unsafe fn uart_putc(c: u8) {
    let mut val: u8;
    // Wait for transmit buffer empty
    loop {
        core::arch::asm!("in al, dx", out("al") val, in("dx") 0x3FDu16);
        if val & 0x20 != 0 { break; }
    }
    // Write character
    core::arch::asm!("out dx, al", in("dx") 0x3F8u16, in("al") c);
}
unsafe fn enable_paging(pml4_phys: u64) {
    // Load CR3 with PML4 physical address
    core::arch::asm!("mov cr3, {}", in(reg) pml4_phys);

    // Enable PAE (Physical Address Extension) - set bit 5 in CR4
    let mut cr4: u64;
    core::arch::asm!("mov {}, cr4", out(reg) cr4);
    cr4 |= 1 << 5; // PAE
    core::arch::asm!("mov cr4, {}", in(reg) cr4);

    // Enable paging - set bit 31 in CR0
    let mut cr0: u64;
    core::arch::asm!("mov {}, cr0", out(reg) cr0);
    cr0 |= 1 << 31; // PG
    core::arch::asm!("mov cr0, {}", in(reg) cr0);
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

    let mut segments = alloc::vec::Vec::new();

    // Calculate total range needed
    let mut lowest_vaddr = u64::MAX;
    let mut highest_end: u64 = 0;
    for ph in phdrs {
        if ph.p_type != PT_LOAD { continue; }
        let vaddr = ph.p_vaddr & !0xFFF;
        let end = ((ph.p_vaddr + ph.p_memsz + 0xFFF) & !0xFFF);
        if vaddr < lowest_vaddr { lowest_vaddr = vaddr; }
        if end > highest_end { highest_end = end; }
    }
    let total_pages = ((highest_end - lowest_vaddr) / 0x1000) as usize;

    // Allocate the entire kernel range as one contiguous block
    let base_addr = st.boot_services().allocate_pages(AllocateType::Address(lowest_vaddr), MemoryType::LOADER_DATA, total_pages);
    let base_addr = match base_addr {
        Ok(a) => a,
        Err(e) => { println!("alloc failed at {:x}, {} pages: {:?}", lowest_vaddr, total_pages, e); return Status::LOAD_ERROR; }
    };
    println!("kernel: vaddr={:x} -> phys={:x} ({} pages)", lowest_vaddr, base_addr, total_pages);

    segments.push(SegmentMap {
        vaddr: lowest_vaddr,
        paddr: base_addr,
        pages: total_pages,
    });

    // Copy each segment into the allocated block
    for ph in phdrs {
        if ph.p_type != PT_LOAD { continue; }
        let vaddr = ph.p_vaddr;
        let filesz = ph.p_filesz as usize;
        let memsz = ph.p_memsz as usize;
        let offset = ph.p_offset as usize;
        let dest = (base_addr + (vaddr - lowest_vaddr)) as *mut u8;

        unsafe {
            if offset + filesz <= kernel_raw.as_ref().unwrap().len() {
                ptr::copy_nonoverlapping(kernel_raw.as_ref().unwrap().as_ptr().add(offset), dest, filesz);
            }
            if filesz < memsz {
                ptr::write_bytes(dest.add(filesz), 0, memsz - filesz);
            }
        }
    }

    // Kernel was loaded at its expected virtual address, no page tables needed
    println!("kernel loaded at expected address");

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

    unsafe { uart_putc(b'E'); } // Mark: before exit_boot_services
    let (_st_runtime, _) = st.exit_boot_services(MemoryType::LOADER_DATA);
    unsafe { uart_putc(b'X'); }

    // Kernel was loaded at its expected virtual address, no page tables needed
    // Just switch stack and jump
    unsafe {
        core::arch::asm!(
            "mov rsp, {stack}",
            "mov rdi, {bi}",
            "jmp {entry}",
            entry = in(reg) entry,
            stack = in(reg) stack_top,
            bi = in(reg) bi_ptr,
        );
    }
    unreachable!();
}
