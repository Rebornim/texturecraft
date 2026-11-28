//! Where the drag ghost and its badge were painted, and what the badge said

use egui::Rect;

use crate::shelf_drag_verdict::ShelfDragVerdict;

/// The ghost as the last pass painted it
#[derive(Clone, Copy, Debug)]
pub struct ShelfDragGhostShown {
    /// What the badge says
    pub verdict: ShelfDragVerdict,
    /// Where the thumbnail (or the name chip) is
    pub ghost: Rect,
    /// Where the badge is
    pub badge: Rect,
}
