//! User-space Terminal Emulator for VibeOS
//! Creates a window, opens a PTY, spawns a shell, renders text output, routes keyboard input.
#![no_std]
#![no_main]

extern crate alloc;
extern crate libvibe;

use alloc::string::String;
use alloc::vec::Vec;
use libvibe::*;

// ── Bump allocator for user-space app ──────────────────────────────────────
const HEAP_SIZE: usize = 1024 * 1024; // 1 MB
static mut HEAP: [u8; HEAP_SIZE] = [0; HEAP_SIZE];

struct BumpAlloc {
    next: core::sync::atomic::AtomicUsize,
    base: core::sync::atomic::AtomicUsize,
}

unsafe impl alloc::alloc::GlobalAlloc for BumpAlloc {
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        let size = layout.size();
        let align = layout.align();
        // Lazily initialize base on first allocation (can't cast pointers in const)
        let base = {
            let b = self.base.load(core::sync::atomic::Ordering::Relaxed);
            if b == 0 {
                let heap_base = HEAP.as_ptr() as usize;
                self.base.store(heap_base, core::sync::atomic::Ordering::Relaxed);
                // Also init next to the base
                self.next.store(heap_base, core::sync::atomic::Ordering::Relaxed);
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
            if self.next.compare_exchange_weak(
                current,
                new,
                core::sync::atomic::Ordering::Relaxed,
                core::sync::atomic::Ordering::Relaxed,
            ).is_ok() {
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

// Font data (8x16 bitmap)
const FONT_W: usize = 8;
const FONT_H: usize = 16;
include!("font_data.rs");

// Terminal dimensions
const TERM_COLS: usize = 80;
const TERM_ROWS: usize = 24;
const SCROLLBACK_MAX: usize = 1000;

// Window dimensions (in pixels)
const WIN_W: u16 = (TERM_COLS * FONT_W) as u16;  // 640
const WIN_H: u16 = (TERM_ROWS * FONT_H + FONT_H) as u16;  // 400 (extra row for status)

// Colors (BGRA)
const COL_BG: u32 = 0xFF1E1E2E;       // Dark background
const COL_FG: u32 = 0xFFD4D4D4;       // Light text
const COL_CURSOR: u32 = 0xFF34C759;    // Green cursor
const COL_STATUS_BG: u32 = 0xFF222233;
const COL_STATUS_FG: u32 = 0xFF888888;

// ANSI color palette (basic 8 colors)
const ANSI_COLORS: [u32; 8] = [
    0xFF1E1E2E, // 0: Black (dark)
    0xFFCC0400, // 1: Red
    0xFF19A500, // 2: Green
    0xFFCC8800, // 3: Yellow/Brown
    0xFF007AFF, // 4: Blue
    0xFFA52A2A, // 5: Magenta
    0xFF00AAAA, // 6: Cyan
    0xFFD4D4D4, // 7: White (light)
];

/// A single character cell in the terminal
#[derive(Clone, Copy)]
struct Cell {
    ch: u8,          // ASCII character (32-126, or 0 for empty)
    fg_color: u32,   // Foreground color
    bg_color: u32,   // Background color
}

impl Cell {
    const fn new() -> Self {
        Cell { ch: b' ', fg_color: COL_FG, bg_color: COL_BG }
    }
}

/// ANSI escape sequence parser state
#[derive(Clone, Copy, PartialEq)]
enum AnsiState {
    Normal,
    Esc,       // Saw ESC
    Csi,       // Saw ESC [
    CsiParam,  // Collecting CSI parameters
}

/// Terminal state
struct Terminal {
    /// The visible screen buffer (TERM_COLS x TERM_ROWS)
    screen: [[Cell; TERM_COLS]; TERM_ROWS],
    /// Scrollback buffer (lines that scrolled off the top)
    scrollback: Vec<[Cell; TERM_COLS]>,
    /// Current cursor position
    cursor_x: usize,
    cursor_y: usize,
    /// Saved cursor position (for DECSC/DECRC)
    saved_cursor_x: usize,
    saved_cursor_y: usize,
    /// Scroll offset (0 = at bottom, >0 = scrolled up into scrollback)
    scroll_offset: usize,
    /// Current foreground color
    fg_color: u32,
    /// Current background color
    bg_color: u32,
    /// Cursor blink state
    cursor_visible: bool,
    /// Cursor blink timer
    blink_counter: usize,
    /// ANSI escape parsing state
    ansi_state: AnsiState,
    /// CSI parameter buffer
    csi_params: [u8; 16],
    csi_param_len: usize,
    /// PTY ID for this terminal
    pty_id: usize,
    /// Window SHM buffer pointer
    shm_buf: *mut u8,
    /// Window SHM ID
    shm_id: usize,
    /// Window ID (from WindowServer)
    window_id: u16,
    /// WindowServer PID (discovered)
    ws_pid: usize,
    /// Whether the window buffer needs redrawing
    dirty: bool,
}

impl Terminal {
    fn new() -> Self {
        Terminal {
            screen: [[Cell::new(); TERM_COLS]; TERM_ROWS],
            scrollback: Vec::new(),
            cursor_x: 0,
            cursor_y: 0,
            saved_cursor_x: 0,
            saved_cursor_y: 0,
            scroll_offset: 0,
            fg_color: COL_FG,
            bg_color: COL_BG,
            cursor_visible: true,
            blink_counter: 0,
            ansi_state: AnsiState::Normal,
            csi_params: [0u8; 16],
            csi_param_len: 0,
            pty_id: 0,
            shm_buf: core::ptr::null_mut(),
            shm_id: 0,
            window_id: 0,
            ws_pid: 0,
            dirty: true,
        }
    }

    /// Scroll the screen up by one line, moving the top line to scrollback
    fn scroll_up(&mut self) {
        // Move top line to scrollback
        self.scrollback.push(self.screen[0]);
        if self.scrollback.len() > SCROLLBACK_MAX {
            self.scrollback.remove(0);
        }
        // Shift all lines up
        for row in 0..TERM_ROWS - 1 {
            self.screen[row] = self.screen[row + 1];
        }
        // Clear the bottom line
        self.screen[TERM_ROWS - 1] = [Cell::new(); TERM_COLS];
        // Reset scroll offset when new content arrives
        self.scroll_offset = 0;
    }

    /// Write a character at the cursor position and advance cursor
    fn put_char(&mut self, ch: u8) {
        if self.cursor_y >= TERM_ROWS {
            self.cursor_y = TERM_ROWS - 1;
            self.scroll_up();
        }
        if self.cursor_x >= TERM_COLS {
            self.cursor_x = 0;
            self.cursor_y += 1;
            if self.cursor_y >= TERM_ROWS {
                self.cursor_y = TERM_ROWS - 1;
                self.scroll_up();
            }
        }
        self.screen[self.cursor_y][self.cursor_x] = Cell {
            ch,
            fg_color: self.fg_color,
            bg_color: self.bg_color,
        };
        self.cursor_x += 1;
        if self.cursor_x >= TERM_COLS {
            // Will wrap on next character
        }
        self.dirty = true;
    }

    /// Handle a newline / carriage return / linefeed
    fn newline(&mut self) {
        self.cursor_x = 0;
        self.cursor_y += 1;
        if self.cursor_y >= TERM_ROWS {
            self.cursor_y = TERM_ROWS - 1;
            self.scroll_up();
        }
        self.dirty = true;
    }

    /// Handle backspace
    fn backspace(&mut self) {
        if self.cursor_x > 0 {
            self.cursor_x -= 1;
            self.screen[self.cursor_y][self.cursor_x] = Cell {
                ch: b' ',
                fg_color: self.fg_color,
                bg_color: self.bg_color,
            };
            self.dirty = true;
        }
    }

    /// Clear the screen
    fn clear_screen(&mut self) {
        self.screen = [[Cell::new(); TERM_COLS]; TERM_ROWS];
        self.cursor_x = 0;
        self.cursor_y = 0;
        self.dirty = true;
    }

    /// Clear from cursor to end of line
    fn clear_to_eol(&mut self) {
        if self.cursor_y < TERM_ROWS {
            for col in self.cursor_x..TERM_COLS {
                self.screen[self.cursor_y][col] = Cell {
                    ch: b' ',
                    fg_color: self.fg_color,
                    bg_color: self.bg_color,
                };
            }
            self.dirty = true;
        }
    }

    /// Clear from cursor to start of line
    fn clear_to_sol(&mut self) {
        if self.cursor_y < TERM_ROWS {
            for col in 0..self.cursor_x {
                self.screen[self.cursor_y][col] = Cell {
                    ch: b' ',
                    fg_color: self.fg_color,
                    bg_color: self.bg_color,
                };
            }
            self.dirty = true;
        }
    }

    /// Clear from cursor to end of screen
    fn clear_to_eos(&mut self) {
        self.clear_to_eol();
        for row in self.cursor_y + 1..TERM_ROWS {
            self.screen[row] = [Cell::new(); TERM_COLS];
        }
        self.dirty = true;
    }

    /// Clear from start of screen to cursor
    fn clear_to_sos(&mut self) {
        self.clear_to_sol();
        for row in 0..self.cursor_y {
            self.screen[row] = [Cell::new(); TERM_COLS];
        }
        self.dirty = true;
    }

    /// Parse a CSI parameter as an integer, returns default if empty/invalid
    fn parse_csi_param(&self, default: usize) -> usize {
        if self.csi_param_len == 0 {
            return default;
        }
        let mut val: usize = 0;
        for i in 0..self.csi_param_len {
            let d = self.csi_params[i];
            if d >= b'0' && d <= b'9' {
                val = val * 10 + (d - b'0') as usize;
            } else if d == b';' {
                break;
            }
        }
        if val == 0 { default } else { val }
    }

    /// Parse the Nth CSI parameter (semicolon-separated)
    fn parse_csi_param_n(&self, n: usize, default: usize) -> usize {
        let mut param_idx = 0;
        let mut val: usize = 0;
        let mut found_digit = false;
        for i in 0..self.csi_param_len {
            let d = self.csi_params[i];
            if d >= b'0' && d <= b'9' {
                val = val * 10 + (d - b'0') as usize;
                found_digit = true;
            } else if d == b';' {
                if param_idx == n {
                    return if found_digit && val > 0 { val } else { default };
                }
                param_idx += 1;
                val = 0;
                found_digit = false;
            }
        }
        if param_idx == n {
            return if found_digit && val > 0 { val } else { default };
        }
        default
    }

    /// Handle a complete CSI sequence (the final byte tells which command)
    fn handle_csi(&mut self, final_byte: u8) {
        match final_byte {
            // CSI n A — Cursor Up
            b'A' => {
                let n = self.parse_csi_param(1);
                if self.cursor_y >= n {
                    self.cursor_y -= n;
                } else {
                    self.cursor_y = 0;
                }
                self.dirty = true;
            }
            // CSI n B — Cursor Down
            b'B' => {
                let n = self.parse_csi_param(1);
                if self.cursor_y + n < TERM_ROWS {
                    self.cursor_y += n;
                } else {
                    self.cursor_y = TERM_ROWS - 1;
                }
                self.dirty = true;
            }
            // CSI n C — Cursor Forward (right)
            b'C' => {
                let n = self.parse_csi_param(1);
                if self.cursor_x + n < TERM_COLS {
                    self.cursor_x += n;
                } else {
                    self.cursor_x = TERM_COLS - 1;
                }
                self.dirty = true;
            }
            // CSI n D — Cursor Back (left)
            b'D' => {
                let n = self.parse_csi_param(1);
                if self.cursor_x >= n {
                    self.cursor_x -= n;
                } else {
                    self.cursor_x = 0;
                }
                self.dirty = true;
            }
            // CSI n ; m H — Cursor Position (row; col) 1-based
            b'H' | b'f' => {
                let row = self.parse_csi_param_n(0, 1);
                let col = self.parse_csi_param_n(1, 1);
                self.cursor_y = (row - 1).min(TERM_ROWS - 1);
                self.cursor_x = (col - 1).min(TERM_COLS - 1);
                self.dirty = true;
            }
            // CSI n J — Erase in Display
            b'J' => {
                let n = self.parse_csi_param(0);
                match n {
                    0 => self.clear_to_eos(),
                    1 => self.clear_to_sos(),
                    2 => self.clear_screen(),
                    _ => {}
                }
            }
            // CSI n K — Erase in Line
            b'K' => {
                let n = self.parse_csi_param(0);
                match n {
                    0 => self.clear_to_eol(),
                    1 => self.clear_to_sol(),
                    2 => {
                        if self.cursor_y < TERM_ROWS {
                            self.screen[self.cursor_y] = [Cell::new(); TERM_COLS];
                            self.dirty = true;
                        }
                    }
                    _ => {}
                }
            }
            // CSI n m — SGR (Select Graphic Rendition) — colors/styles
            b'm' => {
                let code = self.parse_csi_param(0);
                match code {
                    0 => {
                        // Reset
                        self.fg_color = COL_FG;
                        self.bg_color = COL_BG;
                    }
                    30..=37 => {
                        // Set foreground color (30 + color)
                        let idx = (code - 30) as usize;
                        if idx < ANSI_COLORS.len() {
                            self.fg_color = ANSI_COLORS[idx];
                        }
                    }
                    40..=47 => {
                        // Set background color (40 + color)
                        let idx = (code - 40) as usize;
                        if idx < ANSI_COLORS.len() {
                            self.bg_color = ANSI_COLORS[idx];
                        }
                    }
                    1 => {
                        // Bold — just use brighter fg (no real bold in bitmap font)
                    }
                    _ => {}
                }
            }
            // CSI s — Save cursor position
            b's' => {
                self.saved_cursor_x = self.cursor_x;
                self.saved_cursor_y = self.cursor_y;
            }
            // CSI u — Restore cursor position
            b'u' => {
                self.cursor_x = self.saved_cursor_x;
                self.cursor_y = self.saved_cursor_y;
                self.dirty = true;
            }
            // CSI n S — Scroll Up
            b'S' => {
                let n = self.parse_csi_param(1);
                for _ in 0..n {
                    self.scroll_up();
                }
            }
            _ => {
                // Unknown CSI sequence - ignore
            }
        }
    }

    /// Process incoming bytes from the PTY (shell output)
    fn process_output(&mut self, data: &[u8]) {
        for &byte in data {
            match self.ansi_state {
                AnsiState::Normal => {
                    match byte {
                        0x1b => {
                            // ESC - start escape sequence
                            self.ansi_state = AnsiState::Esc;
                        }
                        0x0a => {
                            // Line Feed
                            self.newline();
                        }
                        0x0d => {
                            // Carriage Return
                            self.cursor_x = 0;
                            self.dirty = true;
                        }
                        0x08 => {
                            // Backspace
                            self.backspace();
                        }
                        0x09 => {
                            // Tab - move to next 8-column boundary
                            self.cursor_x = (self.cursor_x + 8) & !7;
                            if self.cursor_x >= TERM_COLS {
                                self.cursor_x = TERM_COLS - 1;
                            }
                            self.dirty = true;
                        }
                        0x07 => {
                            // Bell - ignore
                        }
                        0x20..=0x7e => {
                            // Printable character
                            self.put_char(byte);
                        }
                        _ => {
                            // Other control chars - ignore
                        }
                    }
                }
                AnsiState::Esc => {
                    match byte {
                        b'[' => {
                            // CSI sequence start
                            self.ansi_state = AnsiState::Csi;
                            self.csi_param_len = 0;
                        }
                        b'c' => {
                            // RIS - Reset
                            self.clear_screen();
                            self.ansi_state = AnsiState::Normal;
                        }
                        b'7' => {
                            // DECSC - Save cursor
                            self.saved_cursor_x = self.cursor_x;
                            self.saved_cursor_y = self.cursor_y;
                            self.ansi_state = AnsiState::Normal;
                        }
                        b'8' => {
                            // DECRC - Restore cursor
                            self.cursor_x = self.saved_cursor_x;
                            self.cursor_y = self.saved_cursor_y;
                            self.dirty = true;
                            self.ansi_state = AnsiState::Normal;
                        }
                        b'D' => {
                            // IND - Index (move down, scroll if needed)
                            self.cursor_y += 1;
                            if self.cursor_y >= TERM_ROWS {
                                self.cursor_y = TERM_ROWS - 1;
                                self.scroll_up();
                            }
                            self.dirty = true;
                            self.ansi_state = AnsiState::Normal;
                        }
                        b'M' => {
                            // RI - Reverse Index (move up, scroll if needed)
                            if self.cursor_y == 0 {
                                // Insert blank line at top
                                self.scrollback.push(self.screen[TERM_ROWS - 1]);
                                for row in (1..TERM_ROWS).rev() {
                                    self.screen[row] = self.screen[row - 1];
                                }
                                self.screen[0] = [Cell::new(); TERM_COLS];
                            } else {
                                self.cursor_y -= 1;
                            }
                            self.dirty = true;
                            self.ansi_state = AnsiState::Normal;
                        }
                        _ => {
                            // Unknown escape - return to normal
                            self.ansi_state = AnsiState::Normal;
                        }
                    }
                }
                AnsiState::Csi => {
                    match byte {
                        b'0'..=b'9' | b';' | b'?' => {
                            // Parameter byte
                            if self.csi_param_len < 16 {
                                self.csi_params[self.csi_param_len] = byte;
                                self.csi_param_len += 1;
                            }
                            self.ansi_state = AnsiState::CsiParam;
                        }
                        0x40..=0x7e => {
                            // Final byte - dispatch CSI command
                            self.handle_csi(byte);
                            self.ansi_state = AnsiState::Normal;
                        }
                        _ => {
                            // Intermediate bytes or unknown - ignore
                            self.ansi_state = AnsiState::CsiParam;
                        }
                    }
                }
                AnsiState::CsiParam => {
                    match byte {
                        b'0'..=b'9' | b';' | b'?' => {
                            // More parameter bytes
                            if self.csi_param_len < 16 {
                                self.csi_params[self.csi_param_len] = byte;
                                self.csi_param_len += 1;
                            }
                        }
                        0x40..=0x7e => {
                            // Final byte - dispatch CSI command
                            self.handle_csi(byte);
                            self.ansi_state = AnsiState::Normal;
                        }
                        _ => {
                            // Intermediate bytes - ignore but stay in CSI param
                        }
                    }
                }
            }
        }
    }

    /// Scroll up in the scrollback buffer
    fn scroll_up_scrollback(&mut self) {
        if self.scroll_offset < self.scrollback.len() {
            self.scroll_offset += 1;
            self.dirty = true;
        }
    }

    /// Scroll down in the scrollback buffer
    fn scroll_down_scrollback(&mut self) {
        if self.scroll_offset > 0 {
            self.scroll_offset -= 1;
            self.dirty = true;
        }
    }

    /// Render the terminal screen to the SHM buffer
    fn render(&mut self) {
        if self.shm_buf.is_null() { return; }

        let buf_w = WIN_W as usize;
        let buf_h = WIN_H as usize;
        let buf = self.shm_buf;

        // Clear the buffer to background color
        for y in 0..buf_h {
            for x in 0..buf_w {
                let off = (y * buf_w + x) * 4;
                unsafe {
                    *buf.add(off) = COL_BG as u8;
                    *buf.add(off + 1) = (COL_BG >> 8) as u8;
                    *buf.add(off + 2) = (COL_BG >> 16) as u8;
                    *buf.add(off + 3) = (COL_BG >> 24) as u8;
                }
            }
        }

        // Render scrollback lines if scrolled up
        let scrollback_visible = self.scroll_offset.min(self.scrollback.len());
        let scrollback_start = self.scrollback.len().saturating_sub(self.scroll_offset);
        
        // Render scrollback lines at the top (if scrolled up)
        for i in 0..scrollback_visible {
            let row = i;
            let line_idx = scrollback_start + i;
            if line_idx < self.scrollback.len() {
                let line = &self.scrollback[line_idx];
                for col in 0..TERM_COLS {
                    let cell = line[col];
                    let px = col * FONT_W;
                    let py = row * FONT_H;
                    self.draw_cell(px, py, cell);
                }
            }
        }

        // Render visible screen lines
        let screen_start_row = scrollback_visible;
        for row in 0..TERM_ROWS {
            let display_row = screen_start_row + row;
            if display_row >= TERM_ROWS {
                break;  // No room to display
            }
            for col in 0..TERM_COLS {
                let cell = self.screen[row][col];
                let px = col * FONT_W;
                let py = display_row * FONT_H;
                self.draw_cell(px, py, cell);
            }
        }

        // Draw cursor (blinking)
        if self.cursor_visible && self.scroll_offset == 0 {
            if self.cursor_y < TERM_ROWS && self.cursor_x < TERM_COLS {
                let cx = self.cursor_x * FONT_W;
                let cy = (self.cursor_y) * FONT_H;
                // Draw a filled block for the cursor
                for row in 0..FONT_H {
                    for col in 0..FONT_W {
                        let px = cx + col;
                        let py = cy + row;
                        if px < buf_w && py < buf_h {
                            let off = (py * buf_w + px) * 4;
                            unsafe {
                                *buf.add(off) = COL_CURSOR as u8;
                                *buf.add(off + 1) = (COL_CURSOR >> 8) as u8;
                                *buf.add(off + 2) = (COL_CURSOR >> 16) as u8;
                                *buf.add(off + 3) = (COL_CURSOR >> 24) as u8;
                            }
                        }
                    }
                }
            }
        }

        // Draw status bar at the bottom
        let status_y = (TERM_ROWS) * FONT_H;
        if status_y < buf_h {
            // Status bar background
            for y in status_y..buf_h.min(status_y + FONT_H) {
                for x in 0..buf_w {
                    let off = (y * buf_w + x) * 4;
                    unsafe {
                        *buf.add(off) = COL_STATUS_BG as u8;
                        *buf.add(off + 1) = (COL_STATUS_BG >> 8) as u8;
                        *buf.add(off + 2) = (COL_STATUS_BG >> 16) as u8;
                        *buf.add(off + 3) = (COL_STATUS_BG >> 24) as u8;
                    }
                }
            }
            // Status text
            let status_str = "VibeOS Terminal  80x24  UTF-8  LF";
            self.draw_str_at(4, status_y + 2, status_str, COL_STATUS_FG);
            
            // If scrolled, show scrollback indicator
            if self.scroll_offset > 0 {
                let scroll_str = " [SCROLL]";
                self.draw_str_at(4 + 26 * FONT_W, status_y + 2, scroll_str, 0xFF34C759);
            }
        }

        self.dirty = false;
    }

    /// Draw a single character cell at pixel position (px, py)
    fn draw_cell(&self, px: usize, py: usize, cell: Cell) {
        let buf_w = WIN_W as usize;
        let buf_h = WIN_H as usize;
        
        // Draw background
        for row in 0..FONT_H {
            for col in 0..FONT_W {
                let x = px + col;
                let y = py + row;
                if x < buf_w && y < buf_h {
                    let off = (y * buf_w + x) * 4;
                    unsafe {
                        *self.shm_buf.add(off) = cell.bg_color as u8;
                        *self.shm_buf.add(off + 1) = (cell.bg_color >> 8) as u8;
                        *self.shm_buf.add(off + 2) = (cell.bg_color >> 16) as u8;
                        *self.shm_buf.add(off + 3) = (cell.bg_color >> 24) as u8;
                    }
                }
            }
        }

        // Draw character glyph
        let idx = if (cell.ch as usize) >= 32 && (cell.ch as usize) <= 126 {
            (cell.ch as usize) - 32
        } else {
            0
        };
        let glyph = &FONT_DATA[idx];
        for row in 0..FONT_H {
            let bits = glyph[row];
            for col in 0..FONT_W {
                if bits & (0x80 >> col) != 0 {
                    let x = px + col;
                    let y = py + row;
                    if x < buf_w && y < buf_h {
                        let off = (y * buf_w + x) * 4;
                        unsafe {
                            *self.shm_buf.add(off) = cell.fg_color as u8;
                            *self.shm_buf.add(off + 1) = (cell.fg_color >> 8) as u8;
                            *self.shm_buf.add(off + 2) = (cell.fg_color >> 16) as u8;
                            *self.shm_buf.add(off + 3) = (cell.fg_color >> 24) as u8;
                        }
                    }
                }
            }
        }
    }

    /// Draw a string at pixel position (px, py)
    fn draw_str_at(&self, mut px: usize, py: usize, s: &str, color: u32) {
        for ch in s.chars() {
            if px + FONT_W > WIN_W as usize { break; }
            let idx = if (ch as usize) >= 32 && (ch as usize) <= 126 {
                (ch as usize) - 32
            } else {
                0
            };
            let glyph = &FONT_DATA[idx];
            let buf_w = WIN_W as usize;
            let buf_h = WIN_H as usize;
            for row in 0..FONT_H {
                let bits = glyph[row];
                for col in 0..FONT_W {
                    let x = px + col;
                    let y = py + row;
                    if x < buf_w && y < buf_h && bits & (0x80 >> col) != 0 {
                        let off = (y * buf_w + x) * 4;
                        unsafe {
                            *self.shm_buf.add(off) = color as u8;
                            *self.shm_buf.add(off + 1) = (color >> 8) as u8;
                            *self.shm_buf.add(off + 2) = (color >> 16) as u8;
                            *self.shm_buf.add(off + 3) = (color >> 24) as u8;
                        }
                    }
                }
            }
            px += FONT_W;
        }
    }

    /// Handle a key press from input events
    fn handle_key(&mut self, ascii: u8) {
        if self.pty_id == 0 { return; }

        // Reset scroll offset on key input
        if self.scroll_offset > 0 {
            self.scroll_offset = 0;
            self.dirty = true;
        }

        // Write the key to the PTY master (will appear as input on slave side)
        let data = [ascii];
        ptys_write(self.pty_id, &data);
    }

    /// Handle a special key (arrow keys, etc.) — send ANSI escape sequences
    fn handle_special_key(&mut self, key_code: u8) {
        if self.pty_id == 0 { return; }

        // Arrow keys and special keys are sent as ANSI escape sequences
        let seq: &[u8] = match key_code {
            0x41 => b"\x1b[A",     // Up arrow
            0x42 => b"\x1b[B",     // Down arrow
            0x43 => b"\x1b[C",     // Right arrow
            0x44 => b"\x1b[D",     // Left arrow
            0x48 => b"\x1b[H",     // Home
            0x45 => b"\x1b[2~",    // Insert
            0x46 => b"\x1b[F",     // End
            0x50 => b"\x1b[[A",    // F1
            0x51 => b"\x1b[[B",    // F2
            0x52 => b"\x1b[[C",    // F3
            0x53 => b"\x1b[[D",    // F4
            _ => return,
        };
        ptys_write(self.pty_id, seq);
    }

    /// Poll PTY for new output from the shell and process it
    fn poll_pty(&mut self) {
        if self.pty_id == 0 { return; }
        let mut buf = [0u8; 256];
        let n = ptys_read(self.pty_id, &mut buf);
        if n > 0 {
            self.process_output(&buf[..n]);
        }
    }

    /// Update cursor blink
    fn update_blink(&mut self) {
        self.blink_counter += 1;
        if self.blink_counter >= 30 {
            self.blink_counter = 0;
            self.cursor_visible = !self.cursor_visible;
            self.dirty = true;
        }
    }
}

/// Find the WindowServer PID by trying reasonable PIDs
/// In VibeOS, the WindowServer is typically one of the first few user processes
fn find_windowserver_pid() -> usize {
    // The WindowServer PID is typically 1 or 2 in our system.
    // We try sending a test message to figure out which one is the WindowServer.
    // For simplicity, assume PID 1 is the WindowServer (first user-space app spawned).
    1
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    let mut term = Terminal::new();

    // Step 1: Find WindowServer PID
    term.ws_pid = find_windowserver_pid();

    // Step 2: Create shared memory for the window buffer
    let buf_size = (WIN_W as usize) * (WIN_H as usize) * 4;
    term.shm_id = shm_create(buf_size);
    if term.shm_id == 0 {
        // Failed to create SHM - still run but without display
        loop { yield_cpu(); }
    }
    term.shm_buf = shm_map(term.shm_id);

    // Step 3: Create window via IPC to WindowServer
    let create_msg = msg_create_window(100, 80, WIN_W, WIN_H);
    ipc_send(term.ws_pid, &create_msg);

    // Step 4: Wait for WindowReady response with window ID
    // Try for a while to receive the response
    let mut retries = 0;
    while term.window_id == 0 && retries < 100 {
        let mut resp = [0u8; IPC_PAYLOAD_SIZE];
        let result = ipc_recv(&mut resp);
        if result != 0 {
            let msg_type = resp[0];
            if msg_type == MSG_WINDOW_READY {
                term.window_id = parse_window_id(&resp);
            }
        }
        if term.window_id == 0 {
            yield_cpu();
            retries += 1;
        }
    }

    // If we didn't get a window ID, assign a default and continue
    if term.window_id == 0 {
        term.window_id = 1;
    }

    // Step 5: Open a PTY
    term.pty_id = ptys_open();
    if term.pty_id == 0 {
        // Failed to open PTY
        let err_msg = b"Error: Failed to open PTY\n";
        term.process_output(err_msg);
    }

    // Step 6: Spawn the shell connected to the PTY
    let shell_pid = spawn_pty_shell(term.pty_id);
    if shell_pid == 0 {
        let err_msg = b"Error: Failed to spawn shell\n";
        term.process_output(err_msg);
    }

    // Step 7: Send initial welcome message
    term.process_output(b"VibeOS Terminal v0.1\r\n");
    term.process_output(b"Connected to PTY #");
    // Convert pty_id to string for display
    let mut pty_str = [0u8; 12];
    let mut len = 0;
    {
        let mut id = term.pty_id;
        if id == 0 {
            pty_str[0] = b'0';
            len = 1;
        } else {
            let mut digits = [0u8; 12];
            let mut dlen = 0;
            while id > 0 {
                digits[dlen] = b'0' + (id % 10) as u8;
                id /= 10;
                dlen += 1;
            }
            for i in 0..dlen {
                pty_str[i] = digits[dlen - 1 - i];
                len = dlen;
            }
        }
    }
    term.process_output(&pty_str[..len]);
    term.process_output(b"\r\n\r\n");

    // Step 8: Main event loop
    loop {
        // Poll for keyboard input
        if let Some(event) = input_poll() {
            if event.is_key_press() {
                let ascii = event.ascii();
                // Check for Shift+Up/Down for scrollback
                // (We check via special key codes or we approximate)
                // For now, scroll with Page Up / Page Down
                match ascii {
                    // Page Up (sent as 0x0e or similar, approximate)
                    0x0e => {
                        term.scroll_up_scrollback();
                    }
                    // Page Down
                    0x0f => {
                        term.scroll_down_scrollback();
                    }
                    // Arrow up - could be special key
                    0x41 => {
                        term.handle_special_key(0x41);
                    }
                    // Arrow down
                    0x42 => {
                        term.handle_special_key(0x42);
                    }
                    // Arrow right
                    0x43 => {
                        term.handle_special_key(0x43);
                    }
                    // Arrow left
                    0x44 => {
                        term.handle_special_key(0x44);
                    }
                    _ => {
                        term.handle_key(ascii);
                    }
                }
            }
        }

        // Poll PTY for shell output
        term.poll_pty();

        // Update cursor blink
        term.update_blink();

        // Render if dirty
        if term.dirty {
            term.render();
        }

        // Yield CPU
        yield_cpu();
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}