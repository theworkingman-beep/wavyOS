#![no_std]
#![no_main]

extern crate libvibe;

#[no_mangle]
pub extern "C" fn _start() -> ! {
    // Desktop Shell: focus policy, dock, top bar
    // v0 stub: idle
    loop {
        libvibe::ipc_recv();
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}
