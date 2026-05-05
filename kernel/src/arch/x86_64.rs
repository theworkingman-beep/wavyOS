//! x86_64 architecture support — IDT, PIC, interrupts, paging
use crate::BootInfo;

const IDT_ENTRIES: usize = 256;

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct IdtEntry {
    offset_low: u16,
    selector: u16,
    ist: u8,
    flags: u8,
    offset_mid: u16,
    offset_high: u32,
    reserved: u32,
}

const EMPTY_IDT_ENTRY: IdtEntry = IdtEntry {
    offset_low: 0,
    selector: 0,
    ist: 0,
    flags: 0,
    offset_mid: 0,
    offset_high: 0,
    reserved: 0,
};

static mut IDT: [IdtEntry; IDT_ENTRIES] = [EMPTY_IDT_ENTRY; IDT_ENTRIES];

#[repr(C)]
struct InterruptStackFrame {
    _rip: u64,
    _cs: u64,
    _rflags: u64,
    _rsp: u64,
    _ss: u64,
}

// Page fault handler
extern "x86-interrupt" fn page_fault_handler(sf: &mut InterruptStackFrame, err_code: u64) {
    let cr2: u64;
    unsafe {
        core::arch::asm!("mov {0}, cr2", out(reg) cr2);
    }
    let present = if err_code & 1 != 0 { "present" } else { "not-present" };
    let write = if err_code & 2 != 0 { "write" } else { "read" };
    let user = if err_code & 4 != 0 { "user" } else { "supervisor" };

    // Check if this is a COW (copy-on-write) page
    // COW pages are marked read-only (bit 1 clear) but have PTE_COW bit set (bit 9)
    if write == "write" && present == "present" {
        let pte_cow = 1u64 << 9; // software bit for COW
        let pte_addr_mask = 0x000FFFFFFFFFF000u64;

        // Walk page tables to find the PTE
        let cr3 = unsafe {
            let mut val: u64;
            core::arch::asm!("mov {}, cr3", out(reg) val);
            val
        };
        let pml4_idx = ((cr2 as usize) >> 39) & 0x1FF;
        let pdpt_idx = ((cr2 as usize) >> 30) & 0x1FF;
        let pd_idx = ((cr2 as usize) >> 21) & 0x1FF;
        let pt_idx = ((cr2 as usize) >> 12) & 0x1FF;

        let pml4 = (cr3 & !0xFFF) as *const u64;
        let pml4e = unsafe { *pml4.add(pml4_idx) };
        if pml4e & PTE_PRESENT != 0 {
            let pdpt = (pml4e & !0xFFF) as *const u64;
            let pdpte = unsafe { *pdpt.add(pdpt_idx) };
            if pdpte & PTE_PRESENT != 0 && (pdpte & (1 << 7)) == 0 {
                // Not a huge page
                let pd = (pdpte & !0xFFF) as *const u64;
                let pde = unsafe { *pd.add(pd_idx) };
                if pde & PTE_PRESENT != 0 && (pde & (1 << 7)) == 0 {
                    // Not a large page
                    let pt = (pde & !0xFFF) as *mut u64;
                    let pte = unsafe { *pt.add(pt_idx) };
                    if pte & pte_cow != 0 {
                        // COW fault — copy the page
                        let old_frame = (pte & pte_addr_mask) as usize;
                        let new_frame = crate::mm::frame_alloc::alloc_frame()
                            .expect("COW: failed to allocate frame");
                        // Copy old page content
                        unsafe {
                            core::ptr::copy_nonoverlapping(
                                old_frame as *const u8,
                                new_frame as *mut u8,
                                4096,
                            );
                        }
                        // Map new frame writable, clear COW bit
                        unsafe {
                            *pt.add(pt_idx) = (new_frame as u64) | PTE_PRESENT | PTE_WRITABLE | PTE_ACCESSED | PTE_DIRTY | PTE_USER;
                            // Invalidate TLB entry
                            core::arch::asm!("invlpg [{0}]", in(reg) cr2, options(nostack));
                        }
                        return;
                    }
                }
            }
        }
    }

    log::error!("PAGE FAULT at {:#x}: {} {} {} (rip={:#x})",
        cr2, present, write, user, sf._rip);
    loop { unsafe { core::arch::asm!("hlt") } }
}

