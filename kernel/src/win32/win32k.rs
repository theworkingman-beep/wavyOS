//! Win32 subsystem (win32k) server.
//!
//! Bridges the NT kernel to the Aperture OS GUI compositor. Each Win32 desktop
//! maps to a compositor window tree; window messages are dispatched here.

use crate::gui::{create_window, WindowId};
use spin::Mutex;

/// A Win32 desktop maps to a root compositor window.
#[derive(Clone, Copy)]
pub struct Desktop {
    pub name: [u8; 32],
    pub root: WindowId,
}

const MAX_DESKTOPS: usize = 16;
static DESKTOPS: Mutex<[Option<Desktop>; MAX_DESKTOPS]> =
    Mutex::new([const { None }; MAX_DESKTOPS]);

pub fn init() {
    // The default interactive desktop is created on demand.
}

/// Create a new Win32 desktop.
pub fn create_desktop(name: &str, width: i32, height: i32) -> Option<Desktop> {
    let root = create_window(name, 0, 0, width, height)?;
    let mut desktop_name = [0u8; 32];
    let len = name.len().min(31);
    desktop_name[..len].copy_from_slice(&name.as_bytes()[..len]);

    let desktop = Desktop {
        name: desktop_name,
        root,
    };

    let mut desktops = DESKTOPS.lock();
    let slot = desktops.iter_mut().find(|d| d.is_none())?;
    *slot = Some(desktop);
    Some(desktop)
}

/// Dispatch a window message.
pub fn dispatch_message(_hwnd: u64, _msg: u32, _wparam: u64, _lparam: u64) {
    // TODO: route to the appropriate message queue.
}
