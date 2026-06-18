//! x86_64 interrupt handling.

#![allow(static_mut_refs)]

use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};
use x86_64::instructions::port::Port;

/// Programmable Interrupt Controller constants.
const PIC1_COMMAND: u16 = 0x20;
const PIC1_DATA: u16 = 0x21;
const PIC2_COMMAND: u16 = 0xA0;
const PIC2_DATA: u16 = 0xA1;
const PIC_EOI: u8 = 0x20;

static mut IDT: InterruptDescriptorTable = InterruptDescriptorTable::new();

/// Initialize the IDT and remap the PIC.
pub fn init() {
    unsafe {
        IDT.breakpoint.set_handler_fn(breakpoint_handler);
        IDT.double_fault.set_handler_fn(double_fault_handler);
        IDT.page_fault.set_handler_fn(page_fault_handler);

        // IRQ0: timer
        IDT[32].set_handler_fn(timer_interrupt_handler);
        // IRQ1: keyboard
        IDT[33].set_handler_fn(keyboard_interrupt_handler);

        IDT.load();
    }

    remap_pic();
    unsafe {
        // Unmask timer (IRQ0) and keyboard (IRQ1).
        let mut pic1_data: Port<u8> = Port::new(PIC1_DATA);
        pic1_data.write(0xFC);
    }
}

/// Remap the PIC so IRQs start at IDT entry 32.
fn remap_pic() {
    let mut pic1_command: Port<u8> = Port::new(PIC1_COMMAND);
    let mut pic1_data: Port<u8> = Port::new(PIC1_DATA);
    let mut pic2_command: Port<u8> = Port::new(PIC2_COMMAND);
    let mut pic2_data: Port<u8> = Port::new(PIC2_DATA);

    let a1 = unsafe { pic1_data.read() };
    let a2 = unsafe { pic2_data.read() };

    unsafe {
        pic1_command.write(0x11);
        pic2_command.write(0x11);

        pic1_data.write(0x20);
        pic2_data.write(0x28);

        pic1_data.write(0x04);
        pic2_data.write(0x02);

        pic1_data.write(0x01);
        pic2_data.write(0x01);

        pic1_data.write(a1);
        pic2_data.write(a2);
    }
}

extern "x86-interrupt" fn breakpoint_handler(stack_frame: InterruptStackFrame) {
    crate::logln!("BREAKPOINT: {:#?}", stack_frame);
}

extern "x86-interrupt" fn double_fault_handler(
    stack_frame: InterruptStackFrame,
    _error_code: u64,
) -> ! {
    crate::logln!("DOUBLE FAULT: {:#?}", stack_frame);
    crate::hlt();
}

extern "x86-interrupt" fn page_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: PageFaultErrorCode,
) {
    use x86_64::registers::control::Cr2;
    let addr = Cr2::read().unwrap_or(x86_64::VirtAddr::new_truncate(0));
    crate::logln!("PAGE FAULT at {:#x}: {:#?} {:#?}", addr, error_code, stack_frame);
}

extern "x86-interrupt" fn timer_interrupt_handler(_stack_frame: InterruptStackFrame) {
    unsafe {
        let mut pic1_command: Port<u8> = Port::new(PIC1_COMMAND);
        pic1_command.write(PIC_EOI);
    }
}

extern "x86-interrupt" fn keyboard_interrupt_handler(_stack_frame: InterruptStackFrame) {
    unsafe {
        let mut port: Port<u8> = Port::new(0x60);
        let _scancode = port.read();
        let mut pic1_command: Port<u8> = Port::new(PIC1_COMMAND);
        pic1_command.write(PIC_EOI);
    }
}
