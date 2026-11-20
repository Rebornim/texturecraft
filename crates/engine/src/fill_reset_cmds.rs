//! Reset of a fill's Fill section: its projection, placement and sampling back to a new fill's, its symmetry and material kept

use serde_json::Value;
use texturecraft_core::{Projection, Sampling};

use crate::commands::{CommandSpec, has_layer};
use crate::layer_cmds::{fill_target, placement_mut};
use crate::{Result, Session, cmd};

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd!(
        "layer.fill.resetProjection",
        "Reset fill settings",
        [],
        None,
        "{layer? | effect?: content or mask fill effect id} — the Fill section back to a new fill's in one undo step: UV projection at tiling 1, no offset or rotation, the default filtering, wrap and scale mode (symmetry, sources and material stay)",
        has_layer,
        reset_projection
    )]
}

/// The sampling a reset Fill section has: a new fill's, with `current`'s symmetry (it belongs to the Symmetry section)
pub fn reset_sampling(current: Sampling) -> Sampling {
    Sampling { symmetry: current.symmetry, ..Sampling::default() }
}

/// Whether the Fill section of a fill projected by `projection` and sampled by `sampling` is already a new fill's
pub fn at_defaults(projection: &Projection, sampling: Sampling) -> bool {
    *projection == Projection::default() && sampling == reset_sampling(sampling)
}

fn reset_projection(s: &mut Session, p: &Value) -> Result<Value> {
    let target = fill_target(s, p, "layer.fill.resetProjection")?;
    s.edit("Reset fill settings", |s| {
        let (projection, sampling) = placement_mut(s, target)?;
        *projection = Projection::default();
        *sampling = reset_sampling(*sampling);
        Ok(())
    })?;
    Ok(Value::Null)
}
