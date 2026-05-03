#![no_std]
#![no_main]

extern crate libvibe;

#[no_mangle]
pub extern "C" fn _start() -> ! {
    // WindowServer: owns framebuffer, composites windows, draws UI
    // v0 stub: wait for IPC messages and acknowledge
    loop {
        libvibe::ipc_recv();
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}
