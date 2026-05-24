//! User-space WindowServer for VibeOS
//! Owns the framebuffer, composites windows, handles input, draws UI chrome.
#![no_std]
#![no_main]

extern crate alloc;

extern crate libvibe;

use alloc::vec::Vec;
use libvibe::*;

// ── Bump allocator for user-space app ──────────────────────────────────────
const HEAP_SIZE: usize = 2 * 1024 * 1024; // 2 MB for WindowServer
static mut HEAP: [u8; HEAP_SIZE] = [0; HEAP_SIZE];

struct BumpAlloc {
    next: core::sync::atomic::AtomicUsize,
    base: core::sync::atomic::AtomicUsize,
}

unsafe impl alloc::alloc::GlobalAlloc for BumpAlloc {
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        let size = layout.size();
        let align = layout.align();
        // Lazily initialize base on first allocation
        let base = {
            let b = self.base.load(core::sync::atomic::Ordering::Relaxed);
            if b == 0 {
                let heap_base = HEAP.as_ptr() as usize;
                self.base
                    .store(heap_base, core::sync::atomic::Ordering::Relaxed);
                self.next
                    .store(heap_base, core::sync::atomic::Ordering::Relaxed);
                heap_base
            } else {
                b
            }
        };
        loop {
            let current = self.next.load(core::sync::atomic::Ordering::Relaxed);
            let aligned = (current + align - 1) & !(align - 1);
            let new = aligned + size;
            if new > base + HEAP_SIZE {
                return core::ptr::null_mut();
            }
            if self
                .next
                .compare_exchange_weak(
                    current,
                    new,
                    core::sync::atomic::Ordering::Relaxed,
                    core::sync::atomic::Ordering::Relaxed,
                )
                .is_ok()
            {
                return aligned as *mut u8;
            }
        }
    }
    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: core::alloc::Layout) {
        // Bump allocator: no deallocation
    }
}

#[global_allocator]
static ALLOCATOR: BumpAlloc = BumpAlloc {
    next: core::sync::atomic::AtomicUsize::new(0),
    base: core::sync::atomic::AtomicUsize::new(0),
};

const MAX_WINDOWS: usize = 32;
const TITLE_BAR_H: u16 = 36;
const MENU_BAR_H: u16 = 28;
const DOCK_H: u16 = 70;
const DOCK_W: u16 = 500;
const ICON_SIZE: u16 = 48;
const ICON_SPACING: u16 = 12;
const NUM_DOCK_ICONS: usize = 5;
const CURSOR_SIZE: u16 = 16;

// Colors
const COL_BG_TOP: u32 = 0xFF001A3A;
const COL_BG_BOT: u32 = 0xFF000A1A;
const COL_MENU_TOP: u32 = 0xFF1A1A2E;
const COL_MENU_BOT: u32 = 0xFF12121E;
const COL_MENU_BORDER: u32 = 0xFF333344;
const COL_DOCK_BG: u32 = 0x802A2A3A;
const COL_DOCK_BORDER: u32 = 0x80444455;
const COL_WIN_BG: u32 = 0xFF1E1E2E;
const COL_WIN_TITLE: u32 = 0xFF2A2A3A;
const COL_WIN_STATUS: u32 = 0xFF222233;
const COL_WHITE: u32 = 0xFFFFFFFF;
const COL_TEXT_DIM: u32 = 0xFFAAAAAA;
const COL_TEXT_DARK: u32 = 0xFF888888;
const COL_SHADOW: u32 = 0x80000000;
const COL_ICONS: [u32; 5] = [0xFF007AFF, 0xFF34C759, 0xFFFF9500, 0xFFFF3B30, 0xFF5856D6];
const COL_CLOSE: u32 = 0xFFFF5F57;
const COL_MINIMIZE: u32 = 0xFFFEBC2E;
const COL_MAXIMIZE: u32 = 0xFF28C840;
const COL_HOVER: u32 = 0x20FFFFFF;
const COL_HOVER_LIGHT: u32 = 0x15FFFFFF;
const COL_CURSOR_FG: u32 = 0xFFFFFFFF;
const COL_CURSOR_BG: u32 = 0xFF000000;

