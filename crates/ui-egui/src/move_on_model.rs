//! Move on model: while it is on, a drag in the 3D view slides the selected fill's projection, the wheel resizes it and Shift+drag turns it; a click places a planar or warp frame where it lands

use egui::{PointerButton, Pos2, Rect, Ui};
use serde_json::{Value, json};
use texturecraft_engine::core::{Decal, EffectKind, LayerId, LayerKind, Projection};

use crate::TextureCraftApp;
use crate::frame_gizmo::FrameGizmo;
use crate::trackpad_drag::TrackpadDrag;

/// The fill whose projection Move on model moves: a fill layer, or a content fill effect in it
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MoveOnModel {
    pub layer: LayerId,
    pub effect: Option<LayerId>,
}

impl MoveOnModel {
    pub fn layer(layer: LayerId) -> MoveOnModel {
        MoveOnModel { layer, effect: None }
    }

    /// The `layer` or `effect` param of the fill commands
    pub(crate) fn target(self) -> Value {
        match self.effect {
            Some(effect) => json!({"effect": effect.0}),
            None => json!({"layer": self.layer.0}),
        }
    }

    pub(crate) fn projection(self, app: &TextureCraftApp) -> Option<Projection> {
        let project = &app.session.doc.project;
        match self.effect {
            Some(effect) => match &project.effect(effect)?.1.kind {
                EffectKind::Material(fill) => Some(fill.projection.clone()),
                _ => None,
            },
            None => match &project.layer(self.layer).ok()?.kind {
                LayerKind::Fill(fill) => Some(fill.projection.clone()),
                _ => None,
            },
        }
    }
}

fn drag_id() -> egui::Id {
    egui::Id::unique("tc_trackpad_drag")
}

/// When the last wheel step resized the projection: steps close together are one undo step
fn wheel_id() -> egui::Id {
    egui::Id::unique("tc_trackpad_wheel")
}

/// Image repeats a drag across the view's height slides by
const SLIDE_PER_HEIGHT: f32 = 1.0;
/// Degrees a Shift+drag turns per point
const TURN_PER_POINT: f32 = 0.5;
/// Growth per wheel point (exponential)
const RESIZE_PER_POINT: f32 = 0.0015;
/// Wheel steps this close together (seconds) merge into one undo step
const WHEEL_MERGE: f64 = 0.6;

/// Whether Move on model takes the 3D view's wheel (it resizes instead of zooming)
pub(crate) fn takes_wheel(app: &TextureCraftApp) -> bool {
    app.moving_on_model.is_some() && app.ui.mode == crate::state::Mode::Painting
}

