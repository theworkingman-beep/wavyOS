//! Standard UI widgets — Button, Label, TextField, Toggle, ImageView, ScrollView.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use crate::color::Color;
use crate::drawing::{Font, Framebuffer, Rect};
use crate::event::{InputEvent, MouseEvent};
use crate::{next_view_id, ViewProtocol};

// ── Button ──────────────────────────────────────────────────────────────────

/// State of a button widget.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ButtonState {
    Normal,
    Hovered,
    Pressed,
    Disabled,
}

/// A clickable button widget (like NSButton).
pub struct Button {
    pub id: u64,
    pub frame: Rect,
    pub title: String,
    pub state: ButtonState,
    pub background_color: Color,
    pub title_color: Color,
    pub corner_radius: u32,
    pub is_hidden: bool,
    pub on_click: Option<fn(&mut Button)>,
}

impl Button {
    pub fn new(title: &str, frame: Rect) -> Self {
        Button {
            id: next_view_id(),
            frame,
            title: String::from(title),
            state: ButtonState::Normal,
            background_color: Color::ACCENT,
            title_color: Color::WHITE,
            corner_radius: 6,
            is_hidden: false,
            on_click: None,
        }
    }

    /// Convenience: create a button centered at (cx, cy) with enough width for the title.
    pub fn with_title(title: &str, cx: u32, cy: u32) -> Self {
        let w = Font::str_width(title) + 24;
        let h = 28;
        Self::new(title, Rect::new(cx - w / 2, cy - h / 2, w, h))
    }
}

impl ViewProtocol for Button {
    fn id(&self) -> u64 { self.id }
    fn frame(&self) -> Rect { self.frame }
    fn set_frame(&mut self, frame: Rect) { self.frame = frame; }
    fn is_hidden(&self) -> bool { self.is_hidden }
    fn set_hidden(&mut self, hidden: bool) { self.is_hidden = hidden; }
    fn clips_to_bounds(&self) -> bool { true }
    fn alpha(&self) -> f32 { 1.0 }
    fn background_color(&self) -> Color { self.background_color }
    fn subview_ids(&self) -> Vec<u64> { Vec::new() }
    fn subview_count(&self) -> usize { 0 }
    fn subview_at(&self, _index: usize) -> Option<&dyn ViewProtocol> { None }
    fn subview_at_mut(&mut self, _index: usize) -> Option<&mut dyn ViewProtocol> { None }
    fn add_subview(&mut self, _subview: Box<dyn ViewProtocol>) {}
    fn remove_subview(&mut self, _id: u64) -> Option<Box<dyn ViewProtocol>> { None }
    fn draw(&mut self, fb: &mut Framebuffer, rect: Rect) {
        let bg = match self.state {
            ButtonState::Normal => self.background_color,
            ButtonState::Hovered => self.background_color.lighten(0.15),
            ButtonState::Pressed => self.background_color.darken(0.15),
            ButtonState::Disabled => Color::GRAY,
        };
        fb.fill_rounded_rect(rect, self.corner_radius, bg);
        // Center the title text
        let text_w = Font::str_width(&self.title);
        let text_x = rect.x + (rect.width.saturating_sub(text_w)) / 2;
        let text_y = rect.y + (rect.height.saturating_sub(Font::CHAR_H)) / 2;
        Font::draw_str(fb, text_x, text_y, &self.title, self.title_color, None);
    }
    fn layout_subviews(&mut self) {}
    fn handle_mouse_event(&mut self, event: &MouseEvent) -> bool {
        match self.state {
            ButtonState::Disabled => return false,
            _ => {}
        }
        let inside = self.frame.contains(event.x, event.y);
        match event.is_down {
            true => {
                if inside {
                    self.state = ButtonState::Pressed;
                    return true;
                }
            }
            false => {
                if self.state == ButtonState::Pressed {
                    self.state = ButtonState::Hovered;
                    if let Some(on_click) = self.on_click {
                        on_click(self);
                    }
                    return true;
                }
                if inside {
                    self.state = ButtonState::Hovered;
                } else {
                    self.state = ButtonState::Normal;
                }
            }
        }
        inside
    }
    fn handle_key_event(&mut self, _event: &InputEvent) -> bool { false }
    fn accepts_first_responder(&self) -> bool { false }
    fn tag(&self) -> u64 { 0 }
    fn identifier(&self) -> Option<&str> { None }
    fn type_name(&self) -> &'static str { "Button" }
}

// ── Label ───────────────────────────────────────────────────────────────────

