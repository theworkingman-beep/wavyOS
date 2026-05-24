//! Layout engine — stack views, edge insets, auto-layout.

use crate::drawing::Rect;
use crate::{ViewProtocol, LayoutEngine};
use alloc::boxed::Box;

/// Edge insets for padding.
#[derive(Clone, Copy, Debug)]
pub struct EdgeInsets {
    pub top: u32,
    pub left: u32,
    pub bottom: u32,
    pub right: u32,
}

impl EdgeInsets {
    pub const fn zero() -> Self {
        EdgeInsets { top: 0, left: 0, bottom: 0, right: 0 }
    }

    pub const fn all(v: u32) -> Self {
        EdgeInsets { top: v, left: v, bottom: v, right: v }
    }

    pub fn horizontal(&self) -> u32 {
        self.left + self.right
    }

    pub fn vertical(&self) -> u32 {
        self.top + self.bottom
    }
}

/// Layout axis for StackView.
#[derive(Clone, Copy, Debug)]
pub enum LayoutAxis {
    Vertical,
    Horizontal,
}

/// Alignment within a stack.
#[derive(Clone, Copy, Debug)]
pub enum LayoutAlignment {
    Leading,
    Center,
    Trailing,
    Fill,
}

/// Distribution of space in a stack.
#[derive(Clone, Copy, Debug)]
pub enum LayoutDistribution {
    EqualSpacing,
    FillEqually,
    FillProportionally,
}

/// Stack layout engine — arranges subviews in a row or column.
pub struct StackView {
    pub axis: LayoutAxis,
    pub alignment: LayoutAlignment,
    pub distribution: LayoutDistribution,
    pub spacing: u32,
    pub padding: EdgeInsets,
}

impl StackView {
    pub fn vertical() -> Self {
        StackView {
            axis: LayoutAxis::Vertical,
            alignment: LayoutAlignment::Fill,
            distribution: LayoutDistribution::EqualSpacing,
            spacing: 8,
            padding: EdgeInsets::all(8),
        }
    }

    pub fn horizontal() -> Self {
        StackView {
            axis: LayoutAxis::Horizontal,
            alignment: LayoutAlignment::Fill,
            distribution: LayoutDistribution::EqualSpacing,
            spacing: 8,
            padding: EdgeInsets::all(8),
        }
    }
}

impl LayoutEngine for StackView {
    fn layout_subviews(&mut self, subviews: &mut [Box<dyn ViewProtocol>], bounds: Rect) {
        if subviews.is_empty() {
            return;
        }

        let content_x = bounds.x + self.padding.left;
        let content_y = bounds.y + self.padding.top;
        let content_w = bounds.width.saturating_sub(self.padding.horizontal());
        let content_h = bounds.height.saturating_sub(self.padding.vertical());

        match self.axis {
            LayoutAxis::Vertical => {
                let n = subviews.len() as u32;
                // Calculate total height of all subviews
                let total_natural_h: u32 = subviews.iter().map(|sv| sv.frame().height).sum();
                let total_spacing = self.spacing * n.saturating_sub(1);

                match self.distribution {
                    LayoutDistribution::FillEqually => {
                        let avail_h = content_h.saturating_sub(total_spacing);
                        let each_h = avail_h / n.max(1);
                        let mut cy = content_y;
                        for sv in subviews.iter_mut() {
                            sv.set_frame(Rect::new(content_x, cy, content_w, each_h));
                            cy += each_h + self.spacing;
                        }
                    }
                    LayoutDistribution::EqualSpacing => {
                        let extra = content_h.saturating_sub(total_natural_h);
                        let gap = if n > 1 { extra / (n - 1) } else { 0 };
                        let mut cy = content_y;
                        for sv in subviews.iter_mut() {
                            let h = sv.frame().height;
                            let w = match self.alignment {
                                LayoutAlignment::Fill => content_w,
                                LayoutAlignment::Leading | LayoutAlignment::Trailing => sv.frame().width.min(content_w),
                                LayoutAlignment::Center => sv.frame().width.min(content_w),
                            };
                            sv.set_frame(Rect::new(content_x, cy, w, h));
                            cy += h + gap.max(self.spacing);
                        }
                    }
                    LayoutDistribution::FillProportionally => {
                        let avail_h = content_h.saturating_sub(total_spacing);
                        let mut cy = content_y;
                        for sv in subviews.iter_mut() {
                            let ratio = if total_natural_h > 0 {
                                sv.frame().height as f32 / total_natural_h as f32
                            } else {
                                1.0 / n as f32
                            };
                            let h = (avail_h as f32 * ratio) as u32;
                            sv.set_frame(Rect::new(content_x, cy, content_w, h));
                            cy += h + self.spacing;
                        }
                    }
                }
            }
            LayoutAxis::Horizontal => {
                let n = subviews.len() as u32;
                let total_natural_w: u32 = subviews.iter().map(|sv| sv.frame().width).sum();
                let total_spacing = self.spacing * n.saturating_sub(1);

                match self.distribution {
                    LayoutDistribution::FillEqually => {
                        let avail_w = content_w.saturating_sub(total_spacing);
                        let each_w = avail_w / n.max(1);
                        let mut cx = content_x;
                        for sv in subviews.iter_mut() {
                            sv.set_frame(Rect::new(cx, content_y, each_w, content_h));
                            cx += each_w + self.spacing;
                        }
                    }
                    LayoutDistribution::EqualSpacing => {
                        let extra = content_w.saturating_sub(total_natural_w);
                        let gap = if n > 1 { extra / (n - 1) } else { 0 };
                        let mut cx = content_x;
                        for sv in subviews.iter_mut() {
                            let w = sv.frame().width;
                            let h = match self.alignment {
                                LayoutAlignment::Fill => content_h,
                                LayoutAlignment::Leading | LayoutAlignment::Trailing => sv.frame().height.min(content_h),
                                LayoutAlignment::Center => sv.frame().height.min(content_h),
                            };
                            sv.set_frame(Rect::new(cx, content_y, w, h));
                            cx += w + gap.max(self.spacing);
                        }
                    }
                    LayoutDistribution::FillProportionally => {
                        let avail_w = content_w.saturating_sub(total_spacing);
                        let mut cx = content_x;
                        for sv in subviews.iter_mut() {
                            let ratio = if total_natural_w > 0 {
                                sv.frame().width as f32 / total_natural_w as f32
                            } else {
                                1.0 / n as f32
                            };
                            let w = (avail_w as f32 * ratio) as u32;
                            sv.set_frame(Rect::new(cx, content_y, w, content_h));
                            cx += w + self.spacing;
                        }
                    }
                }
            }
        }
    }
}

/// Simple absolute positioning layout — just places subviews at their
/// requested frames without adjustment.
pub struct AbsoluteLayout;

impl LayoutEngine for AbsoluteLayout {
    fn layout_subviews(&mut self, _subviews: &mut [Box<dyn ViewProtocol>], _bounds: Rect) {
        // No-op: subviews keep their existing frames
    }
}