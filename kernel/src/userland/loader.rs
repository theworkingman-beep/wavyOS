//! ELF64 loader — maps PT_LOAD segments into user page tables
//! Provides functions to load ELF binaries and set up user-space tasks
use alloc::vec::Vec;

const ELFMAG: [u8; 4] = *b"\x7fELF";
const ELFCLASS64: u8 = 2;
const ET_EXEC: u16 = 2;
const PT_LOAD: u32 = 1;

#[repr(C)]
struct Elf64Ehdr {
    e_ident: [u8; 16],
    e_type: u16,
    e_machine: u16,
    e_version: u32,
    e_entry: u64,
    e_phoff: u64,
    e_shoff: u64,
    e_flags: u32,
    e_ehsize: u16,
    e_phentsize: u16,
    e_phnum: u16,
    e_shentsize: u16,
    e_shnum: u16,
    e_shstrndx: u16,
}

#[repr(C)]
struct Elf64Phdr {
    p_type: u32,
    p_flags: u32,
    p_offset: u64,
    p_vaddr: u64,
    p_paddr: u64,
    p_filesz: u64,
    p_memsz: u64,
    p_align: u64,
}

/// User stack top address (grows down from here)
pub const USER_STACK_TOP: u64 = 0x7FFF_FFF0_0000;
/// Stack size in pages
pub const USER_STACK_PAGES: usize = 4;

/// Load an ELF64 binary into the given page tables.
/// Returns (entry_point, stack_top) on success.
#[cfg(target_arch = "x86_64")]
pub fn load_elf(
    data: &[u8],
    page_tables: &mut crate::arch::x86_64::TaskPageTables,
) -> Option<(u64, u64)> {
    load_elf_impl_x86_64(data, page_tables)
}

#[cfg(target_arch = "aarch64")]
pub fn load_elf(
    data: &[u8],
    page_tables: &mut crate::arch::aarch64::TaskPageTables,
) -> Option<(u64, u64)> {
    load_elf_impl_aarch64(data, page_tables)
}

#[cfg(target_arch = "x86_64")]
fn load_elf_impl_x86_64(
    data: &[u8],
    page_tables: &mut crate::arch::x86_64::TaskPageTables,
) -> Option<(u64, u64)> {
    if data.len() < core::mem::size_of::<Elf64Ehdr>() { return None; }

    let hdr = unsafe { &*(data.as_ptr() as *const Elf64Ehdr) };
    if &hdr.e_ident[..4] != &ELFMAG || hdr.e_ident[4] != ELFCLASS64 || hdr.e_type != ET_EXEC {
        return None;
    }

    if hdr.e_machine != 62 { return None; } // EM_X86_64

    let ph_off = hdr.e_phoff as usize;
    let ph_size = core::mem::size_of::<Elf64Phdr>();

    // Collect PT_LOAD segments
    let mut segments: Vec<(u64, u64, u64, u64, u32)> = Vec::new();
    for i in 0..hdr.e_phnum {
        let off = ph_off + (i as usize) * ph_size;
        if off + ph_size > data.len() { break; }
        let ph = unsafe { &*(data.as_ptr().add(off) as *const Elf64Phdr) };
        if ph.p_type == PT_LOAD {
            segments.push((ph.p_vaddr, ph.p_offset, ph.p_filesz, ph.p_memsz, ph.p_flags));
        }
    }

    // Map each PT_LOAD segment
    for (vaddr, file_offset, filesz, memsz, flags) in segments.iter() {
        let writable = (*flags & 2) != 0;
        let start_page = *vaddr & !0xFFF;
        let end_page = (*vaddr + *memsz + 0xFFF) & !0xFFF;

        let mut current_vaddr = start_page;
        let mut remaining = *memsz;
        let mut foff = *file_offset;
        let mut file_remaining = *filesz;

        while current_vaddr < end_page && remaining > 0 {
            let frame_phys = crate::arch::x86_64::map_user_page(page_tables, current_vaddr, writable)?;

            let copy_size = core::cmp::min(4096u64, remaining) as usize;
            let file_copy = core::cmp::min(copy_size as u64, file_remaining) as usize;

            let frame_ptr = frame_phys as *mut u8;

            if file_copy > 0 {
                let src_off = (foff + (current_vaddr - start_page)) as usize;
                if src_off + file_copy <= data.len() {
                    unsafe {
                        core::ptr::copy_nonoverlapping(
                            data.as_ptr().add(src_off),
                            frame_ptr,
                            file_copy,
                        );
                    }
                }
            }

            if file_copy < copy_size {
                unsafe {
                    core::ptr::write_bytes(frame_ptr.add(file_copy), 0, copy_size - file_copy);
                }
            }

            current_vaddr += 4096;
            remaining = remaining.saturating_sub(4096);
            if file_remaining > 4096 {
                file_remaining -= 4096;
            } else {
                file_remaining = 0;
            }
        }
    }

    let stack_top = setup_user_stack_x86_64(page_tables)?;
    Some((hdr.e_entry, stack_top))
}

