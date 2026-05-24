//! Window management — connects a view tree to the WindowServer via SHM.

use alloc::boxed::Box;
use alloc::string::String;

use crate::color::Color;
use crate::drawing::{Font, Framebuffer, Rect};
use crate::event::InputEvent;
use crate::ViewProtocol;

/// Window style determines chrome appearance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WindowStyle {
    /// Standard window with title bar, traffic lights, and resize.
    Titled,
    /// Borderless utility panel.
    Borderless,
    /// Modal dialog with close button only.
    Modal,
    /// Panel/sidebar style.
    Panel,
}

/// Delegate callbacks for window lifecycle events.
pub trait WindowDelegate {
    /// Called when the window becomes visible.
    fn on_show(&mut self, _window: &mut Window) {}
    /// Called when the window is about to close.
    fn on_close(&mut self, _window: &mut Window) {}
    /// Called when the window needs redrawing.
    fn on_draw(&mut self, _window: &mut Window, _fb: &mut Framebuffer) {}
    /// Called when a mouse event occurs in the content area.
    fn on_mouse_event(&mut self, _window: &mut Window, _event: &InputEvent) {}
    /// Called when a key event occurs.
    fn on_key_event(&mut self, _window: &mut Window, _event: &InputEvent) {}
    /// Called when the window is resized.
    fn on_resize(&mut self, _window: &mut Window, _new_width: u32, _new_height: u32) {}
}

/// A top-level window that contains a view hierarchy and connects
/// to the WindowServer for display.
pub struct Window {
    /// Unique window ID (assigned by WindowServer via IPC).
    pub id: u32,
    /// Position and size.
    pub frame: Rect,
    /// Window style.
    pub style: WindowStyle,
    /// Window title.
    pub title: String,
    /// Whether the window is visible.
    pub visible: bool,
    /// Whether the window is minimized.
    pub minimized: bool,
    /// Whether the window is maximized.
    pub maximized: bool,
    /// Root content view.
    pub content_view: Option<Box<dyn ViewProtocol>>,
    /// Whether the window needs redrawing.
    pub needs_redraw: bool,
    /// Optional delegate for lifecycle callbacks.
    pub delegate: Option<Box<dyn WindowDelegate>>,
    /// Title bar height.
    pub title_bar_height: u32,
    /// Window background color.
    pub background_color: Color,
    /// Focused view ID (for keyboard routing).
    pub focused_view_id: Option<u64>,
    /// Whether we're currently dragging the title bar.
    pub is_dragging: bool,
    /// Drag offset.
    pub drag_offset: (u32, u32),
    /// Previous frame (for restore from maximize).
    pub prev_frame: Rect,
}

impl Window {
    pub fn new(title: &str, x: u32, y: u32, width: u32, height: u32) -> Self {
        Window {
            id: 0,
            frame: Rect::new(x, y, width, height),
            style: WindowStyle::Titled,
            title: String::from(title),
            visible: true,
            minimized: false,
            maximized: false,
            content_view: None,
            needs_redraw: true,
            delegate: None,
            title_bar_height: 28,
            background_color: Color::WINDOW_BG,
            focused_view_id: None,
            is_dragging: false,
            drag_offset: (0, 0),
            prev_frame: Rect::new(x, y, width, height),
        }
    }

    /// Create a window with a content view.
    pub fn with_content(title: &str, x: u32, y: u32, width: u32, height: u32, content: Box<dyn ViewProtocol>) -> Self {
        let mut win = Self::new(title, x, y, width, height);
        win.content_view = Some(content);
        win
    }

    /// Content area rect (below title bar).
    pub fn content_rect(&self) -> Rect {
        let title_h = match self.style {
            WindowStyle::Titled | WindowStyle::Modal => self.title_bar_height,
            WindowStyle::Borderless | WindowStyle::Panel => 0,
        };
        Rect::new(0, title_h, self.frame.width, self.frame.height - title_h)
    }

    /// Set the content view.
    pub fn set_content_view(&mut self, view: Box<dyn ViewProtocol>) {
        self.content_view = Some(view);
        self.needs_redraw = true;
    }

    /// Redraw the window into the given framebuffer.
    pub fn redraw(&mut self) {
        // Mark as clean — actual drawing happens when composited
        self.needs_redraw = false;
    }

