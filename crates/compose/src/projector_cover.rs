//! How much of a texel a 3D projection reaches: its shape crop and its backface culling

use texturecraft_core::{Backface, Projection, ShapeCrop};
use texturecraft_geom::glam::Vec3;

use crate::pattern::{Coords, projector_local};

/// How much of the texel at `c` the projection reaches through its shape crop and backface culling
pub fn projector_cover(proj: &Projection, c: &Coords) -> f32 {
    let projector = match proj {
        Projection::Triplanar { projector, .. }
        | Projection::Planar { projector, .. }
        | Projection::Spherical { projector, .. }
        | Projection::Cylindrical { projector, .. } => projector,
        Projection::Uv { .. } | Projection::UvTile | Projection::Decal(_) => return 1.0,
    };
    if projector.crop == ShapeCrop::ExtendsOutside && !projector.backface.on {
        return 1.0;
    }
    let local = projector_local(projector, c);
    let (inside, facing) = match proj {
        Projection::Triplanar { .. } => (local.pos.abs().max_element() <= 1.0, None),
        Projection::Planar { scale, axis, offset, .. } => {
            let p = local.pos * *scale + Vec3::from(*offset);
            // The frame across the axis, and the side the image reads the right way round from
            let (frame, facing) = match axis {
                0 => (p.z.abs().max(p.y.abs()), -local.nrm.x),
                1 => (p.x.abs().max(p.z.abs()), local.nrm.y),
                _ => (p.x.abs().max(p.y.abs()), local.nrm.z),
            };
            (frame <= 1.0, Some(facing))
        }
        Projection::Spherical { offset, .. } => ((local.pos + Vec3::from(*offset)).length() <= 1.0, None),
        Projection::Cylindrical { axis, offset, .. } => {
            let p = local.pos + Vec3::from(*offset);
            let n = local.nrm;
            let ((a, h, b), (na, nb)) = match axis {
                0 => ((p.y, p.x, p.z), (n.y, n.z)),
                2 => ((p.x, p.z, p.y), (n.x, n.y)),
                _ => ((p.x, p.y, p.z), (n.x, n.z)),
            };
            let radius = a.hypot(b);
            let outward = if radius > 1e-6 { (na * a + nb * b) / radius } else { 1.0 };
            (radius <= 1.0 && h.abs() <= 1.0, Some(outward))
        }
        Projection::Uv { .. } | Projection::UvTile | Projection::Decal(_) => (true, None),
    };
    if projector.crop == ShapeCrop::Cropped && !inside {
        return 0.0;
    }
    match facing {
        Some(facing) if projector.backface.on => backface_cover(&projector.backface, facing),
        _ => 1.0,
    }
}

/// How much a face turned `facing` (the cosine of its turn from the projection) keeps
pub fn backface_cover(backface: &Backface, facing: f32) -> f32 {
    // cos(angle) as sin(90 - angle): exactly 0 at 90 degrees
    let threshold = (90.0 - backface.angle).to_radians().sin();
    let width = (1.0 - backface.hardness.clamp(0.0, 1.0)) * BACKFACE_FADE;
    if width <= f32::EPSILON {
        return f32::from(u8::from(facing > threshold));
    }
    let t = ((facing - threshold) / width).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The turn (in cosine) over which backface culling fades at hardness 0
const BACKFACE_FADE: f32 = 0.6;
