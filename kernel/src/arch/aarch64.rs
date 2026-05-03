//! aarch64 architecture support
use crate::BootInfo;

pub fn init(boot_info: &mut BootInfo) {
    crate::log::info!("aarch64 arch init");
    // Device tree parsing, EL1 setup
}

pub fn halt_loop() -> ! {
    loop {
        unsafe { core::arch::asm!("wfe") }
    }
}

pub unsafe fn jump_to_user(entry: usize, stack_top: usize) -> ! {
    core::arch::asm!(
        "mov sp, {stack}",
        "br {entry}",
        stack = in(reg) stack_top,
        entry = in(reg) entry,
        options(noreturn)
    );
}