/// A static text label (like NSTextField in label mode).
pub struct Label {
    pub id: u64,
    pub frame: Rect,
    pub text: String,
    pub text_color: Color,
    pub background_color: Color,
    pub is_hidden: bool,
    pub font_size: u32,
    pub alignment: TextAlignment,
}

/// Text alignment.
#[derive(Clone, Copy, Debug)]
pub enum TextAlignment {
    Left,
    Center,
    Right,
}

impl Label {
    pub fn new(text: &str, frame: Rect) -> Self {
        Label {
            id: next_view_id(),
            frame,
            text: String::from(text),
            text_color: Color::BLACK,
            background_color: Color::CLEAR,
            is_hidden: false,
            font_size: 8,
            alignment: TextAlignment::Left,
        }
    }
}

impl ViewProtocol for Label {
    fn id(&self) -> u64 { self.id }
    fn frame(&self) -> Rect { self.frame }
    fn set_frame(&mut self, frame: Rect) { self.frame = frame; }
    fn is_hidden(&self) -> bool { self.is_hidden }
    fn set_hidden(&mut self, hidden: bool) { self.is_hidden = hidden; }
    fn clips_to_bounds(&self) -> bool { false }
    fn alpha(&self) -> f32 { 1.0 }
    fn background_color(&self) -> Color { self.background_color }
    fn subview_ids(&self) -> Vec<u64> { Vec::new() }
    fn subview_count(&self) -> usize { 0 }
    fn subview_at(&self, _index: usize) -> Option<&dyn ViewProtocol> { None }
    fn subview_at_mut(&mut self, _index: usize) -> Option<&mut dyn ViewProtocol> { None }
    fn add_subview(&mut self, _subview: Box<dyn ViewProtocol>) {}
    fn remove_subview(&mut self, _id: u64) -> Option<Box<dyn ViewProtocol>> { None }
    fn draw(&mut self, fb: &mut Framebuffer, rect: Rect) {
        if self.background_color.a > 0 {
            fb.fill_rect_blend(rect, self.background_color);
        }
        let text_w = Font::str_width(&self.text);
        let x = match self.alignment {
            TextAlignment::Left => rect.x + 4,
            TextAlignment::Center => rect.x + (rect.width.saturating_sub(text_w)) / 2,
            TextAlignment::Right => rect.x + rect.width.saturating_sub(text_w + 4),
        };
        Font::draw_str(fb, x, rect.y + 4, &self.text, self.text_color, None);
    }
    fn layout_subviews(&mut self) {}
    fn handle_mouse_event(&mut self, _event: &MouseEvent) -> bool { false }
    fn handle_key_event(&mut self, _event: &InputEvent) -> bool { false }
    fn tag(&self) -> u64 { 0 }
    fn identifier(&self) -> Option<&str> { None }
    fn type_name(&self) -> &'static str { "Label" }
}

// ── TextField ───────────────────────────────────────────────────────────────

/// An editable text field (like NSTextField).
pub struct TextField {
    pub id: u64,
    pub frame: Rect,
    pub text: String,
    pub placeholder: String,
    pub text_color: Color,
    pub background_color: Color,
    pub border_color: Color,
    pub is_hidden: bool,
    pub is_editing: bool,
    pub cursor_pos: usize,
    pub is_focused: bool,
}

impl TextField {
    pub fn new(placeholder: &str, frame: Rect) -> Self {
        TextField {
            id: next_view_id(),
            frame,
            text: String::new(),
            placeholder: String::from(placeholder),
            text_color: Color::BLACK,
            background_color: Color::WHITE,
            border_color: Color::GRAY,
            is_hidden: false,
            is_editing: false,
            cursor_pos: 0,
            is_focused: false,
        }
    }
}

