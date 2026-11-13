//! Reset of a fill's Fill section: its projection's settings and its sampling back to a new fill's, its projection kind, symmetry and material kept

use serde_json::Value;
use texturecraft_core::{Projection, Sampling};

use crate::commands::{CommandSpec, has_layer};
use crate::layer_cmds::{fill_target, new_unframed_projection, placement_mut};
use crate::{Result, Session, cmd};

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd!(
        "layer.fill.resetProjection",
        "Reset fill settings",
        [],
        None,
        "{layer? | effect?: content or mask fill effect id} — the Fill section back to a new fill's in one undo step: the same projection kind with its defaults (tiling 1, no offset or rotation, a planar or warp frame stays where it is placed, a warp's corners unpulled), the default filtering, wrap and scale mode (symmetry, sources and material stay)",
        has_layer,
        reset_projection
    )]
}

/// The sampling a reset Fill section has: a new fill's, with `current`'s symmetry (it belongs to the Symmetry section)
pub fn reset_sampling(current: Sampling) -> Sampling {
    Sampling { symmetry: current.symmetry, ..Sampling::default() }
}

/// The defaults of `current`'s projection kind: a new fill's projection of that kind, with the frame a placed planar or a warp projects from kept
pub fn reset_projection_of(current: &Projection) -> Projection {
    let id = match current {
        Projection::Uv { .. } => "uv",
        Projection::UvTile => "uvTile",
        Projection::Triplanar { .. } => "triplanar",
        Projection::Planar { .. } => "planar",
        Projection::Spherical { .. } => "spherical",
        Projection::Cylindrical { .. } => "cylindrical",
        Projection::Warp { frame, .. } => return Projection::warp(*frame),
    };
    let mut fresh = new_unframed_projection(id).unwrap_or_default();
    if let (Projection::Planar { placed, .. }, Projection::Planar { placed: Some(frame), .. }) = (&mut fresh, current) {
        *placed = Some(*frame);
    }
    fresh
}

/// Whether the Fill section of a fill projected by `projection` and sampled by `sampling` is already a new fill's (of the same projection kind)
pub fn at_defaults(projection: &Projection, sampling: Sampling) -> bool {
    *projection == reset_projection_of(projection) && sampling == reset_sampling(sampling)
}

fn reset_projection(s: &mut Session, p: &Value) -> Result<Value> {
    let target = fill_target(s, p, "layer.fill.resetProjection")?;
    s.edit("Reset fill settings", |s| {
        let (projection, sampling) = placement_mut(s, target)?;
        *projection = reset_projection_of(projection);
        *sampling = reset_sampling(*sampling);
        Ok(())
    })?;
    Ok(Value::Null)
}
