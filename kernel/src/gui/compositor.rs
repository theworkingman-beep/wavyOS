//! Simple software-rendered window compositor.

use super::color::{blend, Color};
use crate::boot_info::{FrameBufferInfo, PixelFormat};

/// Opaque window handle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowId(pub usize);

/// A window stores its geometry and a backbuffer of premultiplied pixels.
#[derive(Clone, Copy)]
pub struct Window {
    pub id: WindowId,
    pub title: [u8; 64],
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub visible: bool,
    pub backbuffer: *mut Color,
    pub pixel_count: usize,
}

unsafe impl Send for Window {}
unsafe impl Sync for Window {}

/// Compositor owns the framebuffer and the list of windows.
pub struct Compositor {
    buffer: *mut u8,
    buffer_len: usize,
    info: FrameBufferInfo,
    windows: [Option<Window>; 32],
    next_id: usize,
}

unsafe impl Send for Compositor {}
unsafe impl Sync for Compositor {}

impl Compositor {
    pub fn new(buffer: &'static mut [u8], info: FrameBufferInfo) -> Self {
        // For simplicity, allocate window backbuffers out of the early heap.
        // A real implementation would use a dedicated GPU heap.
        Self {
            buffer: buffer.as_mut_ptr(),
            buffer_len: buffer.len(),
            info,
            windows: core::array::from_fn(|_| None),
            next_id: 1,
        }
    }

    /// Create a window with the given geometry.
    pub fn create_window(
        &mut self,
        title: &str,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) -> WindowId {
        let id = WindowId(self.next_id);
        self.next_id += 1;

        let pixel_count = (width * height) as usize;
        let backbuffer = crate::mm::alloc_early(
            pixel_count * core::mem::size_of::<Color>(),
            core::mem::align_of::<Color>(),
        )
        .expect("Failed to allocate window backbuffer") as *mut Color;
        unsafe {
            core::slice::from_raw_parts_mut(backbuffer, pixel_count).fill(Color::WHITE);
        }

        let mut title_buf = [0u8; 64];
        let len = title.len().min(63);
        title_buf[..len].copy_from_slice(&title.as_bytes()[..len]);

        let slot = self.windows.iter_mut().find(|w| w.is_none()).expect("Too many windows");
        *slot = Some(Window {
            id,
            title: title_buf,
            x,
            y,
            width,
            height,
            visible: true,
            backbuffer,
            pixel_count,
        });

        id
    }

    /// Return a mutable reference to the window with the given id.
    pub fn window_mut(&mut self, id: WindowId) -> Option<&mut Window> {
        self.windows.iter_mut().find_map(|w| {
            if let Some(window) = w {
                if window.id == id {
                    return Some(window);
                }
            }
            None
        })
    }

    /// Render all visible windows to the framebuffer, back-to-front.
    pub fn render(&mut self) {
        self.clear(Color::DARK_GRAY);
        for i in 0..self.windows.len() {
            let visible = self.windows[i].map(|w| w.visible).unwrap_or(false);
            if visible {
                let window = self.windows[i].unwrap();
                self.draw_window(&window);
            }
        }
    }

    fn clear(&mut self, color: Color) {
        let info = self.info;
        for y in 0..info.height {
            for x in 0..info.width {
                self.write_pixel(x as i32, y as i32, color);
            }
        }
    }

    fn draw_window(&mut self, window: &Window) {
        let backbuffer = unsafe { core::slice::from_raw_parts(window.backbuffer, window.pixel_count) };
        for local_y in 0..window.height {
            for local_x in 0..window.width {
                let src = backbuffer[(local_y * window.width + local_x) as usize];
                let global_x = window.x + local_x;
                let global_y = window.y + local_y;
                if src.a == 255 {
                    self.write_pixel_unchecked(global_x, global_y, src);
                } else if src.a != 0 {
                    let dst = self.read_pixel(global_x, global_y);
                    self.write_pixel_unchecked(global_x, global_y, blend(src, dst));
                }
            }
        }
    }

    fn write_pixel(&mut self, x: i32, y: i32, color: Color) {
        if x < 0 || y < 0 || x >= self.info.width as i32 || y >= self.info.height as i32 {
            return;
        }
        self.write_pixel_unchecked(x, y, color);
    }

    fn write_pixel_unchecked(&mut self, x: i32, y: i32, color: Color) {
        let bytes_per_pixel = self.info.bytes_per_pixel as usize;
        let stride = self.info.stride;
        let offset = ((y as usize * stride) + x as usize) * bytes_per_pixel;
        let bytes = match self.info.pixel_format {
            PixelFormat::Rgb => [color.r, color.g, color.b, 0],
            PixelFormat::Bgr => [color.b, color.g, color.r, 0],
            PixelFormat::U8 => [color.r, 0, 0, 0],
            PixelFormat::Unknown {
                red_position,
                green_position,
                blue_position,
            } => {
                let mut pixel = [0u8; 4];
                pixel[(red_position as usize / 8).min(3)] = color.r;
                pixel[(green_position as usize / 8).min(3)] = color.g;
                pixel[(blue_position as usize / 8).min(3)] = color.b;
                pixel
            }
        };
        let buf = unsafe { core::slice::from_raw_parts_mut(self.buffer, self.buffer_len) };
        buf[offset..offset + bytes_per_pixel].copy_from_slice(&bytes[..bytes_per_pixel]);
    }

    fn read_pixel(&self, x: i32, y: i32) -> Color {
        if x < 0 || y < 0 || x >= self.info.width as i32 || y >= self.info.height as i32 {
            return Color::BLACK;
        }
        let bytes_per_pixel = self.info.bytes_per_pixel as usize;
        let stride = self.info.stride;
        let offset = ((y as usize * stride) + x as usize) * bytes_per_pixel;
        let buf = unsafe { core::slice::from_raw_parts(self.buffer, self.buffer_len) };
        match self.info.pixel_format {
            PixelFormat::Rgb => Color::new(buf[offset], buf[offset + 1], buf[offset + 2]),
            PixelFormat::Bgr => Color::new(buf[offset + 2], buf[offset + 1], buf[offset]),
            _ => Color::BLACK,
        }
    }
}
