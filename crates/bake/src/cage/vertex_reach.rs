use super::RayStart;
use texturecraft_geom::glam::Vec3;

/// One high-poly vertex seen from the transfer ray through its nearest low-poly point
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VertexReach {
    /// How far the vertex is from that low-poly point
    pub distance: f32,
    pub low_position: Vec3,
    pub start: RayStart,
    pub high_position: Vec3,
    pub high_normal: Vec3,
}

impl VertexReach {
    /// Whether the ray misses the vertex at `frontal` and `rear` world units of cage distance; with `ignore_backface` it also misses a vertex facing away from the ray
    pub fn missed(&self, frontal: f32, rear: f32, ignore_backface: bool) -> bool {
        let origin = self.start.origin(self.low_position, frontal);
        let (direction, front) = match self.start {
            RayStart::CageMesh(offset) => (offset.normalize_or_zero(), offset.length()),
            RayStart::Distance { up, scale } => (-up, frontal * scale),
        };
        let along_ray = (self.high_position - origin).dot(direction);

        !(0.0..=front + rear).contains(&along_ray) || (ignore_backface && self.high_normal.dot(direction) > 0.0)
    }
}