// IRQ1 (keyboard) handler
extern "x86-interrupt" fn irq1_handler(_sf: &mut InterruptStackFrame) {
    unsafe {
        let scancode: u8;
        core::arch::asm!("in al, dx", in("dx") 0x60u16, out("al") scancode);
        crate::drivers::ps2kbd::handle_scancode(scancode);
        // EOI to PIC1
        core::arch::asm!("out dx, al", in("dx") 0x20u16, in("al") 0x20u8);
    }
}

// Timer IRQ0 handler (scheduler tick ~100Hz)
extern "x86-interrupt" fn irq0_handler(_sf: &mut InterruptStackFrame) {
    unsafe {
        core::arch::asm!("out dx, al", in("dx") 0x20u16, in("al") 0x20u8);
    }
}

// IRQ12 (PS/2 mouse) handler
extern "x86-interrupt" fn irq12_handler(_sf: &mut InterruptStackFrame) {
    unsafe {
        let byte: u8;
        core::arch::asm!("in al, dx", in("dx") 0x60u16, out("al") byte);
        crate::drivers::ps2mouse::handle_mouse_byte(byte);
        // EOI to both PICs (mouse is on PIC2)
        core::arch::asm!("out dx, al", in("dx") 0xA0u16, in("al") 0x20u8);
        core::arch::asm!("out dx, al", in("dx") 0x20u16, in("al") 0x20u8);
    }
}

extern "x86-interrupt" fn double_fault_handler(_sf: &mut InterruptStackFrame, _err: u64) -> ! {
    log::error!("DOUBLE FAULT");
    loop { unsafe { core::arch::asm!("hlt") } }
}

fn set_idt_entry(idx: usize, handler: usize, selector: u16, flags: u8) {
    unsafe {
        IDT[idx].offset_low = (handler & 0xFFFF) as u16;
        IDT[idx].selector = selector;
        IDT[idx].ist = 0;
        IDT[idx].flags = flags;
        IDT[idx].offset_mid = ((handler >> 16) & 0xFFFF) as u16;
        IDT[idx].offset_high = ((handler >> 32) & 0xFFFFFFFF) as u32;
        IDT[idx].reserved = 0;
    }
}

#[repr(C, packed)]
struct Idtr {
    limit: u16,
    base: u64,
}

// ============================================================
// GDT — required for ring 3 (user mode) transitions
// ============================================================

#[repr(C)]
#[derive(Clone, Copy)]
struct GdtEntry {
    limit_low: u16,
    base_low: u16,
    base_mid: u8,
    access: u8,
    limit_high: u8,
    base_high: u8,
}

const GDT_NULL: GdtEntry = GdtEntry {
    limit_low: 0, base_low: 0, base_mid: 0, access: 0, limit_high: 0, base_high: 0,
};

// Kernel code: ring 0, executable, readable, present
const GDT_KERNEL_CODE: GdtEntry = GdtEntry {
    limit_low: 0xFFFF, base_low: 0, base_mid: 0, access: 0x9A, limit_high: 0xAF, base_high: 0,
};

// Kernel data: ring 0, writable, present
const GDT_KERNEL_DATA: GdtEntry = GdtEntry {
    limit_low: 0xFFFF, base_low: 0, base_mid: 0, access: 0x92, limit_high: 0xCF, base_high: 0,
};

// User code: ring 3, executable, readable, present, DPL=3
const GDT_USER_CODE: GdtEntry = GdtEntry {
    limit_low: 0xFFFF, base_low: 0, base_mid: 0, access: 0xFA, limit_high: 0xAF, base_high: 0,
};

