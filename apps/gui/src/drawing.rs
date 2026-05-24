//! Drawing primitives — framebuffer, rectangles, font rendering.

use crate::color::Color;

/// Rectangle in pixel coordinates.
#[derive(Clone, Copy, Debug)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    pub const fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Rect { x, y, width, height }
    }

    pub const fn zero() -> Self {
        Rect { x: 0, y: 0, width: 0, height: 0 }
    }

    /// Right edge (exclusive).
    pub fn right(&self) -> u32 {
        self.x + self.width
    }

    /// Bottom edge (exclusive).
    pub fn bottom(&self) -> u32 {
        self.y + self.height
    }

    /// Whether this rect contains the point.
    pub fn contains(&self, px: u32, py: u32) -> bool {
        px >= self.x && px < self.right() && py >= self.y && py < self.bottom()
    }

    /// Intersection of two rects.
    pub fn intersect(&self, other: &Rect) -> Rect {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let r = (self.right()).min(other.right());
        let b = (self.bottom()).min(other.bottom());
        if r > x && b > y {
            Rect::new(x, y, r - x, b - y)
        } else {
            Rect::zero()
        }
    }

    /// Whether this rect has zero area.
    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }
}

/// Size (width x height).
#[derive(Clone, Copy, Debug)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

impl Size {
    pub const fn new(width: u32, height: u32) -> Self {
        Size { width, height }
    }
}

/// A software framebuffer for rendering into.
pub struct Framebuffer {
    pub pixels: &'static mut [u32],
    pub width: u32,
    pub height: u32,
    pub pitch: u32, // pixels per row (may differ from width)
}

impl Framebuffer {
    pub fn new(pixels: &'static mut [u32], width: u32, height: u32) -> Self {
        let pitch = width;
        Framebuffer { pixels, width, height, pitch }
    }

    #[inline]
    pub fn set_pixel(&mut self, x: u32, y: u32, color: Color) {
        if x < self.width && y < self.height {
            let idx = (y * self.pitch + x) as usize;
            if idx < self.pixels.len() {
                self.pixels[idx] = color.to_u32();
            }
        }
    }

    #[inline]
    pub fn get_pixel(&self, x: u32, y: u32) -> Color {
        if x < self.width && y < self.height {
            let idx = (y * self.pitch + x) as usize;
            if idx < self.pixels.len() {
                let v = self.pixels[idx];
                return Color { r: ((v >> 24) & 0xFF) as u8, g: ((v >> 16) & 0xFF) as u8, b: ((v >> 8) & 0xFF) as u8, a: (v & 0xFF) as u8 };
            }
        }
        Color::CLEAR
    }

    /// Blend a pixel over the existing value.
    #[inline]
    pub fn blend_pixel(&mut self, x: u32, y: u32, color: Color) {
        if color.a == 0 { return; }
        if x >= self.width || y >= self.height { return; }
        let idx = (y * self.pitch + x) as usize;
        if idx >= self.pixels.len() { return; }
        if color.a == 255 {
            self.pixels[idx] = color.to_u32();
        } else {
            let dst = self.get_pixel(x, y);
            let blended = color.blend_over(&dst);
            self.pixels[idx] = blended.to_u32();
        }
    }

    /// Fill a rectangle with an opaque color.
    pub fn fill_rect(&mut self, rect: Rect, color: Color) {
        let x_end = (rect.x + rect.width).min(self.width);
        let y_end = (rect.y + rect.height).min(self.height);
        for y in rect.y..y_end {
            for x in rect.x..x_end {
                self.set_pixel(x, y, color);
            }
        }
    }

    /// Fill a rectangle with alpha blending.
    pub fn fill_rect_blend(&mut self, rect: Rect, color: Color) {
        if color.a == 0 { return; }
        if color.a == 255 {
            self.fill_rect(rect, color);
            return;
        }
        let x_end = (rect.x + rect.width).min(self.width);
        let y_end = (rect.y + rect.height).min(self.height);
        for y in rect.y..y_end {
            for x in rect.x..x_end {
                self.blend_pixel(x, y, color);
            }
        }
    }

