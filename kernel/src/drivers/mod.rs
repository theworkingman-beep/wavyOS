pub mod uart;
pub mod uart_logger;
pub mod fbcon;
pub mod cursor;
#[cfg(target_arch = "x86_64")]
pub mod ps2kbd;
#[cfg(target_arch = "x86_64")]
pub mod ps2mouse;
