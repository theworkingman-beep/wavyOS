#![no_std]
#![no_main]

extern crate libvibe;

#[no_mangle]
pub extern "C" fn _start() -> ! {
    let id = libvibe::shm_create(320 * 200 * 4);
    let buf = libvibe::shm_map(id);
    for y in 0..200 {
        for x in 0..320 {
            let off = (y * 320 + x) * 4;
            unsafe {
                *buf.add(off + 0) = 0xFF;
                *buf.add(off + 1) = 0x00;
                *buf.add(off + 2) = 0x00;
                *buf.add(off + 3) = 0xFF;
            }
        }
    }
    let mut msg = [0u8; 64];
    msg[0] = 1;
    libvibe::ipc_send(2, &msg);
    loop {}
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}