    /// Draw the window and its contents into a framebuffer.
    pub fn draw_into(&mut self, fb: &mut Framebuffer) {
        // Draw title bar for titled windows
        match self.style {
            WindowStyle::Titled | WindowStyle::Modal => {
                self.draw_title_bar(fb);
            }
            WindowStyle::Borderless | WindowStyle::Panel => {}
        }

        // Draw content view
        let content_rect = self.content_rect();
        if let Some(ref mut content) = self.content_view {
            content.layout_subviews();
            content.draw(fb, content_rect);
        }

        // Call delegate (skip for now to avoid double &mut self)
    }

    fn draw_title_bar(&self, fb: &mut Framebuffer) {
        let tb_rect = Rect::new(0, 0, self.frame.width, self.title_bar_height);

        // Title bar gradient
        fb.fill_v_gradient(tb_rect, Color::TITLE_BAR_BG, Color::TITLE_BAR_BG.darken(0.05));

        // Traffic light buttons (close=red, minimize=yellow, maximize=green)
        if self.style == WindowStyle::Titled {
            let y_center = self.title_bar_height / 2;
            let radius = 6;
            // Close (red)
            fb.fill_circle(14, y_center, radius, Color::RED);
            Font::draw_str(fb, 11, y_center - 8, "x", Color::WHITE, None);
            // Minimize (yellow)
            fb.fill_circle(34, y_center, radius, Color::YELLOW);
            Font::draw_str(fb, 31, y_center - 8, "-", Color::BLACK, None);
            // Maximize (green)
            fb.fill_circle(54, y_center, radius, Color::GREEN);
            Font::draw_str(fb, 51, y_center - 8, "+", Color::WHITE, None);
        } else if self.style == WindowStyle::Modal {
            let y_center = self.title_bar_height / 2;
            let radius = 6;
            fb.fill_circle(14, y_center, radius, Color::RED);
            Font::draw_str(fb, 11, y_center - 8, "x", Color::WHITE, None);
        }

        // Title text (centered)
        let text_w = Font::str_width(&self.title);
        let text_x = (self.frame.width.saturating_sub(text_w)) / 2;
        Font::draw_str(fb, text_x, 6, &self.title, Color::BLACK, None);

        // Bottom border line
        fb.draw_hline(0, self.title_bar_height - 1, self.frame.width, Color::GRAY);
    }

    /// Handle an input event.
    pub fn handle_event(&mut self, event: &InputEvent) -> bool {
        // First, check title bar clicks
        match event {
            InputEvent::MouseDown(me) => {
                if me.y < self.title_bar_height {
                    // Check traffic light buttons
                    let y_center = self.title_bar_height / 2;
                    // Close button
                    if me.x >= 14 - 6 && me.x <= 14 + 6 && me.y >= y_center - 6 && me.y <= y_center + 6 {
                        self.visible = false;
                        return true;
                    }
                    // Start title bar drag
                    self.is_dragging = true;
                    self.drag_offset = (me.x, me.y);
                    return true;
                }
            }
            InputEvent::MouseUp(_) => {
                self.is_dragging = false;
            }
            _ => {}
        }

        // Route to content view
        if let Some(ref mut content) = self.content_view {
            match event {
                InputEvent::MouseDown(me) | InputEvent::MouseUp(me) | InputEvent::MouseMove(me) | InputEvent::MouseDrag(me) => {
                    // Convert to content-local coordinates
                    let content_y = me.y.saturating_sub(self.title_bar_height);
                    let _local_event = match event {
                        InputEvent::MouseDown(_) => InputEvent::MouseDown(crate::event::MouseEvent {
                            x: me.x, y: content_y, ..*me
                        }),
                        InputEvent::MouseUp(_) => InputEvent::MouseUp(crate::event::MouseEvent {
                            x: me.x, y: content_y, ..*me
                        }),
                        InputEvent::MouseMove(_) => InputEvent::MouseMove(crate::event::MouseEvent {
                            x: me.x, y: content_y, ..*me
                        }),
                        InputEvent::MouseDrag(_) => InputEvent::MouseDrag(crate::event::MouseEvent {
                            x: me.x, y: content_y, ..*me
                        }),
                        _ => unreachable!(),
                    };
                    return content.handle_mouse_event(me);
                }
                InputEvent::KeyDown(_) | InputEvent::KeyUp(_) => {
                    return content.handle_key_event(event);
                }
            }
        }
        false
    }
}