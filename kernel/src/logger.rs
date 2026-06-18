//! Minimal serial logger.

use core::fmt::{self, Write};
use spin::Mutex;

struct SerialWriter;

impl Write for SerialWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for byte in s.bytes() {
            crate::arch::debug_putchar(byte);
        }
        Ok(())
    }
}

static LOGGER: Mutex<Option<SerialWriter>> = Mutex::new(None);

pub fn init() {
    *LOGGER.lock() = Some(SerialWriter);
}

pub fn _print(args: fmt::Arguments) {
    if let Some(writer) = LOGGER.lock().as_mut() {
        let _ = writer.write_fmt(args);
    }
}

#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => {
        $crate::logger::_print(format_args!($($arg)*));
    };
}

#[macro_export]
macro_rules! logln {
    () => ($crate::log!("\n"));
    ($fmt:expr) => ($crate::log!(concat!($fmt, "\n")));
    ($fmt:expr, $($arg:tt)*) => ($crate::log!(concat!($fmt, "\n"), $($arg)*));
}
