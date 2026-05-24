//! Event types for the GUI framework.

#![allow(dead_code)]

/// Mouse button identifier.
#[derive(Clone, Copy, Debug)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

/// Mouse event with position and state.
#[derive(Clone, Copy, Debug)]
pub struct MouseEvent {
    pub x: u32,
    pub y: u32,
    pub button: MouseButton,
    pub click_count: u32,
    pub is_down: bool,
    pub is_dragging: bool,
    pub delta_x: i32,
    pub delta_y: i32,
}

/// Keyboard key code (raw scancode).
#[derive(Clone, Copy, Debug)]
pub struct KeyCode(pub u8);

/// Input event types dispatched through the view hierarchy.
#[derive(Clone, Copy, Debug)]
pub enum InputEvent {
    MouseDown(MouseEvent),
    MouseUp(MouseEvent),
    MouseMove(MouseEvent),
    MouseDrag(MouseEvent),
    KeyDown(KeyCode),
    KeyUp(KeyCode),
}

impl InputEvent {
    /// Create a mouse-down event.
    pub fn mouse_down(x: u32, y: u32, button: MouseButton) -> Self {
        InputEvent::MouseDown(MouseEvent {
            x, y, button, click_count: 1, is_down: true, is_dragging: false, delta_x: 0, delta_y: 0,
        })
    }

    /// Create a mouse-up event.
    pub fn mouse_up(x: u32, y: u32, button: MouseButton) -> Self {
        InputEvent::MouseUp(MouseEvent {
            x, y, button, click_count: 0, is_down: false, is_dragging: false, delta_x: 0, delta_y: 0,
        })
    }

    /// Create a mouse-move event.
    pub fn mouse_move(x: u32, y: u32, dx: i32, dy: i32) -> Self {
        InputEvent::MouseMove(MouseEvent {
            x, y, button: MouseButton::Left, click_count: 0, is_down: false, is_dragging: false, delta_x: dx, delta_y: dy,
        })
    }

    /// Create a key-down event.
    pub fn key_down(code: u8) -> Self {
        InputEvent::KeyDown(KeyCode(code))
    }

    /// Create a key-up event.
    pub fn key_up(code: u8) -> Self {
        InputEvent::KeyUp(KeyCode(code))
    }

    /// Try to deserialize from a byte slice (IPC message payload).
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, &'static str> {
        if bytes.len() < 2 {
            return Err("too short");
        }
        match bytes[0] {
            0x01 => {
                // MouseDown
                if bytes.len() < 10 { return Err("mouse event too short"); }
                let x = u32::from_le_bytes([bytes[2], bytes[3], bytes[4], bytes[5]]);
                let y = u32::from_le_bytes([bytes[6], bytes[7], bytes[8], bytes[9]]);
                Ok(InputEvent::mouse_down(x, y, MouseButton::Left))
            }
            0x02 => {
                if bytes.len() < 2 { return Err("key event too short"); }
                Ok(InputEvent::key_down(bytes[1]))
            }
            _ => Err("unknown event type"),
        }
    }

    /// Serialize to bytes for IPC.
    pub fn to_bytes(&self) -> [u8; 16] {
        let mut buf = [0u8; 16];
        match self {
            InputEvent::MouseDown(me) => {
                buf[0] = 0x01;
                buf[2..6].copy_from_slice(&me.x.to_le_bytes());
                buf[6..10].copy_from_slice(&me.y.to_le_bytes());
            }
            InputEvent::MouseUp(me) => {
                buf[0] = 0x01;
                buf[1] = 0x01; // up flag
                buf[2..6].copy_from_slice(&me.x.to_le_bytes());
                buf[6..10].copy_from_slice(&me.y.to_le_bytes());
            }
            InputEvent::KeyDown(kc) => {
                buf[0] = 0x02;
                buf[1] = kc.0;
            }
            InputEvent::KeyUp(kc) => {
                buf[0] = 0x02;
                buf[1] = kc.0;
                buf[2] = 0x01; // up flag
            }
            InputEvent::MouseMove(me) => {
                buf[0] = 0x03;
                buf[2..6].copy_from_slice(&me.x.to_le_bytes());
                buf[6..10].copy_from_slice(&me.y.to_le_bytes());
            }
            InputEvent::MouseDrag(me) => {
                buf[0] = 0x04;
                buf[2..6].copy_from_slice(&me.x.to_le_bytes());
                buf[6..10].copy_from_slice(&me.y.to_le_bytes());
            }
        }
        buf
    }

    /// Convert a vibe::InputEvent (kernel input) into a GUI InputEvent.
    pub fn from_vibe_input(input: &vibe::InputEvent) -> Self {
        if input.is_mouse_move() {
            InputEvent::MouseMove(MouseEvent {
                x: input.x as u32,
                y: input.y as u32,
                button: MouseButton::Left,
                click_count: 0,
                is_down: false,
                is_dragging: false,
                delta_x: 0,
                delta_y: 0,
            })
        } else if input.is_mouse_down() {
            InputEvent::MouseDown(MouseEvent {
                x: input.x as u32,
                y: input.y as u32,
                button: MouseButton::Left,
                click_count: 1,
                is_down: true,
                is_dragging: false,
                delta_x: 0,
                delta_y: 0,
            })
        } else if input.is_mouse_up() {
            InputEvent::MouseUp(MouseEvent {
                x: input.x as u32,
                y: input.y as u32,
                button: MouseButton::Left,
                click_count: 0,
                is_down: false,
                is_dragging: false,
                delta_x: 0,
                delta_y: 0,
            })
        } else if input.is_key_press() {
            InputEvent::KeyDown(KeyCode(input.ascii()))
        } else {
            // Unknown event type — treat as key up with the raw byte
            InputEvent::KeyUp(KeyCode(input.extra))
        }
    }

    /// Get the x,y position if this is a mouse event.
    pub fn mouse_pos(&self) -> Option<(u32, u32)> {
        match self {
            InputEvent::MouseDown(me) | InputEvent::MouseUp(me) | InputEvent::MouseMove(me) | InputEvent::MouseDrag(me) => {
                Some((me.x, me.y))
            }
            _ => None,
        }
    }
}