//! How long one repeat of a fill's tile is on the mesh (mesh units): Physical size reads it from the tiling and writes tiling back

use std::f32::consts::TAU;

use texturecraft_core::Projection;

/// The mesh's size as the projections see it: the bounds radius (3D projections) and the mesh units one UV unit covers (UV)
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeshSize {
    pub radius: f32,
    pub uv_density: f32,
}

/// Mesh units one tile covers across and down at a tiling of 1; None without a physical size (per UV tile, spherical)
fn unit_tile(projection: &Projection, size: MeshSize) -> Option<[f32; 2]> {
    let r = size.radius;
    match projection {
        Projection::Uv { .. } => Some([size.uv_density; 2]),
        Projection::Triplanar { scale, .. } => Some([r / scale; 2]),
        Projection::Planar { placed: Some(frame), .. } => Some(frame.size),
        Projection::Planar { scale, .. } => Some([2.0 * r / scale; 2]),
        Projection::Cylindrical { scale, angle, .. } => Some([TAU * r * (angle / 360.0) / scale, 2.0 * r / scale]),
        Projection::UvTile | Projection::Spherical { .. } => None,
    }
}

/// The tiling a physical size reads and writes: UV's own tiling, a 3D projection's UV tiling
fn tiling(projection: &Projection) -> Option<[f32; 2]> {
    match projection {
        Projection::Uv { scale, .. } => Some(*scale),
        Projection::Triplanar { uv, .. } | Projection::Planar { uv, .. } | Projection::Cylindrical { uv, .. } => Some(uv.tiling),
        Projection::UvTile | Projection::Spherical { .. } => None,
    }
}

/// Mesh units one repeat of the tile covers across and down
pub fn tile_length(projection: &Projection, size: MeshSize) -> Option<[f32; 2]> {
    let unit = unit_tile(projection, size)?;
    let tiling = tiling(projection)?;
    Some([unit[0] / tiling[0].max(f32::EPSILON), unit[1] / tiling[1].max(f32::EPSILON)])
}

/// The tiling that makes one repeat `length` mesh units long across and down
pub fn tiling_for_length(projection: &Projection, size: MeshSize, length: [f32; 2]) -> Option<[f32; 2]> {
    let unit = unit_tile(projection, size)?;
    Some([unit[0] / length[0].max(f32::EPSILON), unit[1] / length[1].max(f32::EPSILON)])
}

#[cfg(test)]
mod tests {
    use super::*;
    use texturecraft_core::UvTransform;

    const SIZE: MeshSize = MeshSize { radius: 2.0, uv_density: 3.0 };

    #[test]
    fn uv_tile_length_is_the_uv_density_over_the_tiling() {
        let uv = Projection::Uv { scale: [2.0, 4.0], rotation: 0.0, offset: [0.0; 2] };
        assert_eq!(tile_length(&uv, SIZE), Some([1.5, 0.75]));
    }

    #[test]
    fn a_length_read_back_gives_the_same_tiling() {
        let uv = UvTransform { tiling: [3.0, 1.5], ..UvTransform::default() };
        for projection in [
            Projection::Uv { scale: [3.0, 1.5], rotation: 0.0, offset: [0.0; 2] },
            Projection::Triplanar { scale: 1.5, sharpness: 8.0, offset: [0.0; 3], uv, projector: Default::default() },
            Projection::Planar { scale: 0.5, axis: 2, offset: [0.0; 3], uv, projector: Default::default(), placed: None },
            Projection::Cylindrical { scale: 2.0, axis: 1, uv, offset: [0.0; 3], projector: Default::default(), angle: 360.0 },
        ] {
            let length = tile_length(&projection, SIZE).unwrap();
            let tiling = tiling_for_length(&projection, SIZE, length).unwrap();
            assert!((tiling[0] - 3.0).abs() < 1e-5 && (tiling[1] - 1.5).abs() < 1e-5, "{projection:?}: {tiling:?}");
        }
    }

    #[test]
    fn spherical_has_no_physical_size() {
        assert_eq!(tile_length(&Projection::Spherical { scale: 1.0, uv: UvTransform::default(), offset: [0.0; 3], projector: Default::default() }, SIZE), None);
    }
}
