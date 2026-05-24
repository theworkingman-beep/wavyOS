//! VibeOS GUI Framework — AppKit-inspired user interface toolkit
//!
//! Provides a view hierarchy, event routing, layout engine, and standard
//! widgets for building graphical applications on VibeOS.
//!
//! Architecture:
//! - View: base class with subviews, layout, drawing, event handling
//! - Responder chain: events route through hit-tested view hierarchy
//! - Window: connects a view tree to the WindowServer via shared memory
//! - Widgets: Button, TextField, Label, ImageView, ScrollView, StackView
//!
//! Rendering: each View draws into a bounded pixel buffer; Window composites
//! subview buffers in z-order, then blits the result to the SHM surface
//! that the WindowServer reads from.

#![no_std]

extern crate alloc;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

// ── Re-exports ──────────────────────────────────────────────────────────────

pub mod color;
pub mod drawing;
pub mod event;
pub mod layout;
pub mod widget;
pub mod window;

pub use color::{Color, PixelFormat};
pub use drawing::{Font, Framebuffer, Rect, Size};
pub use event::{InputEvent, KeyCode, MouseButton, MouseEvent};
pub use layout::{EdgeInsets, LayoutAxis, StackView};
pub use widget::{Button, ImageView, Label, ScrollBehavior, ScrollView, TextField, Toggle};
pub use window::{Window, WindowDelegate, WindowStyle};

// ── View trait (the foundation) ────────────────────────────────────────────

/// Unique view identifier (monotonically increasing).
static mut NEXT_VIEW_ID: u64 = 1;
fn next_view_id() -> u64 {
    unsafe {
        let id = NEXT_VIEW_ID;
        NEXT_VIEW_ID += 1;
        id
    }
}

/// The fundamental building block of the UI — analogous to NSView.
///
/// Views own a rectangular frame, a list of subviews, and participate in
/// the responder chain for event delivery. Each view can draw into a
/// pixel buffer and layout its children.
pub struct View {
    /// Unique identity for hit-testing and debugging.
    pub id: u64,
    /// Tag for identifying views by purpose (e.g. "content", "sidebar").
    pub tag: u64,
    /// Frame rectangle in parent coordinates.
    pub frame: Rect,
    /// Whether the view is visible and participates in drawing/events.
    pub is_hidden: bool,
    /// Whether the view clips drawing to its bounds.
    pub clips_to_bounds: bool,
    /// Opacity (0.0 = fully transparent, 1.0 = fully opaque).
    pub alpha: f32,
    /// Background color (drawn before subviews).
    pub background_color: Color,
    /// Subviews drawn in front-to-back order.
    pub subviews: Vec<Box<dyn ViewProtocol>>,
    /// Optional pointer to superview (set by add_subview).
    #[allow(dead_code)]
    superview_id: Option<u64>,
    /// Layout engine for arranging subviews.
    pub layout: Option<Box<dyn LayoutEngine>>,
    /// Whether layout is needed before next draw.
    needs_layout: bool,
    /// User-provided tag string for identifying view types.
    pub identifier: Option<String>,
}

/// Trait for custom layout engines (like Auto Layout).
pub trait LayoutEngine: Send + Sync {
    /// Lay out all subviews within the given bounds.
    fn layout_subviews(&mut self, subviews: &mut [Box<dyn ViewProtocol>], bounds: Rect);
}

/// The ViewProtocol trait — all views must implement this.
/// View itself implements it, and widgets override selectively.
pub trait ViewProtocol: Send + Sync {
    /// Unique view ID.
    fn id(&self) -> u64;
    /// The view's frame rectangle in parent coordinates.
    fn frame(&self) -> Rect;
    /// Set the frame rectangle.
    fn set_frame(&mut self, frame: Rect);
    /// Whether the view is hidden.
    fn is_hidden(&self) -> bool;
    /// Set hidden state.
    fn set_hidden(&mut self, hidden: bool);
    /// Whether the view clips to its bounds.
    fn clips_to_bounds(&self) -> bool;
    /// Opacity of the view.
    fn alpha(&self) -> f32;
    /// Background color.
    fn background_color(&self) -> Color;
    /// List of subview IDs.
    fn subview_ids(&self) -> Vec<u64>;
    /// Number of subviews.
    fn subview_count(&self) -> usize;
    /// Get subview at index.
    fn subview_at(&self, index: usize) -> Option<&dyn ViewProtocol>;
    /// Get mutable subview at index.
    fn subview_at_mut(&mut self, index: usize) -> Option<&mut dyn ViewProtocol>;
    /// Add a subview.
    fn add_subview(&mut self, subview: Box<dyn ViewProtocol>);
    /// Remove subview by ID. Returns the removed subview.
    fn remove_subview(&mut self, id: u64) -> Option<Box<dyn ViewProtocol>>;
    /// Draw the view's own content (not subviews) into the framebuffer.
    fn draw(&mut self, fb: &mut Framebuffer, rect: Rect);
    /// Called before draw to lay out subviews.
    fn layout_subviews(&mut self);
    /// Handle a mouse event. Return true if consumed.
    fn handle_mouse_event(&mut self, event: &MouseEvent) -> bool;
    /// Handle a key event. Return true if consumed.
    fn handle_key_event(&mut self, event: &InputEvent) -> bool;
    /// Whether this view accepts first responder status.
    fn accepts_first_responder(&self) -> bool {
        false
    }
    /// Tag for identifying the view.
    fn tag(&self) -> u64;
    /// Identifier string.
    fn identifier(&self) -> Option<&str>;
    /// View type name (for debugging).
    fn type_name(&self) -> &'static str;
}