/// Trackpad input over the 3D view; returns whether Move on model owns the pointer
pub(crate) fn input(app: &mut TextureCraftApp, ui: &Ui, response: &egui::Response, rect: Rect, blocked: bool) -> bool {
    let Some(target) = app.moving_on_model.filter(|_| app.ui.mode == crate::state::Mode::Painting) else { return false };
    // It follows the selection: another layer selected stops it
    let Some(projection) = target.projection(app).filter(|_| app.session.selected_layer() == Some(target.layer)) else {
        app.moving_on_model = None;
        return false;
    };

    if blocked {
        return false;
    }

    let ctx = ui.ctx().clone();
    let frame = FrameGizmo::of(app, rect).filter(|gizmo| target.effect.is_none() && gizmo.layer == target.layer && gizmo.effect.is_none());
    let local = |pointer: Pos2| pointer - rect.left_top();

    // A click puts a planar or warp frame where it lands
    if response.clicked_by(PointerButton::Primary)
        && frame.is_some()
        && let Some(pointer) = response.interact_pointer_pos()
    {
        let (at, viewport) = (local(pointer), app.session.viewport);
        app.run("layer.decal.set", json!({"layer": target.layer.0, "x": at.x, "y": at.y, "viewport": [viewport.x, viewport.y]}));
    }

    let mut drag = ctx.data(|data| data.get_temp::<TrackpadDrag>(drag_id()));
    if response.drag_started_by(PointerButton::Primary)
        && let Some(from) = ctx.input(|input| input.pointer.press_origin())
    {
        let turn = ctx.input(|input| input.modifiers.shift);
        drag = Some(TrackpadDrag { from, start: projection.clone(), frame, turn, stepped: false });
    }
    match drag.filter(|_| response.dragged_by(PointerButton::Primary) || response.drag_started_by(PointerButton::Primary)) {
        Some(mut drag) => {
            ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
            if let Some(pointer) = response.interact_pointer_pos().filter(|_| ctx.input(|input| input.pointer.delta() != egui::Vec2::ZERO))
                && let Some((command, params)) = dragged(&drag, target, pointer - drag.from, rect)
            {
                let params = with_merge(params, drag.stepped);
                drag.stepped |= texturecraft_engine::execute(&mut app.session, command, &params).is_ok();
            }
            ctx.data_mut(|data| data.insert_temp(drag_id(), drag));
        }
        None => {
            ctx.data_mut(|data| data.remove::<TrackpadDrag>(drag_id()));
            if response.hovered() {
                ctx.set_cursor_icon(egui::CursorIcon::Grab);
            }
        }
    }

    let wheel = if response.hovered() { ctx.input(|input| input.smooth_scroll_delta.y) } else { 0.0 };
    if wheel != 0.0
        && let Some((command, params)) = resized(&projection, frame.as_ref(), target, (RESIZE_PER_POINT * wheel).exp())
    {
        let now = ctx.input(|input| input.time);
        let last = ctx.data(|data| data.get_temp::<f64>(wheel_id()));
        let merge = last.is_some_and(|last| now - last < WHEEL_MERGE);
        if texturecraft_engine::execute(&mut app.session, command, &with_merge(params, merge)).is_ok() {
            ctx.data_mut(|data| data.insert_temp(wheel_id(), now));
        }
    }
    true
}

fn with_merge(mut params: Value, merge: bool) -> Value {
    params["merge"] = json!(merge);
    params
}

/// A projection's UV transformations: tiling, rotation, offset (None for Fill (Match per UV Tile))
fn placement(projection: &Projection) -> Option<([f32; 2], f32, [f32; 2])> {
    match projection {
        Projection::UvTile => None,
        Projection::Uv { scale, rotation, offset } => Some((*scale, *rotation, *offset)),
        Projection::Triplanar { uv, .. }
        | Projection::Planar { uv, .. }
        | Projection::Spherical { uv, .. }
        | Projection::Cylindrical { uv, .. }
        | Projection::Warp { uv, .. } => Some((uv.tiling, uv.rotation, uv.offset)),
    }
}

/// The edit a trackpad drag of `delta` makes: a frame slides across its plane or turns; other projections slide or turn their image
fn dragged(drag: &TrackpadDrag, target: MoveOnModel, delta: egui::Vec2, rect: Rect) -> Option<(&'static str, Value)> {
    if let Some(gizmo) = &drag.frame {
        let mut params =
            if drag.turn { json!({"rotation": gizmo.decal.rotation - delta.x * TURN_PER_POINT}) } else { json!({"offset": gizmo.slid_on_screen(delta)?}) };
        params["layer"] = json!(target.layer.0);
        return Some(("layer.decal.set", params));
    }

    let (_, rotation, offset) = placement(&drag.start)?;
    let slide = delta * SLIDE_PER_HEIGHT / rect.height().max(1.0);
    let offset = [offset[0] - slide.x, offset[1] - slide.y];
    let mut params = match (drag.turn, &drag.start) {
        (true, _) => json!({"rotation": rotation + delta.x * TURN_PER_POINT}),
        (false, Projection::Uv { .. }) => json!({"offset": offset}),
        (false, _) => json!({"uvOffset": offset}),
    };
    merge_target(&mut params, target);
    Some(("layer.fill.projection", params))
}

