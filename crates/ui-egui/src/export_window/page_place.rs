use egui::{Pos2, pos2};

/// Where a Settings page lays out from: the window's left edge, the tab row's top and the page's right edge
#[derive(Clone, Copy)]
pub(super) struct PagePlace {
    pub(super) left: f32,
    pub(super) top: f32,
    pub(super) right: f32,
}

impl PagePlace {
    /// `x` from the window's left edge
    pub(super) fn x(&self, x: f32) -> f32 {
        self.left + x
    }

    /// `y` from the top of the tab row
    pub(super) fn y(&self, y: f32) -> f32 {
        self.top + y
    }

    /// A point `from_right` in from the page's right edge, at `y` from the tab row's top
    pub(super) fn right_inset(&self, from_right: f32, y: f32) -> Pos2 {
        pos2(self.right - from_right, self.top + y)
    }
}