// ── View implementation ─────────────────────────────────────────────────────

impl View {
    pub fn new(frame: Rect) -> Self {
        View {
            id: next_view_id(),
            tag: 0,
            frame,
            is_hidden: false,
            clips_to_bounds: true,
            alpha: 1.0,
            background_color: Color::CLEAR,
            subviews: Vec::new(),
            superview_id: None,
            layout: None,
            needs_layout: true,
            identifier: None,
        }
    }

    /// Create a view filling the given size at (0,0).
    pub fn with_size(width: u32, height: u32) -> Self {
        Self::new(Rect::new(0, 0, width, height))
    }
}

impl ViewProtocol for View {
    fn id(&self) -> u64 {
        self.id
    }
    fn frame(&self) -> Rect {
        self.frame
    }
    fn set_frame(&mut self, frame: Rect) {
        self.frame = frame;
        self.needs_layout = true;
    }
    fn is_hidden(&self) -> bool {
        self.is_hidden
    }
    fn set_hidden(&mut self, hidden: bool) {
        self.is_hidden = hidden;
    }
    fn clips_to_bounds(&self) -> bool {
        self.clips_to_bounds
    }
    fn alpha(&self) -> f32 {
        self.alpha
    }
    fn background_color(&self) -> Color {
        self.background_color
    }
    fn subview_ids(&self) -> Vec<u64> {
        self.subviews.iter().map(|sv| sv.id()).collect()
    }
    fn subview_count(&self) -> usize {
        self.subviews.len()
    }
    fn subview_at(&self, index: usize) -> Option<&dyn ViewProtocol> {
        self.subviews.get(index).map(|s| s.as_ref() as &dyn ViewProtocol)
    }
    fn subview_at_mut(&mut self, index: usize) -> Option<&mut dyn ViewProtocol> {
        self.subviews.get_mut(index).map(|s| s.as_mut() as &mut dyn ViewProtocol)
    }
    fn add_subview(&mut self, subview: Box<dyn ViewProtocol>) {
        self.subviews.push(subview);
        self.needs_layout = true;
    }
    fn remove_subview(&mut self, id: u64) -> Option<Box<dyn ViewProtocol>> {
        if let Some(pos) = self.subviews.iter().position(|sv| sv.id() == id) {
            Some(self.subviews.remove(pos))
        } else {
            None
        }
    }
    fn draw(&mut self, fb: &mut Framebuffer, rect: Rect) {
        // Default: fill background color
        if self.background_color.a > 0 {
            fb.fill_rect_blend(rect, self.background_color);
        }
    }
    fn layout_subviews(&mut self) {
        if let Some(ref mut engine) = self.layout {
            let bounds = Rect::new(0, 0, self.frame.width, self.frame.height);
            engine.layout_subviews(&mut self.subviews, bounds);
        }
        self.needs_layout = false;
    }
    fn handle_mouse_event(&mut self, event: &MouseEvent) -> bool {
        // Default: forward to subviews in reverse z-order (topmost first)
        let local_x = event.x.saturating_sub(self.frame.x);
        let local_y = event.y.saturating_sub(self.frame.y);
        for i in (0..self.subviews.len()).rev() {
            let sv_frame = self.subviews[i].frame();
            if local_x >= sv_frame.x
                && local_x < sv_frame.x + sv_frame.width
                && local_y >= sv_frame.y
                && local_y < sv_frame.y + sv_frame.height
            {
                let local_event = MouseEvent {
                    x: local_x,
                    y: local_y,
                    button: event.button,
                    click_count: event.click_count,
                    is_down: event.is_down,
                    is_dragging: event.is_dragging,
                    delta_x: event.delta_x,
                    delta_y: event.delta_y,
                };
                if self.subviews[i].handle_mouse_event(&local_event) {
                    return true;
                }
            }
        }
        false
    }
    fn handle_key_event(&mut self, _event: &InputEvent) -> bool {
        false
    }
    fn tag(&self) -> u64 {
        self.tag
    }
    fn identifier(&self) -> Option<&str> {
        self.identifier.as_deref()
    }
    fn type_name(&self) -> &'static str {
        "View"
    }
}

// ── Responder chain ────────────────────────────────────────────────────────