    /// Fill a vertical gradient between two colors.
    pub fn fill_v_gradient(&mut self, rect: Rect, top: Color, bottom: Color) {
        if rect.height == 0 { return; }
        let x_end = (rect.x + rect.width).min(self.width);
        let y_end = (rect.y + rect.height).min(self.height);
        for y in rect.y..y_end {
            let t = (y - rect.y) as f32 / rect.height as f32;
            let r = (top.r as f32 + (bottom.r as f32 - top.r as f32) * t) as u8;
            let g = (top.g as f32 + (bottom.g as f32 - top.g as f32) * t) as u8;
            let b = (top.b as f32 + (bottom.b as f32 - top.b as f32) * t) as u8;
            let a = (top.a as f32 + (bottom.a as f32 - top.a as f32) * t) as u8;
            let color = Color { r, g, b, a };
            for x in rect.x..x_end {
                self.blend_pixel(x, y, color);
            }
        }
    }

    /// Fill a horizontal gradient between two colors.
    pub fn fill_h_gradient(&mut self, rect: Rect, left: Color, right: Color) {
        if rect.width == 0 { return; }
        let x_end = (rect.x + rect.width).min(self.width);
        let y_end = (rect.y + rect.height).min(self.height);
        for x in rect.x..x_end {
            let t = (x - rect.x) as f32 / rect.width as f32;
            let r = (left.r as f32 + (right.r as f32 - left.r as f32) * t) as u8;
            let g = (left.g as f32 + (right.g as f32 - left.g as f32) * t) as u8;
            let b = (left.b as f32 + (right.b as f32 - left.b as f32) * t) as u8;
            let a = (left.a as f32 + (right.a as f32 - left.a as f32) * t) as u8;
            let color = Color { r, g, b, a };
            for y in rect.y..y_end {
                self.blend_pixel(x, y, color);
            }
        }
    }

    /// Draw a filled circle (approximate).
    pub fn fill_circle(&mut self, cx: u32, cy: u32, radius: u32, color: Color) {
        let r2 = (radius * radius) as i32;
        let x_start = if cx > radius { cx - radius } else { 0 };
        let y_start = if cy > radius { cy - radius } else { 0 };
        let x_end = (cx + radius + 1).min(self.width);
        let y_end = (cy + radius + 1).min(self.height);
        for y in y_start..y_end {
            for x in x_start..x_end {
                let dx = (x as i32) - (cx as i32);
                let dy = (y as i32) - (cy as i32);
                if dx * dx + dy * dy <= r2 {
                    self.blend_pixel(x, y, color);
                }
            }
        }
    }

    /// Draw a filled rounded rectangle.
    pub fn fill_rounded_rect(&mut self, rect: Rect, radius: u32, color: Color) {
        // Fill center area
        self.fill_rect_blend(
            Rect::new(rect.x + radius, rect.y, rect.width - 2 * radius, rect.height),
            color,
        );
        // Fill left strip
        self.fill_rect_blend(
            Rect::new(rect.x, rect.y + radius, radius, rect.height - 2 * radius),
            color,
        );
        // Fill right strip
        self.fill_rect_blend(
            Rect::new(rect.x + rect.width - radius, rect.y + radius, radius, rect.height - 2 * radius),
            color,
        );
        // Draw four corner circles
        self.fill_circle(rect.x + radius, rect.y + radius, radius, color);
        self.fill_circle(rect.x + rect.width - radius - 1, rect.y + radius, radius, color);
        self.fill_circle(rect.x + radius, rect.y + rect.height - radius - 1, radius, color);
        self.fill_circle(rect.x + rect.width - radius - 1, rect.y + rect.height - radius - 1, radius, color);
    }

    /// Draw a horizontal line.
    pub fn draw_hline(&mut self, x: u32, y: u32, width: u32, color: Color) {
        for dx in 0..width {
            self.blend_pixel(x + dx, y, color);
        }
    }

