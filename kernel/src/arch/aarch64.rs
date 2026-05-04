//! aarch64 architecture support — GIC, exception vectors, IRQ dispatch
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

/// Initialize aarch64 architecture support
pub fn init(_boot_info: &mut BootInfo) {
    log::info!("aarch64 arch init: GIC + exceptions + interrupts");

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

    log::info!("aarch64: exceptions and GIC initialized");
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
                #[cfg(feature = "bsp_qemu")]
                crate::drivers::pl050_kmi::handle_kmi0_irq();
            }
            KMI1_IRQ => {
                #[cfg(feature = "bsp_qemu")]
                crate::drivers::pl050_kmi::handle_kmi1_irq();
            }
            _ => {
                log::warn!("aarch64: unknown IRQ {}", irq_id);
            }
        }
        gic_eoi(irq_id);
    }
}

/// Synchronous exception handler
#[no_mangle]
pub extern "C" fn handle_sync_el1() {
    log::error!("aarch64: Synchronous exception at EL1");
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
        log::error!("ESR_EL1={:#018x} FAR_EL1={:#018x} ELR_EL1={:#018x}", esr, far, elr);
    }
    panic!("Unhandled synchronous exception");
}

/// Lower EL synchronous exception handler (for syscalls from EL0)
#[no_mangle]
pub extern "C" fn handle_sync_lower_el() {
    log::error!("aarch64: Synchronous exception from lower EL");
    unsafe {
        let esr: u64;
        asm!(
            "mrs {0}, esr_el1",
            out(reg) esr,
            options(nostack),
        );
        log::error!("ESR_EL1={:#018x}", esr);
    }
    panic!("Unhandled lower EL exception");
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

pub unsafe fn jump_to_user(entry: usize, stack_top: usize) -> ! {
    asm!(
        "mov sp, {stack}",
        "br {entry}",
        stack = in(reg) stack_top,
        entry = in(reg) entry,
        options(noreturn)
    );
}
