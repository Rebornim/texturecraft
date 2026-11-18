//! A drag on a handle of the selected fill's projection gizmo, a planar or warp frame or a round projector: one undo step per drag, the projection updating live

mod gizmo;
mod gizmo_drag;
mod shown_gizmo;

use egui::{PointerButton, Rect};
use serde_json::json;

use crate::TextureCraftApp;
use crate::viewport::focus_lost;

pub(crate) use gizmo::Gizmo;
pub(crate) use gizmo_drag::GizmoDrag;
pub(crate) use shown_gizmo::ShownGizmo;

/// The drag under way on a handle of a `G` gizmo
pub(crate) fn dragging<G: Gizmo>(ctx: &egui::Context) -> Option<GizmoDrag<G>> {
    ctx.data(|data| data.get_temp::<GizmoDrag<G>>(G::drag_id()))
}

/// Ends a drag under way on a `G` gizmo where it is: its undo step stays one step
pub(crate) fn let_go<G: Gizmo>(ctx: &egui::Context) {
    ctx.data_mut(|data| data.remove::<GizmoDrag<G>>(G::drag_id()));
}

/// Drags on `gizmo`'s handles (one undo step per drag, the projection updates live); returns whether a drag owns the pointer. The cursor shows what the handle under it, or the one dragged, does (only while the gizmo takes input: an Alt orbit or a held Move on model keeps its own)
pub(crate) fn input<G: Gizmo>(app: &mut TextureCraftApp, ctx: &egui::Context, response: &egui::Response, rect: Rect, gizmo: Option<&G>) -> bool {
    let mut drag = dragging::<G>(ctx).filter(|drag| gizmo.is_some_and(|gizmo| gizmo.same_projection(&drag.start)));
    if response.drag_started_by(PointerButton::Primary)
        && let (Some(gizmo), Some(from)) = (gizmo, ctx.input(|input| input.pointer.press_origin()))
        && let Some(grab) = gizmo.grab(from)
    {
        drag = Some(GizmoDrag { grab, from, start: gizmo.clone(), stepped: false });
    }

    // A window that lost the focus gets no release: the drag ends there
    let pressed = response.dragged_by(PointerButton::Primary) || response.drag_started_by(PointerButton::Primary);
    let Some(mut drag) = drag.filter(|_| pressed && !focus_lost(ctx)) else {
        let_go::<G>(ctx);
        if let Some((gizmo, grab)) = gizmo.and_then(|gizmo| Some((gizmo, gizmo.grab(response.hover_pos()?)?))) {
            ctx.set_cursor_icon(gizmo.cursor(grab, false));
        }
        return false;
    };

    // A drag lives only while its gizmo shows, so the gizmo drawn this frame names the cursor
    ctx.set_cursor_icon(gizmo.unwrap_or(&drag.start).cursor(drag.grab, true));

    if let Some(pointer) = response.interact_pointer_pos().filter(|_| ctx.input(|input| input.pointer.delta() != egui::Vec2::ZERO))
        && let Some((command, mut params)) = drag.start.edit(app, rect, drag.grab, drag.from, pointer)
    {
        params["merge"] = json!(drag.stepped);

        // Off the plane or past the size limits: the projection waits where it is
        drag.stepped |= texturecraft_engine::execute(&mut app.session, command, &params).is_ok();

        // The panel drew before this edit: one more frame shows its fields and preview where the projection is now
        ctx.request_repaint();
    }
    ctx.data_mut(|data| data.insert_temp(G::drag_id(), drag));
    true
}