impl ViewProtocol for TextField {
    fn id(&self) -> u64 { self.id }
    fn frame(&self) -> Rect { self.frame }
    fn set_frame(&mut self, frame: Rect) { self.frame = frame; }
    fn is_hidden(&self) -> bool { self.is_hidden }
    fn set_hidden(&mut self, hidden: bool) { self.is_hidden = hidden; }
    fn clips_to_bounds(&self) -> bool { true }
    fn alpha(&self) -> f32 { 1.0 }
    fn background_color(&self) -> Color { self.background_color }
    fn subview_ids(&self) -> Vec<u64> { Vec::new() }
    fn subview_count(&self) -> usize { 0 }
    fn subview_at(&self, _index: usize) -> Option<&dyn ViewProtocol> { None }
    fn subview_at_mut(&mut self, _index: usize) -> Option<&mut dyn ViewProtocol> { None }
    fn add_subview(&mut self, _subview: Box<dyn ViewProtocol>) {}
    fn remove_subview(&mut self, _id: u64) -> Option<Box<dyn ViewProtocol>> { None }
    fn draw(&mut self, fb: &mut Framebuffer, rect: Rect) {
        // Background
        fb.fill_rounded_rect(rect, 4, self.background_color);
        // Border
        if self.is_focused {
            fb.draw_border(rect, Color::ACCENT, 2);
        } else {
            fb.draw_border(rect, self.border_color, 1);
        }
        // Text or placeholder
        let display_text = if self.text.is_empty() {
            &self.placeholder
        } else {
            &self.text
        };
        let color = if self.text.is_empty() {
            Color::GRAY
        } else {
            self.text_color
        };
        Font::draw_str(fb, rect.x + 6, rect.y + 4, display_text, color, None);
        // Cursor
        if self.is_focused {
            let cursor_x = rect.x + 6 + Font::str_width(&self.text[..self.cursor_pos.min(self.text.len())]);
            fb.fill_rect(Rect::new(cursor_x, rect.y + 4, 1, Font::CHAR_H), Color::BLACK);
        }
    }
    fn layout_subviews(&mut self) {}
    fn handle_mouse_event(&mut self, event: &MouseEvent) -> bool {
        if event.is_down && self.frame.contains(event.x, event.y) {
            self.is_focused = true;
            // Place cursor near click position
            let rel_x = event.x.saturating_sub(self.frame.x + 6);
            let char_pos = (rel_x / Font::CHAR_W) as usize;
            self.cursor_pos = char_pos.min(self.text.len());
            return true;
        }
        false
    }
    fn handle_key_event(&mut self, event: &InputEvent) -> bool {
        if !self.is_focused { return false; }
        match event {
            InputEvent::KeyDown(kc) => {
                // Map keycode to character (basic ASCII mapping)
                let code = kc.0;
                if code == 8 {
                    // Backspace
                    if self.cursor_pos > 0 {
                        self.cursor_pos -= 1;
                        self.text.remove(self.cursor_pos);
                    }
                    return true;
                } else if code == 13 {
                    // Enter — finish editing
                    self.is_focused = false;
                    return true;
                } else if code == 27 {
                    // Escape — cancel
                    self.is_focused = false;
                    return true;
                } else if code >= 32 && code < 127 {
                    let ch = code as char;
                    self.text.insert(self.cursor_pos, ch);
                    self.cursor_pos += 1;
                    return true;
                }
            }
            _ => {}
        }
        false
    }
    fn accepts_first_responder(&self) -> bool { true }
    fn tag(&self) -> u64 { 0 }
    fn identifier(&self) -> Option<&str> { None }
    fn type_name(&self) -> &'static str { "TextField" }
}

// ── Toggle ──────────────────────────────────────────────────────────────────

/// A toggle/checkbox widget.
pub struct Toggle {
    pub id: u64,
    pub frame: Rect,
    pub title: String,
    pub is_on: bool,
    pub is_hidden: bool,
    pub on_color: Color,
    pub off_color: Color,
    pub on_change: Option<fn(bool)>,
}

impl Toggle {
    pub fn new(title: &str, frame: Rect) -> Self {
        Toggle {
            id: next_view_id(),
            frame,
            title: String::from(title),
            is_on: false,
            is_hidden: false,
            on_color: Color::GREEN,
            off_color: Color::LIGHT_GRAY,
            on_change: None,
        }
    }
}

impl ViewProtocol for Toggle {
    fn id(&self) -> u64 { self.id }
    fn frame(&self) -> Rect { self.frame }
    fn set_frame(&mut self, frame: Rect) { self.frame = frame; }
    fn is_hidden(&self) -> bool { self.is_hidden }
    fn set_hidden(&mut self, hidden: bool) { self.is_hidden = hidden; }
    fn clips_to_bounds(&self) -> bool { false }
    fn alpha(&self) -> f32 { 1.0 }
    fn background_color(&self) -> Color { Color::CLEAR }
    fn subview_ids(&self) -> Vec<u64> { Vec::new() }
    fn subview_count(&self) -> usize { 0 }
    fn subview_at(&self, _index: usize) -> Option<&dyn ViewProtocol> { None }
    fn subview_at_mut(&mut self, _index: usize) -> Option<&mut dyn ViewProtocol> { None }
    fn add_subview(&mut self, _subview: Box<dyn ViewProtocol>) {}
    fn remove_subview(&mut self, _id: u64) -> Option<Box<dyn ViewProtocol>> { None }
    fn draw(&mut self, fb: &mut Framebuffer, rect: Rect) {
        let box_size = 16u32;
        let box_rect = Rect::new(rect.x, rect.y + (rect.height - box_size) / 2, box_size, box_size);
        let color = if self.is_on { self.on_color } else { self.off_color };
        fb.fill_rounded_rect(box_rect, 4, color);
        if self.is_on {
            // Draw checkmark
            Font::draw_str(fb, box_rect.x + 2, box_rect.y + 1, "✓", Color::WHITE, None);
        }
        // Title text
        Font::draw_str(fb, rect.x + box_size + 6, rect.y + 4, &self.title, Color::BLACK, None);
    }
    fn layout_subviews(&mut self) {}
    fn handle_mouse_event(&mut self, event: &MouseEvent) -> bool {
        if event.is_down && self.frame.contains(event.x, event.y) {
            self.is_on = !self.is_on;
            if let Some(on_change) = self.on_change {
                on_change(self.is_on);
            }
            return true;
        }
        false
    }
    fn handle_key_event(&mut self, _event: &InputEvent) -> bool { false }
    fn tag(&self) -> u64 { 0 }
    fn identifier(&self) -> Option<&str> { None }
    fn type_name(&self) -> &'static str { "Toggle" }
}

