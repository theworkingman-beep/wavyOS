//! Software-rendered mouse cursor.

use super::color::Color;
use super::compositor::Compositor;

/// Standard 16x16 arrow cursor bitmap. `1` = body.
const CURSOR: [u16; 16] = [
    0b1100000000000000,
    0b1110000000000000,
    0b1111000000000000,
    0b1111100000000000,
    0b1111110000000000,
    0b1111111000000000,
    0b1111111100000000,
    0b1111111110000000,
    0b1111111111000000,
    0b1111111111100000,
    0b1111111111110000,
    0b1111110000000000,
    0b1110011110000000,
    0b1100011110000000,
    0b0000001110000000,
    0b0000001100000000,
];

/// Draw the mouse cursor onto the framebuffer at `(x, y)`.
pub fn draw_cursor(c: &mut Compositor, x: i32, y: i32) {
    for row in 0..16 {
        let mask = CURSOR[row as usize];
        for col in 0..16 {
            let bit = 0x8000u16 >> col;
            let value = mask & bit;
            if value != 0 {
                // Body: white pixel.
                c.write_pixel(x + col, y + row, Color::WHITE);
            } else if col > 0 && (mask & (0x8000u16 >> (col - 1))) != 0 {
                // Simple outline drop-shadow to the right of body pixels.
                c.write_pixel(x + col, y + row, Color::BLACK);
            }
        }
    }
}
