use core::fmt;

#[cfg(target_arch = "x86_64")]
const COM1_PORT: u16 = 0x3F8;

#[cfg(target_arch = "aarch64")]
const PL011_BASE: u64 = 0x09000000;

#[cfg(target_arch = "x86_64")]
fn outb(port: u16, val: u8) {
    unsafe { core::arch::asm!("out dx, al", in("dx") port, in("al") val); }
}

#[cfg(target_arch = "x86_64")]
fn inb(port: u16) -> u8 {
    let ret: u8;
    unsafe { core::arch::asm!("in al, dx", out("al") ret, in("dx") port); }
    ret
}

#[cfg(target_arch = "aarch64")]
fn pl011_read(offset: u64) -> u32 {
    unsafe { core::ptr::read_volatile((PL011_BASE + offset) as *const u32) }
}

#[cfg(target_arch = "aarch64")]
fn pl011_write(offset: u64, val: u32) {
    unsafe { core::ptr::write_volatile((PL011_BASE + offset) as *mut u32, val) }
}

pub fn init() {
    #[cfg(target_arch = "x86_64")]
    {
        outb(COM1_PORT + 1, 0x00);
        outb(COM1_PORT + 3, 0x80);
        outb(COM1_PORT + 0, 0x03);
        outb(COM1_PORT + 1, 0x00);
        outb(COM1_PORT + 3, 0x03);
        outb(COM1_PORT + 2, 0xC7);
        outb(COM1_PORT + 4, 0x0B);
    }

    #[cfg(target_arch = "aarch64")]
    {
        // Disable UART before configuration
        pl011_write(0x30, 0); // UARTCR = 0
        // Set baud rate: 115200 with 24MHz clock
        // IBRD = 24000000 / (16 * 115200) = 13
        pl011_write(0x24, 13); // UARTIBRD
        // FBRD = round(64 * fractional_part) = round(64 * 0.0286) = 2
        pl011_write(0x28, 2);  // UARTFBRD
        // Line control: 8-bit, FIFO enabled, parity none
        pl011_write(0x2C, 0x70); // UARTLCR_H: FEN(4) + WLEN(5,6)
        // Enable UART, TX, RX
        pl011_write(0x30, 0x301); // UARTCR: UARTEN(0) + TXE(8) + RXE(9)
    }
}

pub fn putc(c: u8) {
    #[cfg(target_arch = "x86_64")]
    {
        while (inb(COM1_PORT + 5) & 0x20) == 0 {}
        outb(COM1_PORT, c);
    }
    #[cfg(target_arch = "aarch64")]
    {
        while (pl011_read(0x18) & 0x20) != 0 {} // Wait while TXFF
        pl011_write(0x00, c as u32); // UARTDR
    }
}

pub fn puts(s: &str) {
    for c in s.bytes() {
        if c == b'\n' { putc(b'\r'); }
        putc(c);
    }
}

pub struct UartWriter;
impl fmt::Write for UartWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        puts(s);
        Ok(())
    }
}

/// Handle UART receive interrupt — reads available characters and pushes them as input events
#[cfg(target_arch = "aarch64")]
pub fn handle_uart_irq() {
    use crate::input::{self, InputEvent};

    // Read characters from FIFO until empty
    loop {
        let fr = pl011_read(0x18); // UARTFR
        // Check if RX FIFO is empty (bit 4 = RXFE)
        if fr & 0x10 != 0 {
            break;
        }
        let data = pl011_read(0x00) as u8; // UARTDR
        // Only process valid ASCII characters (skip errors)
        if data >= 32 && data <= 126 {
            input::push(InputEvent::KeyPress { ascii: data });
        } else if data == b'\r' || data == b'\n' {
            input::push(InputEvent::KeyPress { ascii: b'\n' });
        } else if data == 0x08 { // Backspace
            input::push(InputEvent::KeyPress { ascii: 0x08 });
        }
    }

    // Clear UART interrupt (ICR register)
    pl011_write(0x44, 0xFFFF); // Clear all interrupts
}
