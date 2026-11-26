//! A UV seam side in texel space

use texturecraft_geom::UvSeamSide;
use texturecraft_geom::glam::Vec2;

/// A seam side in texel space, with its unit normal into the island
pub(crate) struct TexelEdge {
    pub(crate) start: Vec2,
    pub(crate) end: Vec2,
    pub(crate) inward: Vec2,
}

impl TexelEdge {
    pub(crate) fn new(side: UvSeamSide, size: Vec2) -> Option<TexelEdge> {
        let (start, end, interior) = (side.start * size, side.end * size, side.interior * size);
        let normal = (end - start).perp().try_normalize()?;
        let inward = if normal.dot(interior - start) < 0.0 { -normal } else { normal };
        Some(TexelEdge { start, end, inward })
    }

    /// How far along the edge (0..1) and how far outside the island `point` is, when it is beside the edge on the outside
    pub(crate) fn outside(&self, point: Vec2) -> Option<(f32, f32)> {
        let direction = self.end - self.start;
        let along = (point - self.start).dot(direction) / direction.length_squared();
        let outside = -(point - self.start).dot(self.inward);
        ((0.0..=1.0).contains(&along) && outside > 0.0).then_some((along, outside))
    }
}