// User data: ring 3, writable, present, DPL=3
const GDT_USER_DATA: GdtEntry = GdtEntry {
    limit_low: 0xFFFF, base_low: 0, base_mid: 0, access: 0xF2, limit_high: 0xCF, base_high: 0,
};

static mut GDT: [GdtEntry; 5] = [
    GDT_NULL,
    GDT_KERNEL_CODE,  // selector 0x08
    GDT_KERNEL_DATA,  // selector 0x10
    GDT_USER_CODE,    // selector 0x1B (0x18 | 3)
    GDT_USER_DATA,    // selector 0x23 (0x20 | 3)
];

#[repr(C, packed)]
struct Gdtr {
    limit: u16,
    base: u64,
}

unsafe fn load_gdt() {
    let gdtr = Gdtr {
        limit: (core::mem::size_of::<GdtEntry>() * 5 - 1) as u16,
        base: core::ptr::addr_of!(GDT) as u64,
    };
    core::arch::asm!("lgdt [{0}]", in(reg) &gdtr, options(nostack));
    // Reload CS and DS
    core::arch::asm!(
        "push 0x08",
        "lea rax, [rip + 2f]",
        "push rax",
        "retfq",
        "2:",
        options(nostack),
    );
    core::arch::asm!("mov ds, {0}", in(reg) 0x10u64, options(nostack));
    core::arch::asm!("mov es, {0}", in(reg) 0x10u64, options(nostack));
    core::arch::asm!("mov ss, {0}", in(reg) 0x10u64, options(nostack));
}

// ============================================================
// Paging — x86_64 4-level page tables (PML4 → PDPT → PD → PT)
// ============================================================

/// Kernel virtual address (identity-mapped)
const KERNEL_VADDR: u64 = 0x10000000;
const KERNEL_SIZE: usize = 64 * 1024 * 1024; // 64 MB for kernel
const PAGE_SIZE: usize = 4096;

/// User space starts at 4 GB
const USER_SPACE_START: u64 = 0x100000000;

/// Page table entry flags
const PTE_PRESENT: u64 = 1 << 0;
const PTE_WRITABLE: u64 = 1 << 1;
const PTE_USER: u64 = 1 << 2;
const PTE_ACCESSED: u64 = 1 << 5;
const PTE_DIRTY: u64 = 1 << 6;
const PTE_GLOBAL: u64 = 1 << 8;
const PTE_COW: u64 = 1 << 9; // Copy-on-write marker (software bit)

const PTE_KERNEL: u64 = PTE_PRESENT | PTE_WRITABLE | PTE_ACCESSED | PTE_DIRTY | PTE_GLOBAL;
const PTE_USER_RW: u64 = PTE_PRESENT | PTE_WRITABLE | PTE_USER | PTE_ACCESSED | PTE_DIRTY;
const PTE_USER_RO: u64 = PTE_PRESENT | PTE_USER | PTE_ACCESSED;

/// A page table level (512 entries = 4 KB)
type PageTable = [u64; 512];

/// Root of a task's page tables (PML4 physical address)
#[derive(Clone, Copy)]
pub struct TaskPageTables {
    pub pml4_phys: usize,
    pub pml4_virt: *mut PageTable,
}

/// Allocate a zeroed page table page and return (phys, virt)
fn alloc_pt_page() -> (usize, *mut PageTable) {
    let phys = crate::mm::frame_alloc::alloc_frame()
        .expect("Failed to allocate page table page");
    let virt = phys as *mut PageTable;
    unsafe {
        core::ptr::write_bytes(virt as *mut u8, 0, PAGE_SIZE);
    }
    (phys, virt)
}

