//! Panic handler.

use core::panic::PanicInfo;

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    crate::logln!("PANIC: {}", info);
    crate::hlt();
}