/// The edit a wheel step growing the projection by `growth` makes: a frame grows, other projections' images repeat less
fn resized(projection: &Projection, frame: Option<&FrameGizmo>, target: MoveOnModel, growth: f32) -> Option<(&'static str, Value)> {
    if let Some(Decal { size, .. }) = frame.map(|gizmo| gizmo.decal) {
        return Some(("layer.decal.set", json!({"layer": target.layer.0, "size": [size[0] * growth, size[1] * growth]})));
    }

    let (tiling, _, _) = placement(projection)?;
    let mut params = json!({"tiling": [tiling[0] / growth, tiling[1] / growth]});
    merge_target(&mut params, target);
    Some(("layer.fill.projection", params))
}

fn merge_target(params: &mut Value, target: MoveOnModel) {
    if let (Some(params), Value::Object(fields)) = (params.as_object_mut(), target.target()) {
        params.extend(fields);
    }
}

#[cfg(test)]
mod tests {
    use egui::{Modifiers, vec2};
    use egui_kittest::kittest::Queryable;
    use texturecraft_engine::core::{Projection, UvTransform};

    use super::*;
    use crate::frame_gizmo::tests::{drag, fill_app, projection};

    fn uv_of(projection: &Projection) -> UvTransform {
        match projection {
            Projection::Triplanar { uv, .. } => *uv,
            other => panic!("tri-planar: {other:?}"),
        }
    }

    #[test]
    fn move_on_model_slides_resizes_and_turns_the_projection_until_esc() {
        let (mut harness, layer) = fill_app("triplanar");
        harness.get_by_label("Move on model").click();
        harness.run_steps(2);
        assert_eq!(harness.state().moving_on_model, Some(MoveOnModel::layer(layer)));
        let steps = harness.state().session.history_labels().len();
        // On the model, away from the projector's handles in the middle
        let middle = harness.state().viewport_rect.center() + vec2(-100.0, 80.0);

        drag(&mut harness, middle, middle + vec2(60.0, 0.0), Modifiers::NONE);
        let slid = uv_of(&projection(&harness, layer));
        assert!(slid.offset[0] < 0.0 && slid.offset[1] == 0.0, "a drag right slides the image right: {slid:?}");

        drag(&mut harness, middle, middle + vec2(40.0, 0.0), Modifiers::SHIFT);
        let turned = uv_of(&projection(&harness, layer));
        assert!(turned.rotation > 0.0 && turned.offset == slid.offset, "Shift+drag turns it: {turned:?}");

        let camera = harness.state().session.camera;
        harness.event(egui::Event::PointerMoved(middle));
        harness.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: vec2(0.0, 120.0),
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Unknown,
            modifiers: Modifiers::NONE,
        });
        harness.run_steps(4);
        let resized = uv_of(&projection(&harness, layer));
        assert!(resized.tiling[0] < turned.tiling[0] && resized.tiling[0] == resized.tiling[1], "the wheel up makes the image bigger: {resized:?}");
        assert_eq!(harness.state().session.history_labels().len(), steps + 3, "one undo step per drag and per wheel turn");
        assert_eq!(camera, harness.state().session.camera, "the wheel does not zoom");

        harness.key_press(egui::Key::Escape);
        harness.run_steps(2);
        assert_eq!(harness.state().moving_on_model, None, "Esc stops it");
        drag(&mut harness, middle, middle + vec2(60.0, 0.0), Modifiers::NONE);
        assert_eq!(uv_of(&projection(&harness, layer)), resized, "then drags no longer move it");
    }

    #[test]
    fn move_on_model_slides_a_frame_across_its_plane() {
        let (mut harness, layer) = fill_app("warp");
        harness.state_mut().moving_on_model = Some(MoveOnModel::layer(layer));
        let middle = harness.state().viewport_rect.center() + vec2(0.0, 60.0);
        drag(&mut harness, middle, middle + vec2(0.0, -40.0), Modifiers::NONE);
        let Projection::Warp { frame, .. } = projection(&harness, layer) else { panic!("still a warp") };
        assert!(frame.offset[1] > 0.0 && frame.offset[0].abs() < frame.offset[1] * 0.2, "a drag up slides the frame up: {frame:?}");
    }
}
