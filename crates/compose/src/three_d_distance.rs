//! The 3D Distance generator: a gradient falling off with the distance from a point across the mesh's bounds

use texturecraft_core::DistanceParameters;
use texturecraft_geom::glam::Vec3;

use crate::histogram_scan::histogram_scan;

/// The value at `position` (0..1 across the bounds, the longest side spanning 0..1): 1 at the point, 0 at the radius, then scanned by the offset and contrast
pub(crate) fn distance_value(distance: &DistanceParameters, position: Vec3) -> f32 {
    let from_point = position.distance(Vec3::from(distance.point));
    let gradient = if distance.radius > 0.0 { (1.0 - from_point / distance.radius).clamp(0.0, 1.0) } else { 0.0 };
    histogram_scan(gradient, distance.offset, distance.contrast)
}