#[cfg(target_arch = "aarch64")]
fn load_elf_impl_aarch64(
    data: &[u8],
    page_tables: &mut crate::arch::aarch64::TaskPageTables,
) -> Option<(u64, u64)> {
    if data.len() < core::mem::size_of::<Elf64Ehdr>() { return None; }

    let hdr = unsafe { &*(data.as_ptr() as *const Elf64Ehdr) };
    if &hdr.e_ident[..4] != &ELFMAG || hdr.e_ident[4] != ELFCLASS64 || hdr.e_type != ET_EXEC {
        return None;
    }

    if hdr.e_machine != 183 { return None; } // EM_AARCH64

    let ph_off = hdr.e_phoff as usize;
    let ph_size = core::mem::size_of::<Elf64Phdr>();

    let mut segments: Vec<(u64, u64, u64, u64, u32)> = Vec::new();
    for i in 0..hdr.e_phnum {
        let off = ph_off + (i as usize) * ph_size;
        if off + ph_size > data.len() { break; }
        let ph = unsafe { &*(data.as_ptr().add(off) as *const Elf64Phdr) };
        if ph.p_type == PT_LOAD {
            segments.push((ph.p_vaddr, ph.p_offset, ph.p_filesz, ph.p_memsz, ph.p_flags));
        }
    }

    for (vaddr, file_offset, filesz, memsz, flags) in segments.iter() {
        let writable = (*flags & 2) != 0;
        let start_page = *vaddr & !0xFFF;
        let end_page = (*vaddr + *memsz + 0xFFF) & !0xFFF;

        let mut current_vaddr = start_page;
        let mut remaining = *memsz;
        let mut foff = *file_offset;
        let mut file_remaining = *filesz;

        while current_vaddr < end_page && remaining > 0 {
            let frame_phys = crate::arch::aarch64::map_user_page(page_tables, current_vaddr, writable)?;

            let copy_size = core::cmp::min(4096u64, remaining) as usize;
            let file_copy = core::cmp::min(copy_size as u64, file_remaining) as usize;

            let frame_ptr = frame_phys as *mut u8;

            if file_copy > 0 {
                let src_off = (foff + (current_vaddr - start_page)) as usize;
                if src_off + file_copy <= data.len() {
                    unsafe {
                        core::ptr::copy_nonoverlapping(
                            data.as_ptr().add(src_off),
                            frame_ptr,
                            file_copy,
                        );
                    }
                }
            }

            if file_copy < copy_size {
                unsafe {
                    core::ptr::write_bytes(frame_ptr.add(file_copy), 0, copy_size - file_copy);
                }
            }

            current_vaddr += 4096;
            remaining = remaining.saturating_sub(4096);
            if file_remaining > 4096 {
                file_remaining -= 4096;
            } else {
                file_remaining = 0;
            }
        }
    }

    let stack_top = setup_user_stack_aarch64(page_tables)?;
    Some((hdr.e_entry, stack_top))
}

/// Set up user stack for x86_64
#[cfg(target_arch = "x86_64")]
pub fn setup_user_stack_x86_64(page_tables: &mut crate::arch::x86_64::TaskPageTables) -> Option<u64> {
    for i in 0..USER_STACK_PAGES {
        let vaddr = USER_STACK_TOP - ((i as u64 + 1) * 4096);
        crate::arch::x86_64::map_user_page(page_tables, vaddr, true)?;
    }
    Some(USER_STACK_TOP)
}

#[cfg(target_arch = "aarch64")]
pub fn setup_user_stack_aarch64(page_tables: &mut crate::arch::aarch64::TaskPageTables) -> Option<u64> {
    for i in 0..USER_STACK_PAGES {
        let vaddr = USER_STACK_TOP - ((i as u64 + 1) * 4096);
        crate::arch::aarch64::map_user_page(page_tables, vaddr, true)?;
    }
    Some(USER_STACK_TOP)
}