/// Set up identity-mapped kernel page tables.
/// Returns (pml4_phys, pml4_virt) for the kernel template.
pub fn setup_kernel_page_tables() -> (usize, *mut PageTable) {
    let (pml4_phys, pml4_virt) = alloc_pt_page();
    let pml4 = unsafe { &mut *pml4_virt };

    // Map kernel region: KERNEL_VADDR .. KERNEL_VADDR + KERNEL_SIZE
    let kernel_start = KERNEL_VADDR as usize;

    // PML4 index for kernel region (0x10000000 >> 39 = 0)
    let pml4_idx = ((KERNEL_VADDR >> 39) & 0x1FF) as usize;

    // Allocate PDPT
    let (pdpt_phys, pdpt_virt) = alloc_pt_page();
    pml4[pml4_idx] = pdpt_phys as u64 | PTE_KERNEL;

    let pdpt = unsafe { &mut *pdpt_virt };

    // PDPT index
    let pdpt_idx = ((KERNEL_VADDR >> 30) & 0x1FF) as usize;

    // Use 2MB pages for kernel (PD entry with PS=1)
    let mut remaining = KERNEL_SIZE;
    let mut pd_idx = pdpt_idx;
    let mut pd_offset_in_region = 0usize;

    while remaining > 0 {
        let (pd_phys, pd_virt) = alloc_pt_page();
        pdpt[pd_idx] = pd_phys as u64 | PTE_KERNEL;

        let pd = unsafe { &mut *pd_virt };

        let entries_needed = (remaining + 0x1FFFFF) / 0x200000; // 2MB per PD
        let entries_to_use = entries_needed.min(512);

        for i in 0..entries_to_use {
            let page_phys = kernel_start + pd_offset_in_region + i * 0x200000;
            pd[i] = page_phys as u64 | PTE_KERNEL | (1 << 7); // PS=1 for 2MB page
        }

        pd_idx += 1;
        pd_offset_in_region += entries_to_use * 0x200000;
        remaining = remaining.saturating_sub(entries_to_use * 0x200000);
    }

    // Map MMIO region (framebuffer, APIC, etc.) at 0xC0000000..0x100000000
    // QEMU framebuffer is typically around 0xFD000000
    // Local APIC is at 0xFEE00000
    let mmio_start: u64 = 0xC0000000;
    let mmio_end: u64 = 0x100000000;

    let mmio_pml4_idx = ((mmio_start >> 39) & 0x1FF) as usize;
    // This will be PML4[0] since mmio_start < 512GB
    // Same PML4 entry as kernel, but different PDPT entries
    let (pdpt_phys, pdpt_virt) = if pml4[mmio_pml4_idx] != 0 {
        // Reuse existing PDPT (from kernel mapping)
        let pdpt_phys = (pml4[mmio_pml4_idx] & !0xFFF) as usize;
        let pdpt_virt = pdpt_phys as *mut PageTable;
        (pdpt_phys, pdpt_virt)
    } else {
        let (pdpt_phys, pdpt_virt) = alloc_pt_page();
        pml4[mmio_pml4_idx] = pdpt_phys as u64 | PTE_KERNEL;
        (pdpt_phys, pdpt_virt)
    };

    let pdpt = unsafe { &mut *pdpt_virt };
    let mmio_pdpt_idx = ((mmio_start >> 30) & 0x1FF) as usize;
    let mmio_pdpt_end_idx = (((mmio_end - 1) >> 30) & 0x1FF) as usize;

    for i in mmio_pdpt_idx..=mmio_pdpt_end_idx {
        if i < 512 && pdpt[i] == 0 {
            let phys_1gb = (i as u64) << 30;
            // 1GB page at PDPT level (PS=1 bit 7) - supervisor only
            pdpt[i] = phys_1gb | PTE_KERNEL | (1 << 7);
        }
    }

    log::info!("paging: kernel page tables at pml4_phys={:#x}", pml4_phys);
    (pml4_phys, pml4_virt)
}

