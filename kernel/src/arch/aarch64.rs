//! aarch64 architecture support — GIC, exception vectors, IRQ dispatch, paging
use crate::BootInfo;
use core::arch::asm;

/// GICv2 Distributor base address (QEMU virt machine)
const GICD_BASE: *mut u32 = 0x08000000 as *mut u32;
/// GICv2 CPU Interface base address
const GICC_BASE: *mut u32 = 0x08010000 as *mut u32;

/// GIC register offsets
const GICD_CTLR: u32 = 0x000;
const GICD_TYPER: u32 = 0x004;
const GICD_ISENABLER: u32 = 0x100;
const GICD_IPRIORITYR: u32 = 0x400;
const GICC_CTLR: u32 = 0x000;
const GICC_PMR: u32 = 0x004;
const GICC_IAR: u32 = 0x00c;
const GICC_EOIR: u32 = 0x010;

/// IRQ IDs for QEMU virt machine (SPIs start at 32)
pub const UART_IRQ: u32 = 33; // PL011 UART
pub const KMI0_IRQ: u32 = 34; // PL050 KMI0 (keyboard)
pub const KMI1_IRQ: u32 = 35; // PL050 KMI1 (mouse)

/// Read a GICD register
unsafe fn gicd_read(offset: u32) -> u32 {
    core::ptr::read_volatile(GICD_BASE.byte_add(offset as usize) as *const u32)
}

/// Write a GICD register
unsafe fn gicd_write(offset: u32, val: u32) {
    core::ptr::write_volatile(GICD_BASE.byte_add(offset as usize) as *mut u32, val)
}

/// Read a GICC register
unsafe fn gicc_read(offset: u32) -> u32 {
    core::ptr::read_volatile(GICC_BASE.byte_add(offset as usize) as *const u32)
}

/// Write a GICC register
unsafe fn gicc_write(offset: u32, val: u32) {
    core::ptr::write_volatile(GICC_BASE.byte_add(offset as usize) as *mut u32, val)
}

/// Initialize GICv2
fn gic_init() {
    unsafe {
        // Disable distributor
        gicd_write(GICD_CTLR, 0);

        // Get number of interrupt lines
        let typer = gicd_read(GICD_TYPER);
        let num_interrupts = ((typer & 0x1F) + 1) * 32;

        // Disable all interrupts and set priority
        let num_regs = (num_interrupts + 31) / 32;
        for i in 0..num_regs {
            // Disable all
            gicd_write(GICD_ISENABLER + i * 4, 0);
            // Set priority (0xA0 = low priority)
            for j in 0..8 {
                gicd_write(GICD_IPRIORITYR + i * 32 + j * 4, 0xA0A0A0A0);
            }
        }

        // Enable UART and KMI IRQs (SPI 1, 2, 3)
        let enable_mask = (1 << (UART_IRQ - 32)) | (1 << (KMI0_IRQ - 32)) | (1 << (KMI1_IRQ - 32));
        gicd_write(GICD_ISENABLER, enable_mask);

        // Set priority for our IRQs to 0x40 (higher than default 0xA0)
        for irq in [UART_IRQ, KMI0_IRQ, KMI1_IRQ] {
            let reg_offset = GICD_IPRIORITYR + (irq / 8) * 4;
            let shift = (irq % 8) * 4;
            let mut val = gicd_read(reg_offset);
            val &= !(0xFF << shift);
            val |= 0x40 << shift;
            gicd_write(reg_offset, val);
        }

        // Enable distributor (Group 0 enabled)
        gicd_write(GICD_CTLR, 1);

        // CPU Interface
        gicc_write(GICC_PMR, 0xFF); // Allow all priorities
        gicc_write(GICC_CTLR, 1);   // Enable
    }
}

/// Read IRQ ID from GIC CPU Interface
fn gic_ack_irq() -> Option<u32> {
    unsafe {
        let iar = gicc_read(GICC_IAR);
        let irq_id = iar & 0x3FF;
        if irq_id >= 1022 {
            None
        } else {
            Some(irq_id)
        }
    }
}

/// Signal end-of-interrupt
fn gic_eoi(irq_id: u32) {
    unsafe {
        gicc_write(GICC_EOIR, irq_id);
    }
}

