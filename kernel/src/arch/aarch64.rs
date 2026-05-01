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