// ── ImageView ────────────────────────────────────────────────────────────────

/// A widget that displays a static image (pixel buffer).
pub struct ImageView {
    pub id: u64,
    pub frame: Rect,
    pub pixels: Option<&'static [u32]>,
    pub image_width: u32,
    pub image_height: u32,
    pub is_hidden: bool,
    pub background_color: Color,
}

impl ImageView {
    pub fn new(frame: Rect) -> Self {
        ImageView {
            id: next_view_id(),
            frame,
            pixels: None,
            image_width: 0,
            image_height: 0,
            is_hidden: false,
            background_color: Color::CLEAR,
        }
    }

    pub fn with_image(frame: Rect, pixels: &'static [u32], w: u32, h: u32) -> Self {
        ImageView {
            id: next_view_id(),
            frame,
            pixels: Some(pixels),
            image_width: w,
            image_height: h,
            is_hidden: false,
            background_color: Color::CLEAR,
        }
    }
}

impl ViewProtocol for ImageView {
    fn id(&self) -> u64 { self.id }
    fn frame(&self) -> Rect { self.frame }
    fn set_frame(&mut self, frame: Rect) { self.frame = frame; }
    fn is_hidden(&self) -> bool { self.is_hidden }
    fn set_hidden(&mut self, hidden: bool) { self.is_hidden = hidden; }
    fn clips_to_bounds(&self) -> bool { true }
    fn alpha(&self) -> f32 { 1.0 }
    fn background_color(&self) -> Color { self.background_color }
    fn subview_ids(&self) -> Vec<u64> { Vec::new() }
    fn subview_count(&self) -> usize { 0 }
    fn subview_at(&self, _index: usize) -> Option<&dyn ViewProtocol> { None }
    fn subview_at_mut(&mut self, _index: usize) -> Option<&mut dyn ViewProtocol> { None }
    fn add_subview(&mut self, _subview: Box<dyn ViewProtocol>) {}
    fn remove_subview(&mut self, _id: u64) -> Option<Box<dyn ViewProtocol>> { None }
    fn draw(&mut self, fb: &mut Framebuffer, rect: Rect) {
        if self.background_color.a > 0 {
            fb.fill_rect_blend(rect, self.background_color);
        }
        if let Some(pixels) = self.pixels {
            fb.blit(rect.x, rect.y, pixels, self.image_width, self.image_height);
        }
    }
    fn layout_subviews(&mut self) {}
    fn handle_mouse_event(&mut self, _event: &MouseEvent) -> bool { false }
    fn handle_key_event(&mut self, _event: &InputEvent) -> bool { false }
    fn tag(&self) -> u64 { 0 }
    fn identifier(&self) -> Option<&str> { None }
    fn type_name(&self) -> &'static str { "ImageView" }
}

// ── ScrollView ──────────────────────────────────────────────────────────────

/// Scroll behavior for content overflow.
#[derive(Clone, Copy, Debug)]
pub enum ScrollBehavior {
    None,
    Vertical,
    Horizontal,
    Both,
}

/// A scrollable container with scrollbars.
pub struct ScrollView {
    pub id: u64,
    pub frame: Rect,
    pub content_size: Rect,
    pub scroll_offset_x: u32,
    pub scroll_offset_y: u32,
    pub scroll_behavior: ScrollBehavior,
    pub show_scrollbars: bool,
    pub is_hidden: bool,
    pub background_color: Color,
    pub content: Option<Box<dyn ViewProtocol>>,
}

