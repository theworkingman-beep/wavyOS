//! Color types and pixel format definitions for the GUI framework.

#![allow(dead_code)]

/// RGBA color with 8-bit channels.
#[derive(Clone, Copy, Debug)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const CLEAR: Color = Color { r: 0, g: 0, b: 0, a: 0 };
    pub const WHITE: Color = Color { r: 255, g: 255, b: 255, a: 255 };
    pub const BLACK: Color = Color { r: 0, g: 0, b: 0, a: 255 };
    pub const RED: Color = Color { r: 255, g: 59, b: 48, a: 255 };
    pub const ORANGE: Color = Color { r: 255, g: 149, b: 0, a: 255 };
    pub const YELLOW: Color = Color { r: 255, g: 204, b: 0, a: 255 };
    pub const GREEN: Color = Color { r: 52, g: 199, b: 89, a: 255 };
    pub const TEAL: Color = Color { r: 90, g: 200, b: 250, a: 255 };
    pub const BLUE: Color = Color { r: 0, g: 122, b: 255, a: 255 };
    pub const LIGHT_BLUE: Color = Color { r: 90, g: 200, b: 250, a: 255 };
    pub const PURPLE: Color = Color { r: 175, g: 82, b: 222, a: 255 };
    pub const PINK: Color = Color { r: 255, g: 45, b: 85, a: 255 };
    pub const GRAY: Color = Color { r: 142, g: 142, b: 147, a: 255 };
    pub const LIGHT_GRAY: Color = Color { r: 229, g: 229, b: 234, a: 255 };
    pub const DARK_GRAY: Color = Color { r: 99, g: 99, b: 102, a: 255 };
    pub const WINDOW_BG: Color = Color { r: 236, g: 236, b: 236, a: 255 };
    pub const TITLE_BAR_BG: Color = Color { r: 232, g: 232, b: 232, a: 255 };
    pub const MENU_BAR_BG: Color = Color { r: 220, g: 220, b: 222, a: 230 };
    pub const DESKTOP_BG: Color = Color { r: 46, g: 46, b: 46, a: 255 };
    pub const ACCENT: Color = Color { r: 0, g: 122, b: 255, a: 255 };

    pub const fn rgb(r: u8, g: u8, b: u8) -> Color {
        Color { r, g, b, a: 255 }
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color {
        Color { r, g, b, a }
    }

    /// Convert to a packed 32-bit RGBA value (0xRRGGBBAA).
    pub fn to_u32(&self) -> u32 {
        ((self.r as u32) << 24) | ((self.g as u32) << 16) | ((self.b as u32) << 8) | (self.a as u32)
    }

    /// Blend this color over another (Porter-Duff src-over).
    pub fn blend_over(&self, dst: &Color) -> Color {
        if self.a == 0 {
            return *dst;
        }
        if self.a == 255 {
            return *self;
        }
        let sa = self.a as u32;
        let da = dst.a as u32;
        let out_a = sa + ((da * (255 - sa)) / 255);
        if out_a == 0 {
            return Color::CLEAR;
        }
        let r = ((self.r as u32 * sa + dst.r as u32 * da * (255 - sa) / 255) / out_a) as u8;
        let g = ((self.g as u32 * sa + dst.g as u32 * da * (255 - sa) / 255) / out_a) as u8;
        let b = ((self.b as u32 * sa + dst.b as u32 * da * (255 - sa) / 255) / out_a) as u8;
        Color { r, g, b, a: out_a as u8 }
    }

    /// Lighten by a factor (0.0 = no change, 1.0 = white).
    pub fn lighten(&self, factor: f32) -> Color {
        Color {
            r: (self.r as f32 + (255.0 - self.r as f32) * factor) as u8,
            g: (self.g as f32 + (255.0 - self.g as f32) * factor) as u8,
            b: (self.b as f32 + (255.0 - self.b as f32) * factor) as u8,
            a: self.a,
        }
    }

    /// Darken by a factor (0.0 = no change, 1.0 = black).
    pub fn darken(&self, factor: f32) -> Color {
        Color {
            r: (self.r as f32 * (1.0 - factor)) as u8,
            g: (self.g as f32 * (1.0 - factor)) as u8,
            b: (self.b as f32 * (1.0 - factor)) as u8,
            a: self.a,
        }
    }

    /// With alpha override.
    pub fn with_alpha(&self, alpha: u8) -> Color {
        Color { r: self.r, g: self.g, b: self.b, a: alpha }
    }
}

/// Pixel format for framebuffer and surface buffers.
#[derive(Clone, Copy, Debug)]
pub enum PixelFormat {
    /// 32-bit RGBA, 1 byte per channel.
    RGBA32,
    /// 32-bit BGRA, 1 byte per channel (common on x86).
    BGRA32,
    /// 24-bit RGB, 1 byte per channel.
    RGB24,
}

impl PixelFormat {
    pub fn bytes_per_pixel(&self) -> u32 {
        match self {
            PixelFormat::RGBA32 | PixelFormat::BGRA32 => 4,
            PixelFormat::RGB24 => 3,
        }
    }
}