// Cursor bitmap (16x16 arrow, same as kernel cursor.rs)
const CURSOR_BITMAP: &[u8; 32] = &[
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

// Font data (8x16 bitmap, same as kernel fbcon.rs)
const FONT_W: usize = 8;
const FONT_H: usize = 16;
include!("font_data.rs");

/// Window structure
struct Window {
    id: u16,
    x: u16,
    y: u16,
    w: u16,
    h: u16,
    visible: bool,
    minimized: bool,
    maximized: bool,
    orig_x: u16,
    orig_y: u16,
    orig_w: u16,
    orig_h: u16,
    title: [u8; 32],
    title_len: usize,
    owner_pid: usize,
}

impl Window {
    fn new(id: u16, x: u16, y: u16, w: u16, h: u16, owner: usize) -> Self {
        let mut title = [0u8; 32];
        title[0..14].copy_from_slice(b"VibeOS Window");
        Window {
            id,
            x,
            y,
            w,
            h,
            visible: true,
            minimized: false,
            maximized: false,
            orig_x: x,
            orig_y: y,
            orig_w: w,
            orig_h: h,
            title,
            title_len: 14,
            owner_pid: owner,
        }
    }
}

/// Framebuffer state
struct Framebuffer {
    ptr: *mut u8,
    width: usize,
    height: usize,
    pitch: usize,
    bpp: usize,
}

impl Framebuffer {
    fn set_pixel(&self, x: usize, y: usize, color: u32) {
        if x >= self.width || y >= self.height || self.ptr.is_null() {
            return;
        }
        let off = y * self.pitch + x * 4;
        unsafe {
            let buf = self.ptr.add(off);
            *buf = color as u8;           // B
            *buf.add(1) = (color >> 8) as u8;  // G
            *buf.add(2) = (color >> 16) as u8; // R
            *buf.add(3) = (color >> 24) as u8; // A
        }
    }

    fn get_pixel(&self, x: usize, y: usize) -> u32 {
        if x >= self.width || y >= self.height || self.ptr.is_null() {
            return 0;
        }
        let off = y * self.pitch + x * 4;
        unsafe {
            let buf = self.ptr.add(off);
            let b = *buf as u32;
            let g = *buf.add(1) as u32;
            let r = *buf.add(2) as u32;
            (r << 16) | (g << 8) | b | 0xFF000000
        }
    }

    fn fill_rect(&self, x: usize, y: usize, w: usize, h: usize, color: u32) {
        let max_y = (y + h).min(self.height);
        let max_x = (x + w).min(self.width);
        for cy in y..max_y {
            for cx in x..max_x {
                self.set_pixel(cx, cy, color);
            }
        }
    }

    fn blend_pixel(&self, x: usize, y: usize, color: u32) {
        if x >= self.width || y >= self.height || self.ptr.is_null() {
            return;
        }
        let alpha = ((color >> 24) & 0xFF) as u32;
        if alpha == 0 { return; }
        if alpha == 255 {
            self.set_pixel(x, y, color);
            return;
        }
        let bg = self.get_pixel(x, y);
        let bg_r = (bg >> 16) & 0xFF;
        let bg_g = (bg >> 8) & 0xFF;
        let bg_b = bg & 0xFF;
        let fg_r = (color >> 16) & 0xFF;
        let fg_g = (color >> 8) & 0xFF;
        let fg_b = color & 0xFF;
        let inv = 255 - alpha;
        let r = ((fg_r * alpha + bg_r * inv) / 255) as u8;
        let g = ((fg_g * alpha + bg_g * inv) / 255) as u8;
        let b = ((fg_b * alpha + bg_b * inv) / 255) as u8;
        self.set_pixel(x, y, 0xFF000000 | ((r as u32) << 16) | ((g as u32) << 8) | b as u32);
    }

    fn fill_rect_blend(&self, x: usize, y: usize, w: usize, h: usize, color: u32) {
        let max_y = (y + h).min(self.height);
        let max_x = (x + w).min(self.width);
        for cy in y..max_y {
            for cx in x..max_x {
                self.blend_pixel(cx, cy, color);
            }
        }
    }

    fn fill_v_gradient(&self, x: usize, y: usize, w: usize, h: usize, top: u32, bot: u32) {
        let tr = (top >> 16) & 0xFF;
        let tg = (top >> 8) & 0xFF;
        let tb = top & 0xFF;
        let br = (bot >> 16) & 0xFF;
        let bg = (bot >> 8) & 0xFF;
        let bb = bot & 0xFF;
        let max_y = (y + h).min(self.height);
        let max_x = (x + w).min(self.width);
        for cy in y..max_y {
            let t = if h > 1 { (cy - y) as u32 * 255 / (h as u32 - 1) } else { 0 };
            let r = tr as i32 + ((br as i32 - tr as i32) * t as i32 / 255);
            let g = tg as i32 + ((bg as i32 - tg as i32) * t as i32 / 255);
            let b = tb as i32 + ((bb as i32 - tb as i32) * t as i32 / 255);
            let color = 0xFF000000 | ((r as u32 & 0xFF) << 16) | ((g as u32 & 0xFF) << 8) | (b as u32 & 0xFF);
            for cx in x..max_x {
                self.set_pixel(cx, cy, color);
            }
        }
    }

    fn fill_circle(&self, cx: usize, cy: usize, r: usize, color: u32) {
        let r2 = (r * r) as i32;
        for dy in -(r as i32)..=(r as i32) {
            for dx in -(r as i32)..=(r as i32) {
                if dx * dx + dy * dy <= r2 {
                    let px = (cx as i32 + dx) as usize;
                    let py = (cy as i32 + dy) as usize;
                    self.set_pixel(px, py, color);
                }
            }
        }
    }

    fn fill_rounded_rect(&self, x: usize, y: usize, w: usize, h: usize, radius: usize, color: u32) {
        self.fill_rect(x + radius, y, w - 2 * radius, h, color);
        self.fill_rect(x, y + radius, w, h - 2 * radius, color);
        self.fill_circle(x + radius, y + radius, radius, color);
        self.fill_circle(x + w - 1 - radius, y + radius, radius, color);
        self.fill_circle(x + radius, y + h - 1 - radius, radius, color);
        self.fill_circle(x + w - 1 - radius, y + h - 1 - radius, radius, color);
    }

    fn draw_hline(&self, x: usize, y: usize, w: usize, color: u32) {
        self.fill_rect(x, y, w, 1, color);
    }

    fn draw_shadow(&self, x: usize, y: usize, w: usize, h: usize, offset: usize) {
        self.fill_rect_blend(x + w, y + offset, offset, h - offset, 0x80000000);
        self.fill_rect_blend(x + offset, y + h, w - offset, offset, 0x80000000);
        self.fill_rect_blend(x + w, y + h, offset, offset, 0x60000000);
    }

    fn draw_char(&self, x: usize, y: usize, ch: char, color: u32) {
        let idx = if (ch as u32) >= 32 && (ch as u32) <= 126 {
            (ch as usize) - 32
        } else {
            0
        };
        let glyph = &FONT_DATA[idx];
        for row in 0..16usize {
            let bits = glyph[row];
            for col in 0..8usize {
                if bits & (0x80 >> col) != 0 {
                    self.set_pixel(x + col, y + row, color);
                }
            }
        }
    }

    fn draw_str(&self, x: usize, y: usize, s: &str, color: u32) {
        let mut cx = x;
        for ch in s.chars() {
            if ch == '\n' {
                continue;
            }
            self.draw_char(cx, y, ch, color);
            cx += FONT_W;
        }
    }

    fn clear(&self, color: u32) {
        self.fill_rect(0, 0, self.width, self.height, color);
    }
}

/// Hit test targets
enum HitTarget {
    None,
    TrafficLightClose,
    TrafficLightMin,
    TrafficLightMax,
    TitleBar(u16),   // window index
    WindowBody(u16), // window index
    DockIcon(usize),
}

/// Saved cursor pixels for undraw
static mut SAVED_CURSOR: [[u32; 16]; 16] = [[0; 16]; 16];

/// WindowServer state
struct WindowServer {
    fb: Framebuffer,
    windows: Vec<Window>,
    next_win_id: u16,
    focused_win: u16, // window id
    cursor_x: u16,
    cursor_y: u16,
    cursor_visible: bool,
    dragging: bool,
    drag_win: u16,
    drag_off_x: i32,
    drag_off_y: i32,
    hovered_dock: Option<usize>,
    needs_redraw: bool,
}

impl WindowServer {
    fn new(fb: Framebuffer) -> Self {
        WindowServer {
            fb,
            windows: Vec::new(),
            next_win_id: 1,
            focused_win: 0,
            cursor_x: 400,
            cursor_y: 300,
            cursor_visible: false,
            dragging: false,
            drag_win: 0,
            drag_off_x: 0,
            drag_off_y: 0,
            hovered_dock: None,
            needs_redraw: true,
        }
    }

    fn dock_x(&self) -> u16 {
        if self.fb.width > DOCK_W as usize {
            ((self.fb.width - DOCK_W as usize) / 2) as u16
        } else {
            0
        }
    }

    fn dock_y(&self) -> u16 {
        (self.fb.height - DOCK_H as usize) as u16
    }

    fn dock_icon_x(&self, i: usize) -> u16 {
        self.dock_x() + 16 + (i as u16 * (ICON_SIZE + ICON_SPACING))
    }

    fn dock_icon_y(&self) -> u16 {
        self.dock_y() + 8
    }

    fn find_window_idx(&self, id: u16) -> Option<usize> {
        self.windows.iter().position(|w| w.id == id)
    }

    fn create_window(&mut self, x: u16, y: u16, w: u16, h: u16, owner: usize) -> u16 {
        let id = self.next_win_id;
        self.next_win_id += 1;
        let win = Window::new(id, x, y, w, h, owner);
        self.windows.push(win);
        self.focused_win = id;
        self.needs_redraw = true;
        id
    }

    fn destroy_window(&mut self, id: u16) {
        if let Some(idx) = self.find_window_idx(id) {
            self.windows.remove(idx);
            if self.focused_win == id {
                self.focused_win = self.windows.last().map(|w| w.id).unwrap_or(0);
            }
            self.needs_redraw = true;
        }
    }

    fn hit_test(&self, x: u16, y: u16) -> HitTarget {
        // Check windows in reverse (topmost first — painter's algorithm)
        for win in self.windows.iter().rev() {
            if !win.visible {
                continue;
            }
            let wx = win.x as usize;
            let wy = win.y as usize;
            let ww = win.w as usize;
            let wh = win.h as usize;

            // Traffic lights
            let light_y = win.y + 12 + 6;
            if circle_hit(x, y, win.x + 16, light_y, 6) {
                return HitTarget::TrafficLightClose;
            }
            if circle_hit(x, y, win.x + 32, light_y, 6) {
                return HitTarget::TrafficLightMin;
            }
            if circle_hit(x, y, win.x + 48, light_y, 6) {
                return HitTarget::TrafficLightMax;
            }

            // Title bar
            if x >= win.x && x < win.x + win.w && y >= win.y && y < win.y + TITLE_BAR_H {
                return HitTarget::TitleBar(win.id);
            }

            // Window body
            if x >= win.x && x < win.x + win.w && y >= win.y + TITLE_BAR_H && y < win.y + win.h {
                return HitTarget::WindowBody(win.id);
            }
        }

        // Dock icons
        if y >= self.dock_y() && y < self.dock_y() + DOCK_H {
            for i in 0..NUM_DOCK_ICONS {
                let ix = self.dock_icon_x(i);
                if x >= ix && x < ix + ICON_SIZE {
                    return HitTarget::DockIcon(i);
                }
            }
        }

        HitTarget::None
    }

    fn hit_test_dock(&self, x: u16, y: u16) -> Option<usize> {
        if y >= self.dock_y() && y < self.dock_y() + DOCK_H {
            for i in 0..NUM_DOCK_ICONS {
                let ix = self.dock_icon_x(i);
                if x >= ix && x < ix + ICON_SIZE {
                    return Some(i);
                }
            }
        }
        None
    }

    fn undraw_cursor(&mut self) {
        if !self.cursor_visible {
            return;
        }
        let x = self.cursor_x as usize;
        let y = self.cursor_y as usize;
        for cy in 0..16usize {
            for cx in 0..16usize {
                let px = x + cx;
                let py = y + cy;
                if px < self.fb.width && py < self.fb.height {
                    let pixel = unsafe { SAVED_CURSOR[cy][cx] };
                    self.fb.set_pixel(px, py, pixel);
                }
            }
        }
        self.cursor_visible = false;
    }

    fn draw_cursor(&mut self) {
        let x = self.cursor_x as usize;
        let y = self.cursor_y as usize;
        // Save pixels and draw
        for cy in 0..16usize {
            for cx in 0..16usize {
                let px = x + cx;
                let py = y + cy;
                if px < self.fb.width && py < self.fb.height {
                    unsafe { SAVED_CURSOR[cy][cx] = self.fb.get_pixel(px, py); }
                }
            }
        }
        // Draw cursor bitmap
        for cy in 0..16usize {
            for cx in 0..16usize {
                let bit_index = cy * 16 + cx;
                let byte_idx = bit_index / 8;
                let bit_idx = 7 - (bit_index % 8);
                if (CURSOR_BITMAP[byte_idx] >> bit_idx) & 1 != 0 {
                    let px = x + cx;
                    let py = y + cy;
                    if px < self.fb.width && py < self.fb.height {
                        // Draw outline first
                        if cx > 0 && (CURSOR_BITMAP[(cy * 16 + cx - 1) / 8] >> (7 - ((cy * 16 + cx - 1) % 8))) & 1 == 0 {
                            self.fb.set_pixel(px - 1, py, COL_CURSOR_BG);
                        }
                        if cx < 15 && (CURSOR_BITMAP[(cy * 16 + cx + 1) / 8] >> (7 - ((cy * 16 + cx + 1) % 8))) & 1 == 0 {
                            self.fb.set_pixel(px + 1, py, COL_CURSOR_BG);
                        }
                        self.fb.set_pixel(px, py, COL_CURSOR_FG);
                    }
                }
            }
        }
        self.cursor_visible = true;
    }

    fn move_cursor(&mut self, x: u16, y: u16) {
        let max_x = if self.fb.width > 16 { (self.fb.width - 16) as u16 } else { 0 };
        let max_y = if self.fb.height > 16 { (self.fb.height - 16) as u16 } else { 0 };
        let nx = x.min(max_x);
        let ny = y.min(max_y);
        if nx != self.cursor_x || ny != self.cursor_y {
            self.undraw_cursor();
            self.cursor_x = nx;
            self.cursor_y = ny;
            self.draw_cursor();
        }
    }

    fn draw_desktop(&self) {
        // Background gradient
        self.fb.fill_v_gradient(0, 0, self.fb.width, self.fb.height, COL_BG_TOP, COL_BG_BOT);

        // Menu bar
        self.fb.fill_v_gradient(0, 0, self.fb.width, MENU_BAR_H as usize, COL_MENU_TOP, COL_MENU_BOT);
        self.fb.draw_hline(0, MENU_BAR_H as usize, self.fb.width, COL_MENU_BORDER);

        // Menu bar text
        self.fb.draw_str(8, 6, "VibeOS", COL_WHITE);
        self.fb.draw_str(80, 6, "Finder", COL_WHITE);
        self.fb.draw_str(150, 6, "File", 0xFFD0D0D0);
        self.fb.draw_str(190, 6, "Edit", 0xFFD0D0D0);
        self.fb.draw_str(230, 6, "View", 0xFFD0D0D0);
        self.fb.draw_str(275, 6, "Go", 0xFFD0D0D0);
        self.fb.draw_str(300, 6, "Window", 0xFFD0D0D0);
        self.fb.draw_str(360, 6, "Help", 0xFFD0D0D0);
        self.fb.draw_str(self.fb.width - 80, 6, "04:20", 0xFFD0D0D0);

        // Dock
        let dk_x = self.dock_x() as usize;
        let dk_y = self.dock_y() as usize;
        self.fb.fill_rounded_rect(dk_x, dk_y, DOCK_W as usize, DOCK_H as usize, 16, COL_DOCK_BG);
        self.fb.draw_hline(dk_x + 2, dk_y + 2, DOCK_W as usize - 4, COL_DOCK_BORDER);

        // Dock icons
        for i in 0..NUM_DOCK_ICONS {
            let ix = self.dock_icon_x(i) as usize;
            let iy = self.dock_icon_y() as usize;
            let is_hovered = self.hovered_dock == Some(i);

            if is_hovered {
                self.fb.fill_rounded_rect(ix - 4, iy - 6, ICON_SIZE as usize + 8, ICON_SIZE as usize + 12, 10, COL_HOVER);
            }
            // Shadow
            self.fb.fill_rounded_rect(ix + 2, iy + 4, ICON_SIZE as usize, ICON_SIZE as usize, 8, 0x40000000);
            // Icon
            self.fb.fill_rounded_rect(ix, iy, ICON_SIZE as usize, ICON_SIZE as usize, 8, COL_ICONS[i]);
            // Highlight top half
            self.fb.fill_rounded_rect(ix, iy, ICON_SIZE as usize, ICON_SIZE as usize / 2, 8, 0x20FFFFFF);
            if is_hovered {
                self.fb.fill_rounded_rect(ix, iy, ICON_SIZE as usize, ICON_SIZE as usize, 8, COL_HOVER_LIGHT);
            }
        }
    }

    fn draw_window(&self, win: &Window) {
        if !win.visible {
            return;
        }
        let wx = win.x as usize;
        let wy = win.y as usize;
        let ww = win.w as usize;
        let wh = win.h as usize;

        // Shadow
        self.fb.draw_shadow(wx, wy, ww, wh, 8);

        // Window background
        self.fb.fill_rounded_rect(wx, wy, ww, wh, 10, COL_WIN_BG);

        // Title bar
        self.fb.fill_rounded_rect(wx, wy, ww, TITLE_BAR_H as usize, 10, COL_WIN_TITLE);
        self.fb.fill_rect(wx, wy + TITLE_BAR_H as usize - 10, ww, 10, COL_WIN_TITLE);

        // Traffic lights
        let light_y = wy + 12 + 6;
        self.fb.fill_circle(wx + 16, light_y, 6, COL_CLOSE);
        self.fb.fill_circle(wx + 32, light_y, 6, COL_MINIMIZE);
        self.fb.fill_circle(wx + 48, light_y, 6, COL_MAXIMIZE);

        // Title text
        let title_str = unsafe {
            core::str::from_utf8_unchecked(&win.title[..win.title_len])
        };
        self.fb.draw_str(wx + 60, wy + 10, title_str, COL_WHITE);

        // Window body text
        self.fb.draw_str(wx + 20, wy + 60, "Vibe Coded OS v0.1.0", COL_WHITE);
        self.fb.draw_str(wx + 20, wy + 80, "WindowServer: User-space", COL_TEXT_DIM);

        // Separator
        self.fb.draw_hline(wx + 20, wy + 110, ww - 40, COL_MENU_BORDER);

        // Status bar at bottom
        self.fb.fill_rect(wx, wy + wh - 24, ww, 24, COL_WIN_STATUS);
        self.fb.draw_str(wx + 10, wy + wh - 20, "UTF-8  LF  100%", COL_TEXT_DARK);
    }

    fn redraw_all(&mut self) {
        self.draw_desktop();
        // Draw windows in order (painter's algorithm)
        for win in &self.windows {
            self.draw_window(win);
        }
        self.draw_cursor();
        self.needs_redraw = false;
    }

    fn handle_mouse_move(&mut self, x: u16, y: u16) {
        if self.dragging {
            if let Some(idx) = self.find_window_idx(self.drag_win) {
                let win = &mut self.windows[idx];
                if !win.maximized {
                    let new_x = (x as i32 + self.drag_off_x).max(0).min(self.fb.width as i32 - 100) as u16;
                    let new_y = (y as i32 + self.drag_off_y).max(0).min(self.fb.height as i32 - 50) as u16;
                    if new_x != win.x || new_y != win.y {
                        win.x = new_x;
                        win.y = new_y;
                        self.needs_redraw = true;
                    }
                }
            }
        }
        let hovered = self.hit_test_dock(x, y);
        if hovered != self.hovered_dock {
            self.hovered_dock = hovered;
            self.needs_redraw = true;
        }
        self.move_cursor(x, y);
    }

    fn handle_mouse_down(&mut self, button: u8, x: u16, y: u16) {
        if button != 0 {
            return; // only handle left click
        }
        let target = self.hit_test(x, y);
        match target {
            HitTarget::TrafficLightClose => {
                if let Some(idx) = self.find_window_idx(self.focused_win) {
                    let win = &self.windows[idx];
                    if win.visible {
                        let win_id = win.id;
                        self.destroy_window(win_id);
                    }
                }
            }
            HitTarget::TrafficLightMin => {
                if let Some(idx) = self.find_window_idx(self.focused_win) {
                    let win = &mut self.windows[idx];
                    if win.visible && !win.minimized {
                        win.minimized = true;
                        win.visible = false;
                        self.needs_redraw = true;
                    }
                }
            }
            HitTarget::TrafficLightMax => {
                if let Some(idx) = self.find_window_idx(self.focused_win) {
                    let win = &mut self.windows[idx];
                    if win.visible {
                        if win.maximized {
                            win.x = win.orig_x;
                            win.y = win.orig_y;
                            win.w = win.orig_w;
                            win.h = win.orig_h;
                            win.maximized = false;
                        } else {
                            win.orig_x = win.x;
                            win.orig_y = win.y;
                            win.orig_w = win.w;
                            win.orig_h = win.h;
                            win.x = 0;
                            win.y = MENU_BAR_H;
                            win.w = self.fb.width as u16;
                            win.h = (self.fb.height - DOCK_H as usize - MENU_BAR_H as usize) as u16;
                            win.maximized = true;
                            win.minimized = false;
                        }
                        win.visible = true;
                        self.needs_redraw = true;
                    }
                }
            }
            HitTarget::TitleBar(win_id) => {
                self.focused_win = win_id;
                if let Some(idx) = self.find_window_idx(win_id) {
                    let win = &self.windows[idx];
                    if !win.maximized {
                        self.dragging = true;
                        self.drag_win = win_id;
                        self.drag_off_x = win.x as i32 - x as i32;
                        self.drag_off_y = win.y as i32 - y as i32;
                    }
                }
            }
            HitTarget::WindowBody(win_id) => {
                self.focused_win = win_id;
            }
            HitTarget::DockIcon(i) => {
                // Dock icon click: if a window is minimized, restore it; otherwise create default window
                let mut restored = false;
                for win in &mut self.windows {
                    if win.minimized {
                        win.visible = true;
                        win.minimized = false;
                        restored = true;
                        self.needs_redraw = true;
                        break;
                    }
                }
                if !restored && self.windows.len() < MAX_WINDOWS {
                    // Create a new window for this dock icon
                    let offset = self.windows.len() as u16 * 20;
                    let _ = self.create_window(
                        100 + offset,
                        80 + offset,
                        600,
                        400,
                        0, // no owner
                    );
                }
            }
            HitTarget::None => {}
        }
    }

    fn handle_mouse_up(&mut self) {
        self.dragging = false;
    }

    fn handle_key_press(&mut self, _ascii: u8) {
        // Route keypress to focused window's owner via IPC
        if self.focused_win > 0 {
            if let Some(_idx) = self.find_window_idx(self.focused_win) {
                let owner = self.windows[self.find_window_idx(self.focused_win).unwrap()].owner_pid;
                if owner > 0 {
                    // Forward key event via IPC
                    let mut msg = [0u8; IPC_PAYLOAD_SIZE];
                    msg[0] = MSG_INPUT_EVENT;
                    msg[1] = INPUT_KEY_PRESS;
                    msg[2] = _ascii;
                    ipc_send(owner, &msg);
                }
            }
        }
    }

    fn handle_ipc_message(&mut self, msg: &[u8; IPC_PAYLOAD_SIZE]) {
        match msg[0] {
            MSG_CREATE_WINDOW => {
                let x = u16::from_le_bytes([msg[1], msg[2]]);
                let y = u16::from_le_bytes([msg[3], msg[4]]);
                let w = u16::from_le_bytes([msg[5], msg[6]]);
                let h = u16::from_le_bytes([msg[7], msg[8]]);
                if self.windows.len() < MAX_WINDOWS {
                    let win_id = self.create_window(x, y, w, h, 0);
                    // Send WindowReady back to sender
                    let _ = win_id; // In a real implementation, we'd reply via IPC
                }
            }
            MSG_DESTROY_WINDOW => {
                let win_id = u16::from_le_bytes([msg[1], msg[2]]);
                self.destroy_window(win_id);
            }
            MSG_MOVE_WINDOW => {
                let win_id = u16::from_le_bytes([msg[1], msg[2]]);
                let x = u16::from_le_bytes([msg[3], msg[4]]);
                let y = u16::from_le_bytes([msg[5], msg[6]]);
                if let Some(idx) = self.find_window_idx(win_id) {
                    self.windows[idx].x = x;
                    self.windows[idx].y = y;
                    self.needs_redraw = true;
                }
            }
            MSG_RESIZE_WINDOW => {
                let win_id = u16::from_le_bytes([msg[1], msg[2]]);
                let w = u16::from_le_bytes([msg[3], msg[4]]);
                let h = u16::from_le_bytes([msg[5], msg[6]]);
                if let Some(idx) = self.find_window_idx(win_id) {
                    self.windows[idx].w = w;
                    self.windows[idx].h = h;
                    self.needs_redraw = true;
                }
            }
            _ => {}
        }
    }

    fn run(&mut self) -> ! {
        // Create initial default window
        self.create_window(100, 80, 600, 400, 0);

        // Show cursor
        self.cursor_x = 400.min(self.fb.width as u16 - 16);
        self.cursor_y = 300.min(self.fb.height as u16 - 16);

        // Initial draw
        self.redraw_all();

        loop {
            // Poll for input events
            while let Some(event) = input_poll() {
                if event.is_mouse_move() {
                    self.handle_mouse_move(event.x, event.y);
                } else if event.is_mouse_down() {
                    self.handle_mouse_down(event.button(), event.x, event.y);
                } else if event.is_mouse_up() {
                    self.handle_mouse_up();
                } else if event.is_key_press() {
                    self.handle_key_press(event.ascii());
                }
            }

            // Poll for IPC messages
            let mut msg = [0u8; IPC_PAYLOAD_SIZE];
            let result = ipc_recv(&mut msg);
            if result != 0 {
                self.handle_ipc_message(&msg);
            }

            // Redraw if needed
            if self.needs_redraw {
                self.redraw_all();
            }

            // Yield CPU
            yield_cpu();
        }
    }
}

fn circle_hit(px: u16, py: u16, cx: u16, cy: u16, r: u16) -> bool {
    let dx = (px as i32 - cx as i32).abs();
    let dy = (py as i32 - cy as i32).abs();
    (dx * dx + dy * dy) <= (r as i32 * r as i32)
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    // Map the framebuffer
    let mut fb_info = FramebufferInfo {
        addr: 0,
        width: 0,
        height: 0,
        pitch: 0,
        bpp: 0,
    };
    let fb_addr = framebuffer_map(&mut fb_info as *mut FramebufferInfo);

    let fb = Framebuffer {
        ptr: fb_addr as *mut u8,
        width: fb_info.width as usize,
        height: fb_info.height as usize,
        pitch: fb_info.pitch as usize,
        bpp: fb_info.bpp as usize,
    };

    let mut server = WindowServer::new(fb);
    server.run()
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}