// ============================================================
// Paging — aarch64 4-level page tables (L0 → L1 → L2 → L3)
// ============================================================

const PAGE_SIZE: usize = 4096;
const KERNEL_VADDR: u64 = 0x40000000;
const KERNEL_SIZE: usize = 64 * 1024 * 1024; // 64 MB

/// Page table entry flags
const PTE_VALID: u64 = 1 << 0;
const PTE_TABLE: u64 = 3; // Block 1, Table 3
const PTE_AP_RO: u64 = 2 << 6; // AP[2:1] = 10: read-only for EL0, read/write for EL1
const PTE_AP_RW: u64 = 0 << 6; // AP[2:1] = 00: read/write for EL0 and EL1
const PTE_AP_KERNEL: u64 = 1 << 6; // AP[2:1] = 01: read/write for EL1 only, EL0 no access
const PTE_SH_INNER: u64 = 3 << 8; // Inner shareable
const PTE_AF: u64 = 1 << 10; // Access flag
const PTE_NG: u64 = 1 << 11; // Not global
const PTE_XN: u64 = 1 << 54; // Execute never

/// Kernel page flags: EL1 R/W, inner shareable, execute allowed
const PTE_KERNEL_RW: u64 = PTE_VALID | PTE_AP_KERNEL | PTE_SH_INNER | PTE_AF | PTE_TABLE;
/// Kernel block (1GB) flags
const PTE_KERNEL_BLOCK: u64 = PTE_VALID | PTE_AP_KERNEL | PTE_SH_INNER | PTE_AF;

/// User page flags: EL0/EL1 R/W, execute never
const PTE_USER_RW: u64 = PTE_VALID | PTE_AP_RW | PTE_SH_INNER | PTE_AF | PTE_TABLE;
const PTE_USER_PAGE: u64 = PTE_VALID | PTE_AP_RW | PTE_SH_INNER | PTE_AF;

/// A page table level (512 entries = 4 KB)
type PageTable = [u64; 512];

