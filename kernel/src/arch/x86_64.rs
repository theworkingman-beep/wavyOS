//! x86_64 architecture support — IDT, PIC, interrupts
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
        IDT[idx].flags = flags; // 0x8E = present, DPL=0, interrupt gate
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

pub fn init(_boot_info: &mut BootInfo) {
    log::info!("x86_64 arch init: IDT + PIC + interrupts");
    unsafe {
        // Remap PIC
        pic_remap();

        // Set up IDT entries
        set_idt_entry(32, irq0_handler as usize, 0x08, 0x8E); // IRQ0 timer
        set_idt_entry(33, irq1_handler as usize, 0x08, 0x8E); // IRQ1 keyboard
        set_idt_entry(44, irq12_handler as usize, 0x08, 0x8E); // IRQ12 mouse
        set_idt_entry(8, double_fault_handler as usize, 0x08, 0x8E);

        // Load IDT
        let idtr = Idtr {
            limit: (core::mem::size_of::<IdtEntry>() * IDT_ENTRIES - 1) as u16,
            base: &IDT as *const IdtEntry as u64,
        };
        core::arch::asm!("lidt [{0}]", in(reg) &idtr, options(nostack));

        // Enable interrupts
        core::arch::asm!("sti");
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
        // Mask all except IRQ0, IRQ1
        core::arch::asm!("out dx, al", in("dx") 0x21u16, in("al") 0xFCu8);
        core::arch::asm!("out dx, al", in("dx") 0xA1u16, in("al") 0xFFu8);
    }
}

pub fn halt_loop() -> ! {
    loop { unsafe { core::arch::asm!("hlt") } }
}

pub unsafe fn jump_to_user(entry: usize, stack_top: usize) -> ! {
    core::arch::asm!(
        "push {ss}",
        "push {rsp}",
        "push 0x202",
        "push {cs}",
        "push {entry}",
        "iretq",
        entry = in(reg) entry,
        rsp = in(reg) stack_top,
        cs = in(reg) 0x08u64,
        ss = in(reg) 0x10u64,
        options(noreturn)
    );
}
