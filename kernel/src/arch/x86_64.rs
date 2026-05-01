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
