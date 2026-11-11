//! How a shelf tile lays out its thumbnail and name

use egui::vec2;

/// How a tile lays out its thumbnail and name in the tile's fixed size
#[derive(Clone, Copy)]
pub(crate) struct TileLook {
    /// The thumbnail's size and its center below the tile's top, in points
    pub thumbnail_size: f32,
    pub thumbnail_center_y: f32,
    /// The name on one line cut with "…" (the hover preview shows all of it), else wrapped
    pub one_line_name: bool,
}

impl TileLook {
    /// The shelf's tiles: the name wraps under the thumbnail
    pub const SHELF: TileLook = TileLook { thumbnail_size: 56.0, thumbnail_center_y: 32.0, one_line_name: false };
    /// A thumbnail filling the tile's width, over a one-line name
    pub const LARGE_THUMBNAIL: TileLook = TileLook { thumbnail_size: 70.0, thumbnail_center_y: 36.0, one_line_name: true };

    /// Where this look draws the thumbnail in tile `rect`
    pub fn thumbnail_rect(self, rect: egui::Rect) -> egui::Rect {
        egui::Rect::from_center_size(rect.center_top() + vec2(0.0, self.thumbnail_center_y), vec2(self.thumbnail_size, self.thumbnail_size))
    }
}
