//! PS/2 Keyboard driver for x86_64
//! Uses scancode set 1 (standard PC AT keyboard)
//! IRQ 1 -> IDT entry 33

use crate::input::{self, InputEvent};
use spin::Mutex;

/// PS/2 scancode set 1 translation table (unshifted)
const SCANCODE_TABLE: &'static [u8] = &[
    0,  27, b'1', b'2', b'3', b'4', b'5', b'6', b'7', b'8', b'9', b'0', b'-', b'=', b'\x08', b'\t', // 0x00-0x0F
    b'q', b'w', b'e', b'r', b't', b'y', b'u', b'i', b'o', b'p', b'[', b']', b'\n',  0,    b'a',  0,    // 0x10-0x1F
    0,    0,    b'z', b'x', b'c', b'v', b'b', b'n', b'm', b',', b'.', b'/',  0,    0,    0,    0,    // 0x20-0x2F
    b' ', 0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    // 0x30-0x3F
    0,    0,    0,    0,    0,    0,    0,    0,    0,    b'-', 0,    0,    0,    b'+', 0,    0,    // 0x40-0x4F
    0,    0,    0,    0,    0,    b'.', 0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    // 0x50-0x5F
];

/// Shifted scancode table
const SCANCODE_SHIFT_TABLE: &'static [u8] = &[
    0,  27, b'!', b'@', b'#', b'$', b'%', b'^', b'&', b'*', b'(', b')', b'_', b'+', b'\x08', b'\t',
    b'Q', b'W', b'E', b'R', b'T', b'Y', b'U', b'I', b'O', b'P', b'{', b'}', b'\n',  0,    b'A',  0,
    0,    0,    b'Z', b'X', b'C', b'V', b'B', b'N', b'M', b'<', b'>', b'?',  0,    0,    0,    0,
    b' ', 0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,
    0,    0,    0,    0,    0,    0,    0,    0,    0,    b'_', 0,    0,    0,    b'+', 0,    0,
    0,    0,    0,    0,    0,    b'.', 0,    0,    0,    0,    0,    0,    0,    0,    0,    0,
];

static SHIFT_PRESSED: Mutex<bool> = Mutex::new(false);
static CAPS_LOCK: Mutex<bool> = Mutex::new(false);

/// Initialize PS/2 keyboard
pub fn init() {
    wait_kb_ready();

    // Enable keyboard interface (IRQ 1)
    unsafe {
        outb(0x64, 0xAE);
        outb(0x64, 0x60);
        wait_kb_ready();
        let mut cmd = inb(0x60);
        cmd |= 0x01;
        cmd &= !0x10;
        outb(0x64, 0x60);
        wait_kb_ready();
        outb(0x60, cmd);
    }

    remap_pic();

    unsafe {
        let mask = inb(0x21);
        outb(0x21, mask & !0x02);
    }

    log::info!("ps2kbd: initialized (IRQ 1 -> int 0x21)");
}

/// Called from the IRQ handler when a scancode is received
pub fn handle_scancode(scancode: u8) {
    const KEY_RELEASE: u8 = 0x80;
    const LSHIFT: u8 = 0x2A;
    const RSHIFT: u8 = 0x36;
    const CAPS_KEY: u8 = 0x3A;

    let mut shift = SHIFT_PRESSED.lock();
    let mut caps = CAPS_LOCK.lock();

    if scancode == LSHIFT || scancode == RSHIFT {
        *shift = true;
        return;
    }
    if scancode == (LSHIFT | KEY_RELEASE) || scancode == (RSHIFT | KEY_RELEASE) {
        *shift = false;
        return;
    }
    if scancode == CAPS_KEY {
        *caps = !*caps;
        return;
    }

    if scancode & KEY_RELEASE != 0 {
        return;
    }

    let ascii = if (scancode as usize) < SCANCODE_TABLE.len() {
        let idx = scancode as usize;
        let mut c = SCANCODE_TABLE[idx];

        if *shift {
            if idx < SCANCODE_SHIFT_TABLE.len() {
                c = SCANCODE_SHIFT_TABLE[idx];
            }
        } else if *caps && c >= b'a' && c <= b'z' {
            c = c - 32;
        }

        c
    } else {
        0
    };

    if ascii != 0 {
        input::push(InputEvent::KeyPress { ascii });
    }
}

fn remap_pic() {
    unsafe {
        outb(0x20, 0x11);
        outb(0xA0, 0x11);
        outb(0x21, 0x20);
        outb(0xA1, 0x28);
        outb(0x21, 0x04);
        outb(0xA1, 0x02);
        outb(0x21, 0x01);
        outb(0xA1, 0x01);
        outb(0x21, 0xFC);
        outb(0xA1, 0xFF);
    }
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

#[inline]
unsafe fn outb(port: u16, value: u8) {
    core::arch::asm!("outb %al, %dx", in("al") value, in("dx") port);
}

#[inline]
unsafe fn inb(port: u16) -> u8 {
    let result: u8;
    core::arch::asm!("inb %dx, %al", in("dx") port, out("al") result);
    result
}

/// End-of-interrupt signal to PIC
pub unsafe fn eoi() {
    outb(0x20, 0x20);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{InputEvent, poll};

    #[test]
    fn test_scancode_to_keypress() {
        handle_scancode(0x1E);
        assert_eq!(poll(), Some(InputEvent::KeyPress { ascii: b'a' }));
    }

    #[test]
    fn test_shifted_scancode() {
        handle_scancode(0x2A);
        handle_scancode(0x02);
        assert_eq!(poll(), Some(InputEvent::KeyPress { ascii: b'!' }));
    }

    #[test]
    fn test_key_release_ignored() {
        handle_scancode(0x9E);
        assert_eq!(poll(), None);
    }
}
