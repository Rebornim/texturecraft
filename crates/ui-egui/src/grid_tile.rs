//! A grid tile's measurements in the shelf's small, medium and large views and in pickers

use egui::{Rect, Vec2, vec2};

/// A grid tile's size and its thumbnail's size and center below the tile's top, in points
#[derive(Clone, Copy)]
pub(crate) struct GridTile {
    pub(crate) width: f32,
    pub(crate) height: f32,
    pub(crate) thumbnail_size: f32,
    pub(crate) thumbnail_center_y: f32,
}

impl GridTile {
    /// The shelf's medium tile, which pickers use too
    pub(crate) const MEDIUM: GridTile = GridTile { width: 66.0, height: 76.0, thumbnail_size: 60.0, thumbnail_center_y: 31.0 };
    /// A thumbnail filling the tile's width, four to a picker row
    pub(crate) const PICKER_LARGE: GridTile = GridTile { width: 72.0, height: 86.0, thumbnail_size: 70.0, thumbnail_center_y: 36.0 };

    /// The tile's size
    pub(crate) fn size(self) -> Vec2 {
        vec2(self.width, self.height)
    }

    /// Where the thumbnail goes in the tile `rect`
    pub(crate) fn thumbnail_rect(self, rect: Rect) -> Rect {
        Rect::from_center_size(rect.center_top() + vec2(0.0, self.thumbnail_center_y), vec2(self.thumbnail_size, self.thumbnail_size))
    }
}
