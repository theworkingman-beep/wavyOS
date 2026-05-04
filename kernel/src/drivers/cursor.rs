//! Software cursor renderer — 16x16 arrow bitmap with save/restore
use core::sync::atomic::{AtomicBool, AtomicU16, Ordering};

static CURSOR_BITMAP: &[u8; 32] = &[
    0b10000000, 0b00000000,
    0b11000000, 0b00000000,
    0b10100000, 0b00000000,
    0b10010000, 0b00000000,
    0b10001000, 0b00000000,
    0b10000100, 0b00000000,
    0b10000010, 0b00000000,
    0b10000001, 0b00000000,
    0b10000000, 0b10000000,
    0b10000000, 0b01000000,
    0b10000000, 0b00100000,
    0b10000000, 0b00010000,
    0b10000000, 0b00011000,
    0b10000000, 0b00000000,
    0b10000001, 0b10000000,
    0b11000011, 0b10000000,
];

static mut SAVED_PIXELS: [[u32; 16]; 16] = [[0; 16]; 16];

static CURSOR_VISIBLE: AtomicBool = AtomicBool::new(false);
static CURSOR_X: AtomicU16 = AtomicU16::new(0);
static CURSOR_Y: AtomicU16 = AtomicU16::new(0);

const CURSOR_FG: u32 = 0xFFFFFF;
const CURSOR_OUTLINE: u32 = 0x000000;

pub fn init() {
    log::info!("cursor: initialized");
}

fn pixel_set(x: u16, y: u16) -> bool {
    let bit_index = y as usize * 16 + x as usize;
    let byte_idx = bit_index / 8;
    let bit_idx = 7 - (bit_index % 8);
    (CURSOR_BITMAP[byte_idx] >> bit_idx) & 1 != 0
}

unsafe fn draw_outline_pixel(x: usize, y: usize) {
    let fb = crate::drivers::fbcon::get_framebuffer();
    let fb_width = fb.info.width as usize;
    if x < fb_width && y < fb.info.height as usize {
        crate::drivers::fbcon::set_pixel(x, y, CURSOR_OUTLINE);
    }
}

unsafe fn draw_cursor_pixels(x: u16, y: u16) {
    let fb = crate::drivers::fbcon::get_framebuffer();
    let fb_width = fb.info.width as usize;

    for cy in 0..16u16 {
        for cx in 0..16u16 {
            if !pixel_set(cx, cy) {
                continue;
            }
            let px = (x + cx) as usize;
            let py = (y + cy) as usize;
            if px < fb_width && py < fb.info.height as usize {
                crate::drivers::fbcon::set_pixel(px, py, CURSOR_FG);
            }

            if cx > 0 && !pixel_set(cx - 1, cy) {
                draw_outline_pixel(px - 1, py);
            }
            if cx < 15 && !pixel_set(cx + 1, cy) {
                draw_outline_pixel(px + 1, py);
            }
            if cy > 0 && !pixel_set(cx, cy - 1) {
                draw_outline_pixel(px, py - 1);
            }
            if cy < 15 && !pixel_set(cx, cy + 1) {
                draw_outline_pixel(px, py + 1);
            }
        }
    }
}

pub fn clamp_position(x: i32, y: i32, max_x: u16, max_y: u16) -> (u16, u16) {
    (
        x.max(0).min(max_x as i32 - 16) as u16,
        y.max(0).min(max_y as i32 - 16) as u16,
    )
}

pub fn get_position() -> (u16, u16) {
    (CURSOR_X.load(Ordering::SeqCst), CURSOR_Y.load(Ordering::SeqCst))
}

unsafe fn save_pixels(x: u16, y: u16) {
    let fb = crate::drivers::fbcon::get_framebuffer();
    let fb_width = fb.info.width as usize;

    for cy in 0..16u16 {
        for cx in 0..16u16 {
            let px = (x + cx) as usize;
            let py = (y + cy) as usize;
            if px < fb_width && py < fb.info.height as usize {
                let offset = (py * fb_width + px) * (fb.info.bpp as usize / 8);
                let pixel = if fb.info.bpp == 32 {
                    let ptr = fb.ptr.add(offset) as *const u32;
                    core::ptr::read_volatile(ptr)
                } else {
                    let ptr = fb.ptr.add(offset);
                    let r = *ptr as u32;
                    let g = *ptr.add(1) as u32;
                    let b = *ptr.add(2) as u32;
                    (r << 16) | (g << 8) | b
                };
                SAVED_PIXELS[cy as usize][cx as usize] = pixel;
            }
        }
    }
}

pub fn draw(x: u16, y: u16) {
    unsafe {
        save_pixels(x, y);
        draw_cursor_pixels(x, y);
    }
    CURSOR_X.store(x, Ordering::SeqCst);
    CURSOR_Y.store(y, Ordering::SeqCst);
    CURSOR_VISIBLE.store(true, Ordering::SeqCst);
}

pub fn undraw() {
    if !CURSOR_VISIBLE.load(Ordering::SeqCst) {
        return;
    }
    let x = CURSOR_X.load(Ordering::SeqCst);
    let y = CURSOR_Y.load(Ordering::SeqCst);
    unsafe {
        restore_pixels(x, y);
    }
    CURSOR_VISIBLE.store(false, Ordering::SeqCst);
}

pub fn move_cursor(new_x: u16, new_y: u16) {
    let fb_w = unsafe { crate::drivers::fbcon::fb_width() };
    let fb_h = unsafe { crate::drivers::fbcon::fb_height() };
    let (x, y) = clamp_position(new_x as i32, new_y as i32, fb_w as u16, fb_h as u16);

    undraw();
    draw(x, y);
}



unsafe fn restore_pixels(x: u16, y: u16) {
    let fb = crate::drivers::fbcon::get_framebuffer();
    let fb_width = fb.info.width as usize;

    for cy in 0..16u16 {
        for cx in 0..16u16 {
            let px = (x + cx) as usize;
            let py = (y + cy) as usize;
            if px < fb_width && py < fb.info.height as usize {
                let pixel = SAVED_PIXELS[cy as usize][cx as usize];
                let offset = (py * fb_width + px) * (fb.info.bpp as usize / 8);
                if fb.info.bpp == 32 {
                    let ptr = fb.ptr.add(offset) as *mut u32;
                    core::ptr::write_volatile(ptr, pixel);
                }
            }
        }
    }
}