/// Create per-task page tables by copying kernel half
pub fn create_task_page_tables(kernel_pml4_phys: usize, kernel_pml4_virt: *mut PageTable) -> TaskPageTables {
    let (pml4_phys, pml4_virt) = alloc_pt_page();
    let pml4 = unsafe { &mut *pml4_virt };

    // Copy kernel half (entries 256-511) from kernel PML4
    let kernel_pml4 = unsafe { &*kernel_pml4_virt };
    for i in 256..512 {
        pml4[i] = kernel_pml4[i];
    }

    // Lower half (entries 0-255) is zeroed (unmapped) for user space
    // User space mappings will be added on demand

    TaskPageTables {
        pml4_phys,
        pml4_virt,
    }
}

/// Map a user-space page in a task's page tables
pub fn map_user_page(task_pt: &mut TaskPageTables, vaddr: u64, writable: bool) -> Option<usize> {
    let pml4 = unsafe { &mut *task_pt.pml4_virt };

    let pml4_idx = ((vaddr >> 39) & 0x1FF) as usize;
    if pml4[pml4_idx] == 0 {
        let (pdpt_phys, pdpt_virt) = alloc_pt_page();
        pml4[pml4_idx] = pdpt_phys as u64 | PTE_USER_RW;
        // Zero the new PDPT
        unsafe { core::ptr::write_bytes(pdpt_virt as *mut u8, 0, PAGE_SIZE); }
    }

    let pdpt_phys = (pml4[pml4_idx] & !0xFFF) as usize;
    let pdpt_virt = pdpt_phys as *mut PageTable;
    let pdpt = unsafe { &mut *pdpt_virt };

    let pdpt_idx = ((vaddr >> 30) & 0x1FF) as usize;
    if pdpt[pdpt_idx] == 0 {
        let (pd_phys, pd_virt) = alloc_pt_page();
        pdpt[pdpt_idx] = pd_phys as u64 | PTE_USER_RW;
        unsafe { core::ptr::write_bytes(pd_virt as *mut u8, 0, PAGE_SIZE); }
    }

    let pd_phys = (pdpt[pdpt_idx] & !0xFFF) as usize;
    let pd_virt = pd_phys as *mut PageTable;
    let pd = unsafe { &mut *pd_virt };

    let pd_idx = ((vaddr >> 21) & 0x1FF) as usize;
    if pd[pd_idx] == 0 {
        let (pt_phys, pt_virt) = alloc_pt_page();
        pd[pd_idx] = pt_phys as u64 | PTE_USER_RW;
        unsafe { core::ptr::write_bytes(pt_virt as *mut u8, 0, PAGE_SIZE); }
    }

    let pt_phys = (pd[pd_idx] & !0xFFF) as usize;
    let pt_virt = pt_phys as *mut PageTable;
    let pt = unsafe { &mut *pt_virt };

    let pt_idx = ((vaddr >> 12) & 0x1FF) as usize;

    // Allocate physical frame for the page
    let frame_phys = crate::mm::frame_alloc::alloc_frame()?;
    let flags = if writable { PTE_USER_RW } else { PTE_USER_RO };
    pt[pt_idx] = frame_phys as u64 | flags;

    Some(frame_phys)
}

/// Unmap a user-space page
pub fn unmap_user_page(task_pt: &mut TaskPageTables, vaddr: u64) {
    let pml4 = unsafe { &mut *task_pt.pml4_virt };
    let pml4_idx = ((vaddr >> 39) & 0x1FF) as usize;
    if pml4[pml4_idx] == 0 { return; }

    let pdpt_virt = ((pml4[pml4_idx] & !0xFFF) as usize) as *mut PageTable;
    let pdpt = unsafe { &mut *pdpt_virt };
    let pdpt_idx = ((vaddr >> 30) & 0x1FF) as usize;
    if pdpt[pdpt_idx] == 0 { return; }

    let pd_virt = ((pdpt[pdpt_idx] & !0xFFF) as usize) as *mut PageTable;
    let pd = unsafe { &mut *pd_virt };
    let pd_idx = ((vaddr >> 21) & 0x1FF) as usize;
    if pd[pd_idx] == 0 { return; }

    let pt_virt = ((pd[pd_idx] & !0xFFF) as usize) as *mut PageTable;
    let pt = unsafe { &mut *pt_virt };
    let pt_idx = ((vaddr >> 12) & 0x1FF) as usize;

    if pt[pt_idx] != 0 {
        let phys = (pt[pt_idx] & !0xFFF) as usize;
        crate::mm::frame_alloc::free_frame(phys);
        pt[pt_idx] = 0;
    }
}

