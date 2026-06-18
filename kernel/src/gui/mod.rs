//! GUI subsystem.
//!
//! A simple software-rendered compositor. In the future this will support
//! hardware acceleration and a GPU-driven scene graph.

use bootloader_api::info::FrameBufferInfo;
use spin::Mutex;

pub mod color;
pub mod compositor;

pub use compositor::WindowId;
use compositor::Compositor;

static COMPOSITOR: Mutex<Option<Compositor>> = Mutex::new(None);

/// Initialize the GUI with the bootloader-provided framebuffer.
pub fn init() {
    // The framebuffer pointer/info is passed from kernel_main at startup.
}

/// Set up the compositor once the framebuffer is known.
pub fn init_compositor(buffer: &'static mut [u8], info: FrameBufferInfo) {
    *COMPOSITOR.lock() = Some(Compositor::new(buffer, info));
    // Create a root desktop window.
    if let Some(c) = COMPOSITOR.lock().as_mut() {
        let _desktop = c.create_window("Desktop", 0, 0, info.width as i32, info.height as i32);
    }
}

/// Render the current scene to the framebuffer.
pub fn render() {
    if let Some(c) = COMPOSITOR.lock().as_mut() {
        c.render();
    }
}

/// Create a new window and return its handle.
pub fn create_window(title: &str, x: i32, y: i32, width: i32, height: i32) -> Option<WindowId> {
    COMPOSITOR.lock().as_mut().map(|c| c.create_window(title, x, y, width, height))
}
