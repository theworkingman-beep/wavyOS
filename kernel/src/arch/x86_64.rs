//! x86_64 architecture support
use crate::BootInfo;

pub fn init(boot_info: &mut BootInfo) {
    log::info!("x86_64 arch init");
    unsafe {
        // Minimal setup for now — real IDT/GDT init would go here
        core::arch::asm!("cli");
    }
}

pub fn halt_loop() -> ! {
    loop {
        unsafe { core::arch::asm!("hlt") }
    }
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