/// Load page tables (set CR3)
pub unsafe fn load_page_tables(pml4_phys: usize) {
    core::arch::asm!("mov cr3, {}", in(reg) pml4_phys, options(nostack));
}

/// Read current CR3
pub unsafe fn read_cr3() -> usize {
    let cr3: usize;
    core::arch::asm!("mov {}, cr3", out(reg) cr3);
    cr3
}

/// Enable paging on x86_64
/// Must be called after setup_kernel_page_tables()
/// The kernel is identity-mapped, so this is safe.
pub unsafe fn enable_paging(pml4_phys: usize) {
    // Set CR3 to kernel PML4
    core::arch::asm!("mov cr3, {}", in(reg) pml4_phys, options(nostack));

    // Enable PAE (bit 5 of CR4)
    let mut cr4: u64;
    core::arch::asm!("mov {}, cr4", out(reg) cr4);
    cr4 |= 1 << 5; // PAE
    cr4 |= 1 << 4; // PSE (for 2MB pages)
    cr4 |= 1 << 7; // PGE (global pages)
    core::arch::asm!("mov cr4, {}", in(reg) cr4);

    // Enable paging (bit 31 of CR0) and write protect (bit 16)
    let mut cr0: u64;
    core::arch::asm!("mov {}, cr0", out(reg) cr0);
    cr0 |= 1 << 31; // PG
    cr0 |= 1 << 16; // WP (write-protect supervisor pages from user mode)
    core::arch::asm!("mov cr0, {}", in(reg) cr0);

    log::info!("paging: enabled, CR3={:#x}", pml4_phys);
}

/// Global kernel page table root (set once at boot)
static mut KERNEL_PML4_PHYS: usize = 0;
static mut KERNEL_PML4_VIRT: *mut PageTable = core::ptr::null_mut();

pub fn kernel_page_tables() -> (usize, *mut PageTable) {
    unsafe { (KERNEL_PML4_PHYS, KERNEL_PML4_VIRT) }
}

/// Invoked by mm::init to set up paging
pub fn init_paging() {
    unsafe {
        let (phys, virt) = setup_kernel_page_tables();
        KERNEL_PML4_PHYS = phys;
        KERNEL_PML4_VIRT = virt;
        enable_paging(phys);
    }
}

fn pic_remap() {
    unsafe {
        // ICW1
        core::arch::asm!("out dx, al", in("dx") 0x20u16, in("al") 0x11u8);
        core::arch::asm!("out dx, al", in("dx") 0xA0u16, in("al") 0x11u8);
        // ICW2
        core::arch::asm!("out dx, al", in("dx") 0x21u16, in("al") 0x20u8);
        core::arch::asm!("out dx, al", in("dx") 0xA1u16, in("al") 0x28u8);
        // ICW3
        core::arch::asm!("out dx, al", in("dx") 0x21u16, in("al") 0x04u8);
        core::arch::asm!("out dx, al", in("dx") 0xA1u16, in("al") 0x02u8);
        // ICW4
        core::arch::asm!("out dx, al", in("dx") 0x21u16, in("al") 0x01u8);
        core::arch::asm!("out dx, al", in("dx") 0xA1u16, in("al") 0x01u8);
        // Mask all except IRQ0, IRQ1, IRQ12
        core::arch::asm!("out dx, al", in("dx") 0x21u16, in("al") 0xECu8); // IRQ2 (cascade) + IRQ12 unmasked
        core::arch::asm!("out dx, al", in("dx") 0xA1u16, in("al") 0xEFu8); // IRQ12 on PIC2
    }
}

