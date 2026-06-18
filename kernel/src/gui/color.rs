//! Color utilities for the software renderer.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 0xFF }
    }

    pub const fn with_alpha(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub const BLACK: Self = Self::new(0, 0, 0);
    pub const WHITE: Self = Self::new(255, 255, 255);
    pub const RED: Self = Self::new(255, 0, 0);
    pub const GREEN: Self = Self::new(0, 255, 0);
    pub const BLUE: Self = Self::new(0, 0, 255);
    pub const TEAL: Self = Self::new(0, 128, 128);
    pub const GRAY: Self = Self::new(128, 128, 128);
    pub const DARK_GRAY: Self = Self::new(48, 48, 48);
}

/// Blend `src` over `dst` assuming premultiplied alpha.
pub fn blend(src: Color, dst: Color) -> Color {
    if src.a == 255 {
        return src;
    }
    if src.a == 0 {
        return dst;
    }
    let sa = src.a as u16;
    let inv_sa = 255 - src.a as u16;
    let r = ((src.r as u16 * sa + dst.r as u16 * inv_sa) / 255) as u8;
    let g = ((src.g as u16 * sa + dst.g as u16 * inv_sa) / 255) as u8;
    let b = ((src.b as u16 * sa + dst.b as u16 * inv_sa) / 255) as u8;
    Color::with_alpha(r, g, b, 255)
}
