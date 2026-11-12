//! Where a planar or warp projection's projector looks from, for depth culling

use texturecraft_core::{Decal, Projection, Projector};
use texturecraft_geom::glam::{Vec2, Vec3};

use crate::pattern::{Coords, frame_local, projector_local};

/// The view of a depth-culled planar or warp projection: points in it run across, up and towards the projector
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ProjectorView {
    /// An axis planar projection, reading positions around the mesh centre in mesh radii
    Axis { axis: u8, projector: Projector, center: Vec3, radius: f32 },
    /// A planar projection placed on the surface or a warp, from its frame
    Frame { frame: Decal, projector: Projector, radius: f32 },
}

impl ProjectorView {
    /// The view of `proj` over a mesh within `bounds`, None unless it is a planar or warp projection with depth culling on
    pub fn of(proj: &Projection, bounds: (Vec3, Vec3)) -> Option<ProjectorView> {
        proj.depth_culling()?;
        let (lo, hi) = bounds;
        let radius = ((hi - lo).length() * 0.5).max(1e-4);
        match proj {
            Projection::Planar { placed: Some(frame), projector, .. } | Projection::Warp { frame, projector, .. } => {
                Some(ProjectorView::Frame { frame: *frame, projector: turn_only(projector), radius })
            }
            Projection::Planar { axis, projector, .. } => {
                Some(ProjectorView::Axis { axis: *axis, projector: turn_only(projector), center: (lo + hi) * 0.5, radius })
            }
            _ => None,
        }
    }

    /// World point `p` in the view: across, up and how near the projector (None where the view is degenerate)
    pub fn local(&self, p: Vec3) -> Option<Vec3> {
        let local = match self {
            ProjectorView::Axis { axis, projector, center, radius } => {
                let at = Coords { uv: Vec2::ZERO, pos: (p - *center) / *radius, nrm: Vec3::Z };
                let l = projector_local(projector, &at).pos;
                // The side each axis projects from, as its backface culling reads it
                match axis {
                    0 => Vec3::new(l.z, l.y, -l.x),
                    1 => Vec3::new(l.x, l.z, l.y),
                    _ => l,
                }
            }
            ProjectorView::Frame { frame, projector, .. } => frame_local(frame, projector, p)?,
        };
        local.is_finite().then_some(local)
    }

    /// The mesh radius in the view's nearness units
    pub fn unit(&self) -> f32 {
        match self {
            ProjectorView::Axis { .. } => 1.0,
            ProjectorView::Frame { radius, .. } => *radius,
        }
    }
}

/// Only what moves the view: the turn and stretch, not the culling settings (changing those keeps the depth map)
fn turn_only(projector: &Projector) -> Projector {
    Projector { rotation: projector.rotation, scale: projector.scale, ..Projector::default() }
}
