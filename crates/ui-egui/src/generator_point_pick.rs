//! Picking a 3D generator's point on the mesh: its crosshair button arms it, the next click in the 3D view sets the point, Esc or the button again cancels

use texturecraft_engine::core::LayerId;
use texturecraft_engine::{Session, StackTarget};

/// Which point of which generator effect the next click in the 3D view sets
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PointPick {
    pub(crate) effect: u64,

    /// `effect.generator.pickPoint`'s point: distance, start or end
    pub(crate) point: &'static str,
}

/// Where the armed pick is kept: a transient UI mode, like the depth of field's focus pick
fn pick_id() -> egui::Id {
    egui::Id::unique("tc_generator_point_pick")
}

/// The point the next click in the 3D view sets, if one is armed
pub(crate) fn armed(ctx: &egui::Context) -> Option<PointPick> {
    ctx.data(|data| data.get_temp::<Option<PointPick>>(pick_id())).flatten()
}

/// Arms `pick`, or disarms it when it is the one armed
pub(crate) fn toggle(ctx: &egui::Context, pick: PointPick) {
    let next = if armed(ctx) == Some(pick) { None } else { Some(pick) };
    set(ctx, next);
}

/// The armed pick while its effect is still the selected one; a pick whose effect was deselected or deleted is dropped
pub(crate) fn armed_on(ctx: &egui::Context, session: &Session) -> Option<PointPick> {
    let pick = armed(ctx)?;
    if session.target == StackTarget::Effect(LayerId(pick.effect)) {
        return Some(pick);
    }
    set(ctx, None);
    None
}

pub(crate) fn set(ctx: &egui::Context, pick: Option<PointPick>) {
    ctx.data_mut(|data| data.insert_temp(pick_id(), pick));
}