    /// Draw a border around a rectangle.
    pub fn draw_border(&mut self, rect: Rect, color: Color, thickness: u32) {
        // Top
        self.fill_rect_blend(Rect::new(rect.x, rect.y, rect.width, thickness), color);
        // Bottom
        self.fill_rect_blend(Rect::new(rect.x, rect.y + rect.height - thickness, rect.width, thickness), color);
        // Left
        self.fill_rect_blend(Rect::new(rect.x, rect.y + thickness, thickness, rect.height - 2 * thickness), color);
        // Right
        self.fill_rect_blend(Rect::new(rect.x + rect.width - thickness, rect.y + thickness, thickness, rect.height - 2 * thickness), color);
    }

    /// Draw a shadow (semi-transparent rectangle offset from the target).
    pub fn draw_shadow(&mut self, rect: Rect, offset: u32, color: Color) {
        let shadow_rect = Rect::new(rect.x + offset, rect.y + offset, rect.width, rect.height);
        self.fill_rect_blend(shadow_rect, color);
    }

    /// Clear the entire framebuffer to a color.
    pub fn clear(&mut self, color: Color) {
        let packed = color.to_u32();
        for pixel in self.pixels.iter_mut() {
            *pixel = packed;
        }
    }

    /// Blit a source buffer onto this framebuffer at the given position.
    pub fn blit(&mut self, x: u32, y: u32, src: &[u32], src_w: u32, src_h: u32) {
        for sy in 0..src_h {
            let dy = y + sy;
            if dy >= self.height { break; }
            for sx in 0..src_w {
                let dx = x + sx;
                if dx >= self.width { break; }
                let src_pixel = src[(sy * src_w + sx) as usize];
                let a = (src_pixel & 0xFF) as u8;
                if a > 0 {
                    let color = Color {
                        r: ((src_pixel >> 24) & 0xFF) as u8,
                        g: ((src_pixel >> 16) & 0xFF) as u8,
                        b: ((src_pixel >> 8) & 0xFF) as u8,
                        a,
                    };
                    self.blend_pixel(dx, dy, color);
                }
            }
        }
    }
}

/// 8x16 bitmap font (same as kernel/windowserver font).
pub struct Font;

impl Font {
    /// Width of each character cell in pixels.
    pub const CHAR_W: u32 = 8;
    /// Height of each character cell in pixels.
    pub const CHAR_H: u32 = 16;

    /// Draw a single character at (x, y) using the built-in bitmap font.
    pub fn draw_char(fb: &mut Framebuffer, x: u32, y: u32, ch: char, fg: Color, bg: Option<Color>) {
        let glyph = if (ch as usize) < 128 {
            FONT_DATA.get(ch as usize).copied().unwrap_or([0u8; 16])
        } else {
            [0u8; 16]
        };
        for row in 0..16usize {
            let bits = glyph[row];
            for col in 0..8usize {
                if (bits >> (7 - col)) & 1 != 0 {
                    fb.set_pixel(x + col as u32, y + row as u32, fg);
                } else if let Some(bg_color) = bg {
                    fb.set_pixel(x + col as u32, y + row as u32, bg_color);
                }
            }
        }
    }

    /// Draw a string starting at (x, y).
    pub fn draw_str(fb: &mut Framebuffer, x: u32, y: u32, s: &str, fg: Color, bg: Option<Color>) {
        let mut cx = x;
        for ch in s.chars() {
            if ch == '\n' {
                // Don't draw, just note that newlines are possible
                continue;
            }
            Self::draw_char(fb, cx, y, ch, fg, bg);
            cx += Self::CHAR_W;
            if cx + Self::CHAR_W > fb.width {
                break;
            }
        }
    }

    /// Measure the pixel width of a string.
    pub fn str_width(s: &str) -> u32 {
        s.chars().count() as u32 * Self::CHAR_W
    }
}

#[path = "font_data.rs"]
mod font_data;

/// Built-in 8x16 bitmap font data (ASCII 0-127).
/// Each character is 16 rows of 1 byte (8 bits = 8 pixels wide).
pub use font_data::FONT_DATA;