/// Root of a task's page tables (TTBR0 physical address)
#[derive(Clone, Copy)]
pub struct TaskPageTables {
    pub ttbr0_phys: usize,
    pub ttbr0_virt: *mut PageTable,
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

/// Set up kernel page tables. Returns (ttbr0_phys, ttbr0_virt).
/// The kernel is identity-mapped at KERNEL_VADDR.
pub fn setup_kernel_page_tables() -> (usize, *mut PageTable) {
    let (l0_phys, l0_virt) = alloc_pt_page();
    let l0 = unsafe { &mut *l0_virt };

    // Map kernel region: KERNEL_VADDR .. KERNEL_VADDR + KERNEL_SIZE
    let l0_idx = ((KERNEL_VADDR >> 39) & 0x1FF) as usize;

    // L0 -> L1
    let (l1_phys, l1_virt) = alloc_pt_page();
    l0[l0_idx] = l1_phys as u64 | PTE_TABLE;

    let l1 = unsafe { &mut *l1_virt };
    let l1_idx = ((KERNEL_VADDR >> 30) & 0x1FF) as usize;

    // Map kernel with 1GB block (covers 0x0 to 0x40000000)
    l1[l1_idx] = 0u64 | PTE_KERNEL_BLOCK;

    // Map framebuffer region (0xF0000000..0x100000000)
    let fb_start: u64 = 0xF0000000;
    let fb_l0_idx = ((fb_start >> 39) & 0x1FF) as usize;
    if l0[fb_l0_idx] == 0 {
        let (l1_phys, l1_virt) = alloc_pt_page();
        l0[fb_l0_idx] = l1_phys as u64 | PTE_TABLE;
        let l1 = unsafe { &mut *l1_virt };
        let fb_l1_idx = ((fb_start >> 30) & 0x1FF) as usize;
        l1[fb_l1_idx] = 0xF0000000u64 | PTE_KERNEL_BLOCK;
    }

    // Map GIC and other MMIO regions (0x08000000 for GIC, 0x09000000 for PL011, etc.)
    let mmio_start: u64 = 0x08000000;
    let mmio_l0_idx = ((mmio_start >> 39) & 0x1FF) as usize;
    if l0[mmio_l0_idx] == 0 {
        let (l1_phys, l1_virt) = alloc_pt_page();
        l0[mmio_l0_idx] = l1_phys as u64 | PTE_TABLE;
        let l1 = unsafe { &mut *l1_virt };
        let mmio_l1_idx = ((mmio_start >> 30) & 0x1FF) as usize;
        l1[mmio_l1_idx] = 0x08000000u64 | PTE_KERNEL_BLOCK;
    }

    log::info!("paging: kernel page tables at ttbr0={:#x}", l0_phys);
    (l0_phys, l0_virt)
}

/// Create per-task page tables by copying kernel half
pub fn create_task_page_tables(kernel_ttbr0_phys: usize, kernel_ttbr0_virt: *mut PageTable) -> TaskPageTables {
    let (l0_phys, l0_virt) = alloc_pt_page();
    let l0 = unsafe { &mut *l0_virt };

    // Copy entire kernel page table (kernel is in upper half for identity mapping)
    let kernel_l0 = unsafe { &*kernel_ttbr0_virt };
    for i in 0..512 {
        l0[i] = kernel_l0[i];
    }

    TaskPageTables {
        ttbr0_phys: l0_phys,
        ttbr0_virt: l0_virt,
    }
}

/// Map a user-space page in a task's page tables
pub fn map_user_page(task_pt: &mut TaskPageTables, vaddr: u64, writable: bool) -> Option<usize> {
    let l0 = unsafe { &mut *task_pt.ttbr0_virt };

    let l0_idx = ((vaddr >> 39) & 0x1FF) as usize;
    if l0[l0_idx] == 0 {
        let (l1_phys, l1_virt) = alloc_pt_page();
        l0[l0_idx] = l1_phys as u64 | PTE_USER_RW;
        unsafe { core::ptr::write_bytes(l1_virt as *mut u8, 0, PAGE_SIZE); }
    }

    let l1_virt = ((l0[l0_idx] & !0xFFF) as usize) as *mut PageTable;
    let l1 = unsafe { &mut *l1_virt };

    let l1_idx = ((vaddr >> 30) & 0x1FF) as usize;
    if l1[l1_idx] == 0 || (l1[l1_idx] & 0x3) != PTE_TABLE {
        let (l2_phys, l2_virt) = alloc_pt_page();
        l1[l1_idx] = l2_phys as u64 | PTE_USER_RW;
        unsafe { core::ptr::write_bytes(l2_virt as *mut u8, 0, PAGE_SIZE); }
    }

    let l2_virt = ((l1[l1_idx] & !0xFFF) as usize) as *mut PageTable;
    let l2 = unsafe { &mut *l2_virt };

    let l2_idx = ((vaddr >> 21) & 0x1FF) as usize;
    if l2[l2_idx] == 0 || (l2[l2_idx] & 0x3) != PTE_TABLE {
        let (l3_phys, l3_virt) = alloc_pt_page();
        l2[l2_idx] = l3_phys as u64 | PTE_USER_RW;
        unsafe { core::ptr::write_bytes(l3_virt as *mut u8, 0, PAGE_SIZE); }
    }

    let l3_virt = ((l2[l2_idx] & !0xFFF) as usize) as *mut PageTable;
    let l3 = unsafe { &mut *l3_virt };

    let l3_idx = ((vaddr >> 12) & 0x1FF) as usize;

    let frame_phys = crate::mm::frame_alloc::alloc_frame()?;
    let flags = if writable { PTE_USER_PAGE } else { PTE_VALID | PTE_AP_RO | PTE_SH_INNER | PTE_AF };
    l3[l3_idx] = frame_phys as u64 | flags;

    Some(frame_phys)
}

/// Unmap a user-space page
pub fn unmap_user_page(task_pt: &mut TaskPageTables, vaddr: u64) {
    let l0 = unsafe { &mut *task_pt.ttbr0_virt };
    let l0_idx = ((vaddr >> 39) & 0x1FF) as usize;
    if l0[l0_idx] == 0 { return; }

    let l1_virt = ((l0[l0_idx] & !0xFFF) as usize) as *mut PageTable;
    let l1 = unsafe { &mut *l1_virt };
    let l1_idx = ((vaddr >> 30) & 0x1FF) as usize;
    if l1[l1_idx] == 0 { return; }

    let l2_virt = ((l1[l1_idx] & !0xFFF) as usize) as *mut PageTable;
    let l2 = unsafe { &mut *l2_virt };
    let l2_idx = ((vaddr >> 21) & 0x1FF) as usize;
    if l2[l2_idx] == 0 { return; }

    let l3_virt = ((l2[l2_idx] & !0xFFF) as usize) as *mut PageTable;
    let l3 = unsafe { &mut *l3_virt };
    let l3_idx = ((vaddr >> 12) & 0x1FF) as usize;

    if l3[l3_idx] != 0 {
        let phys = (l3[l3_idx] & !0xFFF) as usize;
        crate::mm::frame_alloc::free_frame(phys);
        l3[l3_idx] = 0;
    }
}

/// Load page tables (set TTBR0_EL1) and invalidate TLB
pub unsafe fn load_page_tables(ttbr0_phys: usize) {
    asm!(
        "msr ttbr0_el1, {ttbr}",
        "isb",
        "tlbi vmalle1",
        "dsb nsh",
        "isb",
        ttbr = in(reg) ttbr0_phys as u64,
        options(nostack),
    );
}

/// Read current TTBR0_EL1
pub unsafe fn read_ttbr0() -> usize {
    let ttbr: u64;
    asm!("mrs {0}, ttbr0_el1", out(reg) ttbr);
    ttbr as usize
}

/// Global kernel page table root (set once at boot)
static mut KERNEL_TTBR0_PHYS: usize = 0;
static mut KERNEL_TTBR0_VIRT: *mut PageTable = core::ptr::null_mut();

pub fn kernel_page_tables() -> (usize, *mut PageTable) {
    unsafe { (KERNEL_TTBR0_PHYS, KERNEL_TTBR0_VIRT) }
}

/// Initialize aarch64 architecture support
pub fn init(boot_info: &mut BootInfo) {
    log::info!("aarch64 arch init: paging + GIC + exceptions + interrupts");

    // Set up kernel page tables
    unsafe {
        let (phys, virt) = setup_kernel_page_tables();
        KERNEL_TTBR0_PHYS = phys;
        KERNEL_TTBR0_VIRT = virt;
        load_page_tables(phys);
    }

    // Set up exception vector table
    unsafe {
        extern "C" {
            fn __vector_table_el1();
        }
        asm!(
            "msr vbar_el1, {vbar}",
            vbar = in(reg) __vector_table_el1,
            options(nostack),
        );
    }

    // Initialize GICv2
    gic_init();

    // Enable interrupts (DAIF register, clear I bit)
    unsafe {
        asm!("msr daifclr, #2", options(nostack)); // Unmask IRQ
    }

    log::info!("aarch64: exceptions, paging, and GIC initialized");
}

/// IRQ handler called from assembly vector table
#[no_mangle]
pub extern "C" fn handle_irq_el1() {
    if let Some(irq_id) = gic_ack_irq() {
        match irq_id {
            UART_IRQ => {
                crate::drivers::uart::handle_uart_irq();
            }
            KMI0_IRQ => {
                crate::drivers::pl050_kmi::handle_kmi0_irq();
            }
            KMI1_IRQ => {
                crate::drivers::pl050_kmi::handle_kmi1_irq();
            }
            _ => {
                log::warn!("aarch64: unknown IRQ {}", irq_id);
            }
        }
        gic_eoi(irq_id);
    }
}

/// Synchronous exception handler (EL1 with SPx)
#[no_mangle]
pub extern "C" fn handle_sync_el1() {
    unsafe {
        let esr: u64;
        let far: u64;
        let elr: u64;
        asm!(
            "mrs {0}, esr_el1",
            "mrs {1}, far_el1",
            "mrs {2}, elr_el1",
            out(reg) esr,
            out(reg) far,
            out(reg) elr,
            options(nostack),
        );
        let ec = (esr >> 26) & 0x3F;
        let iss = esr & 0xFFFFFF;

        // EC 0x21 = Data Abort from lower EL
        // EC 0x25 = Data Abort from current EL
        if ec == 0x25 {
            log::error!("aarch64: Data Abort at EL1, FAR={:#x}, ELR={:#x}, ESR={:#x}", far, elr, esr);
            let dfs = iss & 0x3F;
            let wnr = (iss >> 6) & 1;
            log::error!("  DFS={:#x} (fault status), WnR={} ({})", dfs, wnr, if wnr != 0 { "write" } else { "read" });
        } else {
            log::error!("aarch64: Synchronous exception at EL1, EC={:#x}, FAR={:#x}, ELR={:#x}, ESR={:#x}",
                ec, far, elr, esr);
        }
    }
    panic!("Unhandled synchronous exception at EL1");
}

/// Synchronous exception from lower EL (EL0) — syscall handler
#[no_mangle]
pub extern "C" fn handle_sync_lower_el() {
    unsafe {
        let esr: u64;
        let far: u64;
        let elr: u64;
        asm!(
            "mrs {0}, esr_el1",
            "mrs {1}, far_el1",
            "mrs {2}, elr_el1",
            out(reg) esr,
            out(reg) far,
            out(reg) elr,
            options(nostack),
        );

        let ec = (esr >> 26) & 0x3F;

        if ec == 0x15 {
            // SVC instruction from EL0 — syscall
            handle_syscall(elr);
        } else if ec == 0x24 || ec == 0x20 {
            // Data Abort from lower EL
            log::error!("aarch64: Data Abort from EL0, FAR={:#x}, ELR={:#x}, ESR={:#x}", far, elr, esr);
            // For now, kill the task
            panic!("User space data abort");
        } else {
            log::error!("aarch64: Synchronous exception from EL0, EC={:#x}, FAR={:#x}, ELR={:#x}, ESR={:#x}",
                ec, far, elr, esr);
            panic!("Unhandled lower EL exception");
        }
    }
}

/// Handle syscall from EL0
unsafe fn handle_syscall(elr: u64) {
    let x0: u64;
    let x1: u64;
    let x2: u64;
    let x3: u64;
    let x8: u64;
    let sp: u64;
    asm!(
        "mov {x0}, x0",
        "mov {x1}, x1",
        "mov {x2}, x2",
        "mov {x3}, x3",
        "mov {x8}, x8",
        "mov {sp}, sp",
        x0 = out(reg) x0,
        x1 = out(reg) x1,
        x2 = out(reg) x2,
        x3 = out(reg) x3,
        x8 = out(reg) x8,
        sp = out(reg) sp,
        options(nostack),
    );

    let result = crate::syscalls::dispatch(x8 as usize, x0 as usize, x1 as usize, x2 as usize);

    // Set return value in x0 and advance past SVC
    asm!(
        "mov x0, {result}",
        "msr elr_el1, {elr}",
        result = in(reg) result as u64,
        elr = in(reg) elr + 4, // SVC is 4 bytes
        options(nostack),
    );
}

/// FIQ handler
#[no_mangle]
pub extern "C" fn handle_fiq_el1() {
    log::error!("aarch64: FIQ exception");
    panic!("Unhandled FIQ");
}

/// SError handler
#[no_mangle]
pub extern "C" fn handle_serror_el1() {
    log::error!("aarch64: SError exception");
    panic!("Unhandled SError");
}

pub fn halt_loop() -> ! {
    loop {
        unsafe { asm!("wfe") }
    }
}

pub unsafe fn jump_to_user(entry: usize, stack_top: usize, ttbr0_phys: usize) -> ! {
    // Switch to task's page tables
    load_page_tables(ttbr0_phys);

    // Prepare SPSR_EL1 for EL0 execution (AArch64, SP0, DAIF set, M=0)
    let spsr = (0x0u64)         // EL0t
        | (0x3 << 6)            // DAIF: set all (mask IRQ, FIQ, SError, Debug)
        | (0x5 << 2);           // Set M bits to EL0t

    asm!(
        "msr spsr_el1, {spsr}",
        "msr elr_el1, {elr}",
        "msr sp_el0, {sp}",
        "eret",
        spsr = in(reg) spsr,
        elr = in(reg) entry as u64,
        sp = in(reg) stack_top as u64,
        options(noreturn),
    );
}