/// Prepare a user task to enter user mode (x86_64).
#[cfg(target_arch = "x86_64")]
pub fn prepare_user_task(
    task: &mut crate::scheduler::Task,
    entry: usize,
    stack_top: usize,
    pml4_phys: usize,
) {
    let mut sp = task.stack as usize + crate::scheduler::STACK_SIZE;

    // Build an iretq frame on the task stack
    // iretq pops: RIP, CS, RFLAGS, RSP, SS

    // SS (ring 3 data segment = 0x23)
    sp -= core::mem::size_of::<usize>();
    unsafe { *(sp as *mut usize) = 0x23usize; }

    // RSP (user stack top)
    sp -= core::mem::size_of::<usize>();
    unsafe { *(sp as *mut usize) = stack_top; }

    // RFLAGS (interrupts enabled = 0x202)
    sp -= core::mem::size_of::<usize>();
    unsafe { *(sp as *mut usize) = 0x202usize; }

    // CS (ring 3 code segment = 0x1B)
    sp -= core::mem::size_of::<usize>();
    unsafe { *(sp as *mut usize) = 0x1Bu64 as usize; }

    // RIP (entry point)
    sp -= core::mem::size_of::<usize>();
    unsafe { *(sp as *mut usize) = entry; }

    // Address of iretq stub
    sp -= core::mem::size_of::<usize>();
    unsafe { *(sp as *mut usize) = iretq_stub_x86_64 as usize; }

    // Callee-saved regs (will be popped by switch_context)
    for _ in 0..6 {
        sp -= core::mem::size_of::<usize>();
        unsafe { *(sp as *mut usize) = 0; }
    }

    task.context.rsp = sp;
}

/// Prepare a user task to enter user mode (aarch64).
#[cfg(target_arch = "aarch64")]
pub fn prepare_user_task(
    task: &mut crate::scheduler::Task,
    entry: usize,
    stack_top: usize,
    ttbr0_phys: usize,
) {
    let mut sp = task.stack as usize + crate::scheduler::STACK_SIZE;

    // Push arguments for eret stub: entry, stack_top, ttbr0_phys
    sp -= core::mem::size_of::<usize>();
    unsafe { *(sp as *mut usize) = ttbr0_phys; }
    sp -= core::mem::size_of::<usize>();
    unsafe { *(sp as *mut usize) = stack_top; }
    sp -= core::mem::size_of::<usize>();
    unsafe { *(sp as *mut usize) = entry; }

    // Push address of eret_stub_aarch64
    sp -= core::mem::size_of::<usize>();
    unsafe { *(sp as *mut usize) = eret_stub_aarch64 as usize; }

    // Callee-saved regs x19-x28, x29(fp), x30(lr)
    for _ in 0..11 {
        sp -= core::mem::size_of::<usize>();
        unsafe { *(sp as *mut usize) = 0; }
    }

    task.context.rsp = sp;
}

/// x86_64 stub that does iretq (used as "return address" for user tasks)
#[cfg(target_arch = "x86_64")]
#[unsafe(naked)]
extern "C" fn iretq_stub_x86_64() -> ! {
    unsafe {
        core::arch::naked_asm!("iretq");
    }
}

/// aarch64 stub that sets up EL0 and does eret
#[cfg(target_arch = "aarch64")]
#[unsafe(naked)]
extern "C" fn eret_stub_aarch64() -> ! {
    unsafe {
        core::arch::naked_asm!(
            "ldr x0, [sp], #8",        // entry
            "ldr x1, [sp], #8",        // stack_top
            "ldr x2, [sp], #8",        // ttbr0_phys
            "msr ttbr0_el1, x2",
            "isb",
            "msr elr_el1, x0",
            "msr sp_el0, x1",
            "mov x3, #0",
            "msr spsr_el1, x3",
            "eret",
        );
    }
}

/// Exec syscall: load ELF binary from file path and replace current process
pub fn exec(path_ptr: *const u8, len: usize) -> usize {
    // Copy path from user space
    if len == 0 || len > 256 {
        return 0xFFFF;
    }
    let mut path_buf = alloc::vec![0u8; len];
    unsafe {
        crate::syscalls::copy_from_user(&mut path_buf, path_ptr as usize);
    }
    let path = match core::str::from_utf8(&path_buf) {
        Ok(s) => s,
        Err(_) => return 0xFFFE,
    };

    // Read ELF file from filesystem
    let data = match crate::fs::read_file(path) {
        Ok(d) => d,
        Err(_) => return 0xFFFC,
    };

    // Get current process
    let pid = crate::scheduler::current_task_id();
    let mut procs = crate::scheduler::get_processes();

    let task_idx = match procs.iter().position(|p| p.pid == pid) {
        Some(idx) => idx,
        None => return 0xFFFB,
    };

    let task = &mut procs[task_idx].task;
    let page_tables = match task.page_tables.as_mut() {
        Some(pt) => pt,
        None => return 0xFFFA,
    };

    // Load ELF
    let (entry, stack_top) = match load_elf(&data, page_tables) {
        Some((e, s)) => (e, s),
        None => return 0xFFF9,
    };

    // Get page table root
    #[cfg(target_arch = "x86_64")]
    let pt_root = page_tables.pml4_phys;
    #[cfg(target_arch = "aarch64")]
    let pt_root = page_tables.ttbr0_phys;

    // Prepare user task to jump to entry point
    prepare_user_task(task, entry as usize, stack_top as usize, pt_root);

    // Update task entry point
    task.entry = entry as usize;

    // yield to let the scheduler switch to us (we'll enter user mode)
    drop(procs);
    crate::scheduler::yield_cpu();

    // Should not reach here - we enter user mode instead
    0
}