pub fn init(_boot_info: &mut BootInfo) {
    log::info!("x86_64 arch init: paging + GDT + IDT + PIC + interrupts");

    // Set up paging
    init_paging();

    unsafe {
        // Load GDT (needed for ring 3 transitions)
        load_gdt();

        // Remap PIC
        pic_remap();

        // Set up IDT entries
        set_idt_entry(32, irq0_handler as usize, 0x08, 0x8E); // IRQ0 timer
        set_idt_entry(33, irq1_handler as usize, 0x08, 0x8E); // IRQ1 keyboard
        set_idt_entry(44, irq12_handler as usize, 0x08, 0x8E); // IRQ12 mouse
        set_idt_entry(8, double_fault_handler as usize, 0x08, 0x8E);
        set_idt_entry(14, page_fault_handler as usize, 0x08, 0x8E); // Page fault (#PF)

        // Load IDT
        let idtr = Idtr {
            limit: (core::mem::size_of::<IdtEntry>() * IDT_ENTRIES - 1) as u16,
            base: core::ptr::addr_of!(IDT) as u64,
        };
        core::arch::asm!("lidt [{0}]", in(reg) &idtr, options(nostack));

        // Enable interrupts
        core::arch::asm!("sti");

        // Set up syscall entry (LSTAR MSR = 0xC0000082)
        // Ring 3 → ring 0 via syscall instruction
        // STAR MSR (0xC0000081): SYSRET CS/SS (bits 47:32 = ring 3 CS+16, bits 31:16 = ring 3 SS+16)
        // For ring 3: CS = 0x1B (0x18 | 3), SS = 0x23 (0x20 | 3)
        // STAR[47:32] = 0x1B, STAR[31:16] = 0x23
        // We set STAR to (0x2300000000u64 | 0x1B000000000000u64) but use kernel CS/SS for return
        // ECX = return RIP, R11 = return RFLAGS after SYSRET
        let star_val: u64 = (0x1B_u64 << 48) | (0x08_u64 << 32); // ring3 CS=0x1B, kernel CS=0x08
        core::arch::asm!("wrmsr", in("ecx") 0xC0000081_u32, in("eax") (star_val & 0xFFFF_FFFF) as u32, in("edx") (star_val >> 32) as u32);

        // LSTAR = syscall entry point (kernel RIP)
        extern "C" { fn syscall_entry(); }
        let lstar_val = syscall_entry as u64;
        core::arch::asm!("wrmsr", in("ecx") 0xC0000082_u32, in("eax") (lstar_val & 0xFFFF_FFFF) as u32, in("edx") (lstar_val >> 32) as u32);

        // FMASK = clear IF (disable interrupts on syscall entry) — we want them enabled, so 0
        core::arch::asm!("wrmsr", in("ecx") 0xC0000084_u32, in("eax") 0_u32, in("edx") 0_u32);
    }
}

pub fn halt_loop() -> ! {
    loop { unsafe { core::arch::asm!("hlt") } }
}

pub unsafe fn jump_to_user(entry: usize, stack_top: usize, pml4_phys: usize) -> ! {
    // Switch to task's page tables
    core::arch::asm!("mov cr3, {}", in(reg) pml4_phys, options(nostack));

    // IRET to ring 3
    // CS = 0x1B (ring 3 code segment), SS = 0x23 (ring 3 data segment)
    // RFLAGS = 0x202 (interrupts enabled)
    core::arch::asm!(
        "push {ss}",
        "push {rsp}",
        "push 0x202",
        "push {cs}",
        "push {entry}",
        "iretq",
        entry = in(reg) entry,
        rsp = in(reg) stack_top,
        cs = in(reg) 0x1Bu64, // ring 3 code: GDT index 3, RPL=3 (0x18 | 3 = 0x1B)
        ss = in(reg) 0x23u64, // ring 3 data: GDT index 4, RPL=3 (0x20 | 3 = 0x23)
        options(noreturn)
    );
}
