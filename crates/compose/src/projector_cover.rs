//! How much of a texel a 3D projection reaches: its shape crop and its backface culling

use texturecraft_core::{Backface, Decal, Projection, Projector, ShapeCrop};
use texturecraft_geom::glam::{Vec2, Vec3};

use crate::pattern::{Coords, depth_fade, frame_crop, frame_facing, frame_local, placed_uv, projector_local, smoothstep};
use crate::warp::warp_uv;

/// Nearer a cylinder's axis than this a texel has no outward direction, so it counts as facing out
const MIN_AXIS_DISTANCE: f32 = 1e-6;

/// How much of the texel at `c` the projection reaches through its shape crop and backface culling
pub fn projector_cover(proj: &Projection, c: &Coords) -> f32 {
    let projector = match proj {
        Projection::Triplanar { projector, .. }
        | Projection::Planar { projector, .. }
        | Projection::Spherical { projector, .. }
        | Projection::Cylindrical { projector, .. } => projector,
        // Framed on the surface: see [`framed_cover`]
        Projection::Warp { .. } | Projection::Uv { .. } | Projection::UvTile => return 1.0,
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
            let position = local.pos + Vec3::from(*offset);
            let normal = local.nrm;
            let ((across, height, depth), (normal_across, normal_depth)) = match axis {
                0 => ((position.y, position.x, position.z), (normal.y, normal.z)),
                2 => ((position.x, position.z, position.y), (normal.x, normal.y)),
                _ => ((position.x, position.y, position.z), (normal.x, normal.z)),
            };
            let radius = across.hypot(depth);
            let outward = if radius > MIN_AXIS_DISTANCE { (normal_across * across + normal_depth * depth) / radius } else { 1.0 };
            (radius <= 1.0 && height.abs() <= 1.0, Some(outward))
        }
        Projection::Warp { .. } | Projection::Uv { .. } | Projection::UvTile => (true, None),
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

    smoothstep(threshold, threshold + width, facing)
}

/// A planar projection placed on the surface at world point `p` with normal `nrm`: its texture coordinates and how much its backface culling and shape crop let through (None when the frame is degenerate)
pub fn placed_cover(frame: &Decal, projector: &Projector, p: Vec3, nrm: Vec3) -> Option<(Vec2, f32)> {
    let local = frame_local(frame, projector, p)?;
    let mut cover = if projector.backface.on { backface_cover(&projector.backface, frame_facing(frame, projector, nrm)) } else { 1.0 };
    if projector.crop == ShapeCrop::Cropped {
        cover *= frame_crop(frame, local);
    }
    Some((placed_uv(local), cover))
}

/// A projection from a frame on the surface (a placed planar or a warp) at world point `p` with normal `nrm`: its texture coordinates and how much it covers there (None for other projections, a degenerate frame or where nothing lands)
pub fn framed_cover(proj: &Projection, p: Vec3, nrm: Vec3) -> Option<(Vec2, f32)> {
    match proj {
        Projection::Planar { projector, placed: Some(frame), .. } => placed_cover(frame, projector, p, nrm),
        Projection::Warp { frame, corners, depth, projector, .. } => warp_cover(frame, corners, *depth, projector, p),
        _ => None,
    }
}

/// A warp projection at world point `p`: its texture coordinates in the quad its corners make, and, cropped to its shape, how much of the quad and of its Projection Depth (in lengths of the frame's longer side) cover it
pub fn warp_cover(frame: &Decal, corners: &[[f32; 2]; 4], depth: f32, projector: &Projector, p: Vec3) -> Option<(Vec2, f32)> {
    let local = frame_local(frame, projector, p)?;
    let uv = warp_uv(corners, local.truncate())?;
    if projector.crop == ShapeCrop::ExtendsOutside {
        return Some((uv, 1.0));
    }

    let inside = (0.0..=1.0).contains(&uv.x) && (0.0..=1.0).contains(&uv.y);
    let reach = depth * frame.size[0].max(frame.size[1]);
    Some((uv, if inside { depth_fade(local.z, reach) } else { 0.0 }))
}

/// The turn (in cosine) over which backface culling fades at hardness 0
const BACKFACE_FADE: f32 = 0.6;

#[cfg(test)]
mod tests {
    use super::*;

    /// The decal formula from before decals were placed planar projections, kept to check they still look the same
    fn decal_before(d: &Decal, p: Vec3, nrm: Vec3) -> Option<(Vec2, f32)> {
        let (o, r, u, n) = crate::pattern::decal_axes(d)?;
        let facing = nrm.dot(n);
        let q = p - o;
        let (lx, ly, lz) = (q.dot(r) / d.size[0].max(1e-6), q.dot(u) / d.size[1].max(1e-6), q.dot(n).abs());
        let depth = d.depth.max(1e-6);
        if facing <= 0.0 || lx.abs() > 0.5 || ly.abs() > 0.5 || lz > depth || !(lx.is_finite() && ly.is_finite()) {
            return None;
        }

        let w = smoothstep(0.0, 0.3, facing) * (1.0 - smoothstep(0.75 * depth, depth, lz));
        Some((Vec2::new(lx + 0.5, 0.5 - ly), w))
    }

    #[test]
    fn saved_decals_cover_exactly_as_before() {
        let frames = [
            Decal { center: [0.0; 3], normal: [0.0, 0.0, 1.0], up: [0.0, 1.0, 0.0], size: [2.0, 1.0], rotation: 0.0, offset: [0.0; 2], depth: 0.5 },
            Decal { center: [0.3, -0.2, 0.1], normal: [0.4, 0.5, 0.7], up: [0.0, 1.0, 0.0], size: [0.7, 1.3], rotation: 37.0, offset: [0.2, -0.1], depth: 0.4 },
        ];
        for frame in frames {
            let Projection::Planar { projector, placed: Some(placed), .. } = Projection::decal(frame) else { panic!("a placed planar projection") };
            for i in 0..4000 {
                let t = i as f32 * 0.137;
                let p = Vec3::new(t.sin() * 1.3, (t * 0.71).cos() * 1.1, (t * 0.33).sin() * 0.8);
                let nrm = Vec3::new((t * 1.7).cos(), (t * 0.9).sin(), (t * 0.4).cos()).normalize();
                let now = placed_cover(&placed, &projector, p, nrm).filter(|(_, cover)| *cover > 0.0);
                match (decal_before(&frame, p, nrm), now) {
                    (Some(before), Some(now)) => assert_eq!(before, now, "{p} {nrm}"),
                    (None, None) => {}
                    (before, now) => panic!("{p} {nrm}: {before:?} became {now:?}"),
                }
            }
        }
    }
}
