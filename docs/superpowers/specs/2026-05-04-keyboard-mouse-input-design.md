# Keyboard + Mouse Input Design — v0.2

## Overview

Add mouse cursor, keyboard input, and GUI event handling to VibeOS. The desktop becomes interactive: move a mouse cursor, click dock icons, drag windows, and type in the shell.

## Architecture

```
x86_64: PS/2 Mouse IRQ 12 ────┐
x86_64: PS/2 Keyboard IRQ 1 ──┤──► input.rs ──► gui_task event loop ──► hit-test ──► render
aarch64: PL050 KMI mouse ─────┤
aarch64: PL011 UART keyboard ─┘
```

### Components

1. **PS/2 Mouse Driver** (`kernel/src/drivers/ps2mouse.rs`) — x86_64, IRQ 12
2. **PL050 KMI Mouse Driver** (`kernel/src/drivers/pl050_mouse.rs`) — aarch64, MMIO 0x09004000
3. **Input Subsystem** (`kernel/src/input.rs`) — Unified event queue for keyboard + mouse
4. **Cursor Renderer** (`kernel/src/drivers/cursor.rs`) — 16x16 arrow bitmap with save/restore
5. **GUI Event Loop** (refactored `gui_task` in `kernel/src/main.rs`) — Event-driven compositor
6. **Hit-Test System** (in `kernel/src/wm.rs`) — Determine which UI element is at (x, y)

## Component Details

### 1. Mouse Drivers

**x86_64: PS/2 Mouse Driver** (`kernel/src/drivers/ps2mouse.rs`)

- IRQ 12 on PIC2 (interrupt 0x2C)
- Protocol: 3-byte packets `[buttons, dx, dy]`
- Enable sequence via port 0x64:
  1. 0xA8 — Enable mouse port
  2. 0x20 — Read command byte
  3. 0x60 — Write command byte with mouse enable bit
  4. Unmask IRQ 12 in PIC2 (port 0xA1, clear bit 4)
  5. Send 0xF4 to port 0x60 — Enable mouse data reporting
- State machine in IRQ handler: collect 3 bytes per packet
- Decode: `buttons` byte has bits 0=left, 1=right, 2=middle; `dx`/`dy` are signed deltas with overflow bits

**aarch64: PL050 KMI Mouse Driver** (`kernel/src/drivers/pl050_mouse.rs`)

- QEMU virt machine exposes a PL050 KMI (Keyboard/Mouse Interface) for mouse at MMIO `0x09004000`
- Uses same PS/2 protocol but via MMIO registers instead of I/O ports
- Register layout: `DATA` at offset 0x00, `STATUS` at offset 0x04, `COMMAND` at offset 0x08, `CONTROL` at offset 0x010
- IRQ for mouse KMI — configured via GIC (or polled if GIC not yet set up)
- Same 3-byte packet state machine as PS/2 mouse
- Enable: set CONTROL register to enable Rx interrupts and KMI

**Shared Mouse State**

- Both drivers write to a common `MouseState { x: u16, y: u16, left: bool, right: bool, middle: bool }` protected by `spin::Mutex`
- `pub fn mouse_init()` — calls arch-appropriate driver init
- `pub fn read_mouse() -> MouseState` — arch-independent accessor

### 2. Input Subsystem (`kernel/src/input.rs`)

```rust
pub enum InputEvent {
    MouseMove { x: u16, y: u16, buttons: u8 },
    MouseDown { button: u8, x: u16, y: u16 },
    MouseUp { button: u8, x: u16, y: u16 },
    KeyPress { ascii: u8 },
}
```

- Ring buffer of 256 `InputEvent` entries
- Keyboard IRQ pushes `KeyPress` events (from existing `ps2kbd.rs` `handle_scancode` integration)
- Mouse IRQ pushes `MouseMove`, `MouseDown` (button press), `MouseUp` (button release)
- `pub fn init()` — sets up keyboard handler callback
- `pub fn push(event: InputEvent)` — called by IRQ handlers
- `pub fn poll() -> Option<InputEvent>` — non-blocking dequeue, called by gui_task each frame
- Thread-safe via `spin::Mutex`

### 3. Cursor Renderer

- 16x16 bitmap arrow cursor (hardcoded pixel data)
- `draw_cursor(x, y)` — saves pixels underneath, then draws cursor on top
- `undraw_cursor(x, y)` — restores saved pixels
- Cursor is drawn in the gui_task loop, not in IRQ handlers
- Position is clamped to framebuffer bounds
- Cursor color: white with black outline for visibility on any background

### 4. GUI Event Loop

Refactored `gui_task`:

```
1. Draw initial desktop
2. Draw cursor at center
3. Loop:
   a. Undraw cursor at old position (restore pixels)
   b. Process all pending input events from input::poll()
   c. Update cursor position, redraw cursor
   d. Handle UI events:
      - Dock hover: highlight icon under cursor
      - Dock click: bring shell window to front / focus
      - Title bar drag: move window with mouse
      - Traffic light click: close/minimize/maximize
      - KeyPress: forward to shell task
   e. scheduler::yield_cpu()
```

### 5. Hit-Test System

Simple function that checks UI element rectangles:

```rust
enum HitTarget {
    None,
    DockIcon(usize),       // Which dock icon index
    TrafficLight(TrafficLight), // Close/Minimize/Maximize
    TitleBar,              // For window dragging
    WindowBody,            // Inside the welcome window
}
```

Hit-test order (front to back):
1. Traffic light buttons (3 small circles at window top-left)
2. Title bar (rectangle below traffic lights)
3. Dock icons (row of squares at bottom)
4. Window body (the welcome window content area)

## Data Flow

```
x86_64: PS/2 IRQ handlers (irq1, irq12) ─┐
                                          ├─► input.rs event queue ──► gui_task event loop
aarch64: UART PL011 + PL050 KMI ─────────┘
```

## Data Flow

```
┌─────────────────────────────────┐
│  IRQ Handlers (arch-specific)   │
│  x86_64: PS/2 IRQ 1 + IRQ 12    │
│  aarch64: PL011 UART + PL050    │
└──────────────┬──────────────────┘
               │ push events
               ▼
┌─────────────────────────────────┐
│  input.rs event queue           │
│  (ring buffer, 256 entries)     │
│  InputEvent enum                │
└──────────────┬──────────────────┘
               │ poll()
               ▼
┌─────────────────────────────────┐
│  gui_task event loop            │
│  ┌───────────────────────────┐  │
│  │ undraw cursor (old pos)   │  │
│  │ poll() → dispatch events  │  │
│  │   MouseMove → update pos  │  │
│  │   MouseDown → hit_test    │  │
│  │   MouseUp → end drag      │  │
│  │   KeyPress → shell input  │  │
│  │ draw cursor (new pos)     │  │
│  └───────────────────────────┘  │
│  scheduler::yield_cpu()         │
└─────────────────────────────────┘
```

## Constraints

- Cooperative scheduler only — no preemption. Input events are only processed when gui_task runs
- Mouse supported on both arches: PS/2 on x86_64 (IRQ 12), PL050 KMI on aarch64 (MMIO 0x09004000)
- Keyboard on both arches: PS/2 on x86_64 (IRQ 1), PL011 UART on aarch64
- Single window for now (the welcome window); no window creation/destruction beyond traffic light buttons

## Testing

- QEMU x86_64: mouse should move cursor, click dock to activate shell, type in shell
- QEMU aarch64: mouse via PL050 KMI should work same as x86_64, keyboard via UART for shell input
- No crash on input overflow or malformed PS/2 packets