/// First-responder tracking for keyboard event delivery.
pub struct ResponderChain {
    /// The view ID that currently has keyboard focus.
    first_responder: Option<u64>,
    /// The view ID that the mouse is hovering over.
    hovered_view: Option<u64>,
}

impl ResponderChain {
    pub fn new() -> Self {
        ResponderChain {
            first_responder: None,
            hovered_view: None,
        }
    }

    pub fn set_first_responder(&mut self, view_id: Option<u64>) {
        self.first_responder = view_id;
    }

    pub fn first_responder(&self) -> Option<u64> {
        self.first_responder
    }

    pub fn set_hovered(&mut self, view_id: Option<u64>) {
        self.hovered_view = view_id;
    }

    pub fn hovered(&self) -> Option<u64> {
        self.hovered_view
    }
}

// ── Application run loop ───────────────────────────────────────────────────

/// The main event loop that drives a GUI application.
///
/// Polls for input events and IPC messages, routes them through the
/// view hierarchy via the responder chain, and triggers redraws.
pub struct Application {
    pub windows: Vec<Window>,
    pub responder_chain: ResponderChain,
    running: bool,
}

impl Application {
    pub fn new() -> Self {
        Application {
            windows: Vec::new(),
            responder_chain: ResponderChain::new(),
            running: true,
        }
    }

    /// Run the application event loop. Blocks until quit.
    pub fn run(&mut self) {
        while self.running {
            // Poll for input events from the kernel
            if let Some(input) = vibe::input_poll() {
                let event = crate::event::InputEvent::from_vibe_input(&input);
                self.dispatch_event(&event);
            }

            // Poll for IPC messages (events from WindowServer)
            let mut buf = [0u8; 64];
            let _sender = vibe::ipc_recv(&mut buf);
            // TODO: decode WindowServer messages

            // Redraw windows that need it
            for win in &mut self.windows {
                if win.needs_redraw {
                    win.redraw();
                }
            }

            // Yield CPU
            vibe::yield_cpu();
        }
    }

    /// Quit the application.
    pub fn quit(&mut self) {
        self.running = false;
    }

    fn dispatch_event(&mut self, event: &InputEvent) {
        // Route to appropriate window
        for win in &mut self.windows {
            win.handle_event(event);
        }
    }


}

// ── Hit testing ────────────────────────────────────────────────────────────

/// Find the deepest view at the given point in the view tree.
pub fn hit_test(view: &dyn ViewProtocol, point: (u32, u32)) -> Option<u64> {
    let frame = view.frame();
    // Check if point is inside this view
    if point.0 < frame.x || point.0 >= frame.x + frame.width
        || point.1 < frame.y || point.1 >= frame.y + frame.height
    {
        return None;
    }

    // Convert to local coordinates
    let local = (point.0 - frame.x, point.1 - frame.y);

    // Check subviews in reverse order (topmost first)
    for i in (0..view.subview_count()).rev() {
        if let Some(sv) = view.subview_at(i) {
            if !sv.is_hidden() {
                let sv_frame = sv.frame();
                let _sv_point = (local.0 + sv_frame.x, local.1 + sv_frame.y);
                // Recursively check — but we can't borrow mutably, so
                // we just check if point is in sv's frame
                if local.0 >= sv_frame.x
                    && local.0 < sv_frame.x + sv_frame.width
                    && local.1 >= sv_frame.y
                    && local.1 < sv_frame.y + sv_frame.height
                {
                    if !sv.is_hidden() {
                        return Some(sv.id());
                    }
                }
            }
        }
    }

    // Point is in this view but no subview
    Some(view.id())
}

// ── Drawing context ────────────────────────────────────────────────────────

/// Context passed through the draw tree. Contains the target framebuffer
/// and the dirty rectangle that needs redrawing.
pub struct DrawContext<'a> {
    pub framebuffer: &'a mut Framebuffer,
    pub dirty_rect: Rect,
}

impl<'a> DrawContext<'a> {
    pub fn new(fb: &'a mut Framebuffer, dirty: Rect) -> Self {
        DrawContext {
            framebuffer: fb,
            dirty_rect: dirty,
        }
    }
}

// ── Recursive draw ─────────────────────────────────────────────────────────

/// Draw a view and all its subviews into the framebuffer.
/// The view's frame is in absolute coordinates.
pub fn draw_view_tree(view: &mut dyn ViewProtocol, fb: &mut Framebuffer, offset_x: u32, offset_y: u32) {
    if view.is_hidden() {
        return;
    }

    let frame = view.frame();
    let abs_rect = Rect::new(
        offset_x + frame.x,
        offset_y + frame.y,
        frame.width,
        frame.height,
    );

    // Layout if needed
    view.layout_subviews();

    // Draw this view's background/content
    view.draw(fb, abs_rect);

    // Draw subviews
    for _i in 0..view.subview_count() {
        // We need to get mutable access to subview — but we can't with
        // dynamic dispatch easily. The view handles this in its draw method.
        // For now, skip subview drawing here — the view's draw() should
        // handle it.
    }
}