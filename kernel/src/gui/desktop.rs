//! Minimal interactive desktop.
//!
//! Draws a background, a taskbar, and a clickable "Terminal" button.
//! Clicking the button creates a simple terminal window that receives
//! keyboard input.

#![allow(static_mut_refs)]

use super::{clear_window, create_window, draw_text, Color, WindowId, COMPOSITOR};
use crate::arch::{mouse_buttons, mouse_position};

const TASKBAR_HEIGHT: i32 = 28;
const BUTTON_WIDTH: i32 = 80;
const BUTTON_HEIGHT: i32 = 22;
const BUTTON_X: i32 = 8;
const BUTTON_Y: i32 = 3;

static mut DESKTOP_WINDOW: Option<WindowId> = None;
static mut TERMINAL_WINDOW: Option<WindowId> = None;
static mut BUTTON_DOWN_LAST: bool = false;
static mut TERMINAL_TEXT_LEN: usize = 0;
static mut TERMINAL_TEXT: [u8; 256] = [0; 256];

/// Initialize the desktop once the compositor exists.
pub fn init(screen_width: i32, screen_height: i32) {
    let desktop = create_window("Desktop", 0, 0, screen_width, screen_height);
    unsafe { DESKTOP_WINDOW = desktop };
    draw_desktop();
}

/// Draw the desktop background and taskbar button.
pub fn draw_desktop() {
    let Some(desktop) = (unsafe { DESKTOP_WINDOW }) else { return };
    let _ = desktop; // used to identify the desktop window later.

    // The desktop window covers the whole screen. We render the taskbar by
    // drawing directly into it; the compositor will paint it bottom-most.
    clear_window(Some(desktop), Color::new(0x20, 0x40, 0x60));
    draw_taskbar(desktop);
}

fn draw_taskbar(desktop: WindowId) {
    // Taskbar background at the top.
    fill_rect(desktop, 0, 0, desktop_bounds().0, TASKBAR_HEIGHT, Color::new(0x10, 0x10, 0x10));
    // Button background.
    fill_rect(desktop, BUTTON_X, BUTTON_Y, BUTTON_X + BUTTON_WIDTH, BUTTON_Y + BUTTON_HEIGHT, Color::new(0x40, 0x40, 0x40));
    // Button border.
    draw_rect(desktop, BUTTON_X, BUTTON_Y, BUTTON_X + BUTTON_WIDTH, BUTTON_Y + BUTTON_HEIGHT, Color::WHITE);
    draw_text(Some(desktop), "Terminal", BUTTON_X + 8, BUTTON_Y + 7, Color::WHITE);
}

fn desktop_bounds() -> (i32, i32) {
    // The desktop is created at (0,0) with the full screen size. That size is
    // not stored here; 1280x1024 matches the default QEMU resolution used by
    // the project. A real implementation would query the compositor.
    (1280, 1024)
}

/// Process mouse input. Returns `true` if a click was handled.
pub fn handle_mouse() -> bool {
    let (mx, my) = mouse_position();
    let buttons = mouse_buttons();
    let left_down = (buttons & 1) != 0;
    let mut clicked = false;

    unsafe {
        if left_down && !BUTTON_DOWN_LAST {
            // Rising edge: check hit.
            if point_in_rect(mx, my, BUTTON_X, BUTTON_Y, BUTTON_X + BUTTON_WIDTH, BUTTON_Y + BUTTON_HEIGHT) {
                open_terminal();
                clicked = true;
            }
        }
        BUTTON_DOWN_LAST = left_down;
    }

    clicked
}

/// Feed a typed character to the active terminal window.
pub fn type_char(ch: char) {
    unsafe {
        if TERMINAL_WINDOW.is_none() {
            return;
        }
        match ch {
            '\n' => TERMINAL_TEXT_LEN = 0,
            '\u{8}' => {
                if TERMINAL_TEXT_LEN > 0 {
                    TERMINAL_TEXT_LEN -= 1;
                }
            }
            _ => {
                if TERMINAL_TEXT_LEN < TERMINAL_TEXT.len() && ch.is_ascii() {
                    TERMINAL_TEXT[TERMINAL_TEXT_LEN] = ch as u8;
                    TERMINAL_TEXT_LEN += 1;
                }
            }
        }
        redraw_terminal();
    }
}

fn open_terminal() {
    unsafe {
        if TERMINAL_WINDOW.is_none() {
            let id = create_window("Terminal", 100, 100, 480, 320);
            TERMINAL_WINDOW = id;
        }
        redraw_terminal();
    }
}

fn redraw_terminal() {
    unsafe {
        let Some(term) = TERMINAL_WINDOW else { return };
        clear_window(Some(term), Color::DARK_GRAY);
        draw_text(Some(term), "Aperture Terminal", 12, 12, Color::WHITE);
        let line = core::str::from_utf8(&TERMINAL_TEXT[..TERMINAL_TEXT_LEN]).unwrap_or("");
        draw_text(Some(term), line, 12, 32, Color::new(0x00, 0xFF, 0x00));
    }
}

fn point_in_rect(x: i32, y: i32, x0: i32, y0: i32, x1: i32, y1: i32) -> bool {
    x >= x0 && x < x1 && y >= y0 && y < y1
}

fn fill_rect(window: WindowId, x0: i32, y0: i32, x1: i32, y1: i32, color: Color) {
    for y in y0..y1 {
        for x in x0..x1 {
            write_pixel(window, x, y, color);
        }
    }
}

fn draw_rect(window: WindowId, x0: i32, y0: i32, x1: i32, y1: i32, color: Color) {
    for x in x0..x1 {
        write_pixel(window, x, y0, color);
        write_pixel(window, x, y1 - 1, color);
    }
    for y in y0..y1 {
        write_pixel(window, x0, y, color);
        write_pixel(window, x1 - 1, y, color);
    }
}

fn write_pixel(window: WindowId, x: i32, y: i32, color: Color) {
    let mut guard = COMPOSITOR.lock();
    let Some(c) = guard.as_mut() else { return };
    let Some(w) = c.window_mut(window) else { return };
    if x < 0 || y < 0 || x >= w.width || y >= w.height {
        return;
    }
    let index = (y * w.width + x) as usize;
    unsafe { core::slice::from_raw_parts_mut(w.backbuffer, w.pixel_count)[index] = color };
}
