//! PS/2 Mouse driver for x86_64
//! IRQ 12 on PIC2 -> interrupt 0x2C

use crate::input::{self, InputEvent};
use spin::Mutex;

#[derive(Debug, Clone, Copy)]
pub struct MouseState {
    pub x: u16,
    pub y: u16,
    pub left: bool,
    pub right: bool,
    pub middle: bool,
}

impl Default for MouseState {
    fn default() -> Self {
        Self {
            x: 400,
            y: 300,
            left: false,
            right: false,
            middle: false,
        }
    }
}

static MOUSE_STATE: Mutex<MouseState> = Mutex::new(MouseState {
    x: 400,
    y: 300,
    left: false,
    right: false,
    middle: false,
});
static MOUSE_PHASE: Mutex<u8> = Mutex::new(0);
static MOUSE_BYTE1: Mutex<u8> = Mutex::new(0);
static MOUSE_BYTE2: Mutex<u8> = Mutex::new(0);

pub fn init() {
    unsafe {
        wait_kb_ready();
        outb(0x64, 0xA8);

        wait_kb_ready();
        outb(0x64, 0x20);
        wait_kb_ready();
        let mut cmd = inb(0x60);

        cmd |= 0x02;
        cmd &= !0x20;

        wait_kb_ready();
        outb(0x64, 0x60);
        wait_kb_ready();
        outb(0x60, cmd);

        let mask = inb(0xA1);
        outb(0xA1, mask & !0x10);

        wait_kb_ready();
        outb(0x64, 0xD4);
        wait_kb_ready();
        outb(0x60, 0xF4);

        log::info!("ps2mouse: initialized (IRQ 12 -> int 0x2C)");
    }
}

pub fn handle_mouse_byte(byte: u8) {
    let mut phase = MOUSE_PHASE.lock();
    match *phase {
        0 => {
            if byte & 0x08 != 0 {
                *MOUSE_BYTE1.lock() = byte;
                *phase = 1;
            }
        }
        1 => {
            *MOUSE_BYTE2.lock() = byte;
            *phase = 2;
        }
        2 => {
            let buttons = *MOUSE_BYTE1.lock();
            let dx = *MOUSE_BYTE2.lock();
            let dy = byte;

            let mut state = MOUSE_STATE.lock();
            let prev_left = state.left;
            let prev_right = state.right;
            let prev_middle = state.middle;

            decode_packet(&mut state, buttons, dx as i8, dy as i8);

            let fb_w = unsafe { crate::drivers::fbcon::fb_width() as u16 };
            let fb_h = unsafe { crate::drivers::fbcon::fb_height() as u16 };
            if state.x >= fb_w { state.x = fb_w - 1; }
            if state.y >= fb_h { state.y = fb_h - 1; }

            let x = state.x;
            let y = state.y;
            let left = state.left;
            let right = state.right;
            let middle = state.middle;
            let buttons_byte = if left { 1 } else { 0 }
                | if right { 2 } else { 0 }
                | if middle { 4 } else { 0 };

            drop(state);

            input::push(InputEvent::MouseMove { x, y, buttons: buttons_byte });

            if left && !prev_left {
                input::push(InputEvent::MouseDown { button: 0, x, y });
            } else if !left && prev_left {
                input::push(InputEvent::MouseUp { button: 0, x, y });
            }
            if right && !prev_right {
                input::push(InputEvent::MouseDown { button: 1, x, y });
            } else if !right && prev_right {
                input::push(InputEvent::MouseUp { button: 1, x, y });
            }
            if middle && !prev_middle {
                input::push(InputEvent::MouseDown { button: 2, x, y });
            } else if !middle && prev_middle {
                input::push(InputEvent::MouseUp { button: 2, x, y });
            }

            *phase = 0;
        }
        _ => {
            *phase = 0;
        }
    }
}

pub fn decode_packet(state: &mut MouseState, buttons: u8, dx: i8, dy: i8) {
    state.left = buttons & 0x01 != 0;
    state.right = buttons & 0x02 != 0;
    state.middle = buttons & 0x04 != 0;

    state.x = (state.x as i32 + dx as i32).max(0) as u16;
    state.y = (state.y as i32 - dy as i32).max(0) as u16;
}

pub fn get_mouse_state() -> MouseState {
    *MOUSE_STATE.lock()
}

unsafe fn outb(port: u16, value: u8) {
    core::arch::asm!("out dx, al", in("al") value, in("dx") port);
}

unsafe fn inb(port: u16) -> u8 {
    let result: u8;
    core::arch::asm!("in al, dx", in("dx") port, out("al") result);
    result
}

fn wait_kb_ready() {
    unsafe {
        let mut timeout = 100000u32;
        while timeout > 0 {
            let status = inb(0x64);
            if status & 0x02 == 0 {
                return;
            }
            timeout -= 1;
        }
    }
}

pub unsafe fn eoi() {
    outb(0xA0, 0x20);
    outb(0x20, 0x20);
}
