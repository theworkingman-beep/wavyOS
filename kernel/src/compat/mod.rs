//! macOS application compatibility layer

pub mod macho;

pub fn init() {
    crate::log::info!("macOS compat layer initialized");
}