impl ScrollView {
    pub fn new(frame: Rect) -> Self {
        ScrollView {
            id: next_view_id(),
            frame,
            content_size: Rect::new(0, 0, frame.width, frame.height),
            scroll_offset_x: 0,
            scroll_offset_y: 0,
            scroll_behavior: ScrollBehavior::Vertical,
            show_scrollbars: true,
            is_hidden: false,
            background_color: Color::WHITE,
            content: None,
        }
    }
}

impl ViewProtocol for ScrollView {
    fn id(&self) -> u64 { self.id }
    fn frame(&self) -> Rect { self.frame }
    fn set_frame(&mut self, frame: Rect) { self.frame = frame; }
    fn is_hidden(&self) -> bool { self.is_hidden }
    fn set_hidden(&mut self, hidden: bool) { self.is_hidden = hidden; }
    fn clips_to_bounds(&self) -> bool { true }
    fn alpha(&self) -> f32 { 1.0 }
    fn background_color(&self) -> Color { self.background_color }
    fn subview_ids(&self) -> Vec<u64> {
        self.content.as_ref().map(|c| vec![c.id()]).unwrap_or_default()
    }
    fn subview_count(&self) -> usize { if self.content.is_some() { 1 } else { 0 } }
    fn subview_at(&self, index: usize) -> Option<&dyn ViewProtocol> {
        if index == 0 { self.content.as_ref().map(|c| c.as_ref() as &dyn ViewProtocol) } else { None }
    }
    fn subview_at_mut(&mut self, index: usize) -> Option<&mut dyn ViewProtocol> {
        if index == 0 { self.content.as_mut().map(|c| c.as_mut() as &mut dyn ViewProtocol) } else { None }
    }
    fn add_subview(&mut self, subview: Box<dyn ViewProtocol>) {
        self.content = Some(subview);
    }
    fn remove_subview(&mut self, _id: u64) -> Option<Box<dyn ViewProtocol>> {
        self.content.take()
    }
    fn draw(&mut self, fb: &mut Framebuffer, rect: Rect) {
        // Background
        fb.fill_rect_blend(rect, self.background_color);
        // TODO: clip to rect and draw content at offset
        if let Some(ref mut content) = self.content {
            let content_frame = content.frame();
            let draw_rect = Rect::new(
                rect.x,
                rect.y,
                content_frame.width.min(rect.width),
                content_frame.height.min(rect.height),
            );
            content.draw(fb, draw_rect);
        }
        // Scrollbar indicators
        if self.show_scrollbars {
            let max_scroll_y = self.content_size.height.saturating_sub(self.frame.height);
            if max_scroll_y > 0 {
                let ratio = self.frame.height as f32 / self.content_size.height as f32;
                let thumb_h = (self.frame.height as f32 * ratio).max(20.0) as u32;
                let max_thumb_y = self.frame.height.saturating_sub(thumb_h);
                let thumb_y = if max_scroll_y > 0 {
                    rect.y + (self.scroll_offset_y * max_thumb_y / max_scroll_y)
                } else { rect.y };
                let sb_x = rect.x + rect.width - 6;
                fb.fill_rect(Rect::new(sb_x, rect.y, 6, self.frame.height), Color::GRAY.with_alpha(80));
                fb.fill_rounded_rect(Rect::new(sb_x, thumb_y, 6, thumb_h), 3, Color::DARK_GRAY.with_alpha(180));
            }
        }
    }
    fn layout_subviews(&mut self) {
        if let Some(ref mut content) = self.content {
            content.layout_subviews();
        }
    }
    fn handle_mouse_event(&mut self, event: &MouseEvent) -> bool {
        // Scroll handling: convert mouse wheel to scroll offset
        if self.frame.contains(event.x, event.y) {
            let local_x = event.x.saturating_sub(self.frame.x);
            let local_y = event.y.saturating_sub(self.frame.y);
            // Route to content if inside
            if let Some(ref mut content) = self.content {
                let content_event = MouseEvent {
                    x: local_x + self.scroll_offset_x,
                    y: local_y + self.scroll_offset_y,
                    ..*event
                };
                content.handle_mouse_event(&content_event);
            }
            return true;
        }
        false
    }
    fn handle_key_event(&mut self, event: &InputEvent) -> bool {
        if let Some(ref mut content) = self.content {
            content.handle_key_event(event)
        } else {
            false
        }
    }
    fn tag(&self) -> u64 { 0 }
    fn identifier(&self) -> Option<&str> { None }
    fn type_name(&self) -> &'static str { "ScrollView" }
}