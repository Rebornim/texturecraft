//! Move on model: while it is on, a drag in the 3D view slides the selected fill's projection and Shift+drag turns it; a click places a planar or warp frame where it lands. The wheel still zooms the camera

use egui::{PointerButton, Pos2, Rect, Ui};
use serde_json::{Value, json};
use texturecraft_engine::core::{EffectKind, LayerId, LayerKind, Projection};

use crate::TextureCraftApp;
use crate::frame_gizmo::FrameGizmo;
use crate::round_gizmo::RoundGizmo;
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

    /// The selected fill layer, or the selected fill effect in it (its content or its mask); its projector shows in the 3D view
    pub(crate) fn selected(app: &TextureCraftApp) -> Option<MoveOnModel> {
        let layer = app.session.selected_layer()?;
        let effect = match app.session.target {
            texturecraft_engine::StackTarget::Content => None,
            texturecraft_engine::StackTarget::Effect(effect) => Some(effect),
            texturecraft_engine::StackTarget::Mask => return None,
        };
        Some(MoveOnModel { layer, effect })
    }

    pub(crate) fn projection(self, app: &TextureCraftApp) -> Option<Projection> {
        let project = &app.session.doc.project;
        match self.effect {
            Some(effect) => match &project.effect(effect)?.1.kind {
                EffectKind::Material(fill) => Some(fill.projection.clone()),
                EffectKind::Fill { projection, .. } => Some(projection.clone()),
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

/// Image repeats a drag across the view's height slides by
const SLIDE_PER_HEIGHT: f32 = 1.0;
/// Degrees a Shift+drag turns per point
const TURN_PER_POINT: f32 = 0.5;

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
        let round = RoundGizmo::of(app, rect);
        drag = Some(TrackpadDrag { from, start: projection, frame, round, turn, stepped: false });
    }
    match drag.filter(|_| response.dragged_by(PointerButton::Primary) || response.drag_started_by(PointerButton::Primary)) {
        Some(mut drag) => {
            ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
            if let Some(pointer) = response.interact_pointer_pos().filter(|_| ctx.input(|input| input.pointer.delta() != egui::Vec2::ZERO))
                && let Some((command, params)) = dragged(app, &drag, target, pointer, rect)
            {
                let params = with_merge(params, drag.stepped);
                drag.stepped |= texturecraft_engine::execute(&mut app.session, command, &params).is_ok();
                // The panel drew before this edit: one more frame shows its fields and preview where the projection is now
                ctx.request_repaint();
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

    true
}

fn with_merge(mut params: Value, merge: bool) -> Value {
    params["merge"] = json!(merge);
    params
}

/// A projection's UV rotation and offset (None for Fill (Match per UV Tile))
fn placement(projection: &Projection) -> Option<(f32, [f32; 2])> {
    match projection {
        Projection::UvTile => None,
        Projection::Uv { rotation, offset, .. } => Some((*rotation, *offset)),
        Projection::Triplanar { uv, .. }
        | Projection::Planar { uv, .. }
        | Projection::Spherical { uv, .. }
        | Projection::Cylindrical { uv, .. }
        | Projection::Warp { uv, .. } => Some((uv.rotation, uv.offset)),
    }
}

/// The edit a trackpad drag to `pointer` makes: a frame or projector slides under the pointer, a UV image slides; Shift turns it
fn dragged(app: &TextureCraftApp, drag: &TrackpadDrag, target: MoveOnModel, pointer: Pos2, rect: Rect) -> Option<(&'static str, Value)> {
    let delta = pointer - drag.from;
    if let Some(gizmo) = &drag.frame {
        let mut params =
            if drag.turn { json!({"rotation": gizmo.decal.rotation - delta.x * TURN_PER_POINT}) } else { json!({"offset": gizmo.slid_on_screen(delta)?}) };
        params["layer"] = json!(target.layer.0);
        return Some(("layer.decal.set", params));
    }

    if let Some(round) = drag.round.as_ref().filter(|_| !drag.turn) {
        return Some(("layer.fill.projection", round.slid(app, rect, drag.from, pointer)?));
    }

    let (rotation, offset) = placement(&drag.start)?;
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

    fn projector_offset(projection: &Projection) -> [f32; 3] {
        match projection {
            Projection::Triplanar { offset, .. } => *offset,
            other => panic!("tri-planar: {other:?}"),
        }
    }

    /// A drag as a mouse sends it: press, hold still a frame, then small moves one per frame until released
    fn mouse_drag(harness: &mut egui_kittest::Harness<'_, TextureCraftApp>, from: Pos2, to: Pos2) {
        let button = |pos: Pos2, pressed: bool| egui::Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE };
        harness.event(egui::Event::PointerMoved(from));
        harness.run_steps(1);
        harness.event(button(from, true));
        harness.run_steps(2);
        const STEPS: usize = 40;
        for step in 1..=STEPS {
            harness.event(egui::Event::PointerMoved(from.lerp(to, step as f32 / STEPS as f32)));
            harness.run_steps(1);
        }
        harness.event(button(to, false));
        harness.run_steps(2);
    }

    fn wheel(harness: &mut egui_kittest::Harness<'_, TextureCraftApp>, at: Pos2) {
        harness.event(egui::Event::PointerMoved(at));
        harness.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: vec2(0.0, 120.0),
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Unknown,
            modifiers: Modifiers::NONE,
        });
        harness.run_steps(4);
    }

    #[test]
    fn a_mouse_drag_on_the_model_carries_the_projector_under_the_pointer_and_the_wheel_still_zooms() {
        let (mut harness, layer) = fill_app("triplanar");
        harness.get_by_label("Move on model").click();
        harness.run_steps(2);
        let rect = harness.state().viewport_rect;
        let before = RoundGizmo::of(harness.state(), rect).unwrap();
        let steps = harness.state().session.history_labels().len();

        // On the model, away from the projector's handles in the middle
        let from = rect.center() + vec2(-100.0, 80.0);
        let moved = vec2(60.0, -20.0);
        mouse_drag(&mut harness, from, from + moved);
        let after = RoundGizmo::of(harness.state(), rect).unwrap();
        assert!((after.center - (before.center + moved)).length() < 2.0, "the projector follows the pointer: {:?} -> {:?}", before.center, after.center);
        let slid = projection(&harness, layer);
        assert_ne!(projector_offset(&slid), [0.0; 3], "the projector itself moved: {slid:?}");
        assert_eq!(uv_of(&slid), UvTransform::default(), "not only the image inside it");
        assert_eq!(harness.state().session.history_labels().len(), steps + 1, "one undo step per drag");

        let camera = harness.state().session.camera;
        wheel(&mut harness, from);
        assert_ne!(camera, harness.state().session.camera, "the wheel zooms the camera while Move on model is on");
        assert_eq!(projection(&harness, layer), slid, "and leaves the projection alone");
        assert_eq!(harness.state().session.history_labels().len(), steps + 1);
    }

    #[test]
    fn move_on_model_slides_a_uv_image_and_turns_the_projection_until_esc() {
        let (mut harness, layer) = fill_app("triplanar");
        harness.get_by_label("Move on model").click();
        harness.run_steps(2);
        assert_eq!(harness.state().moving_on_model, Some(MoveOnModel::layer(layer)));
        let middle = harness.state().viewport_rect.center() + vec2(-100.0, 80.0);

        drag(&mut harness, middle, middle + vec2(40.0, 0.0), Modifiers::SHIFT);
        let turned = projection(&harness, layer);
        assert!(uv_of(&turned).rotation > 0.0 && projector_offset(&turned) == [0.0; 3], "Shift+drag turns the image: {turned:?}");

        harness.key_press(egui::Key::Escape);
        harness.run_steps(2);
        assert_eq!(harness.state().moving_on_model, None, "Esc stops it");
        drag(&mut harness, middle, middle + vec2(60.0, 0.0), Modifiers::NONE);
        assert_eq!(projection(&harness, layer), turned, "then drags no longer move it");

        // A UV projection has no projector: the drag slides its image
        let (mut harness, layer) = fill_app("uv");
        harness.get_by_label("Move on model").click();
        harness.run_steps(2);
        mouse_drag(&mut harness, middle, middle + vec2(60.0, 0.0));
        let Projection::Uv { offset, .. } = projection(&harness, layer) else { panic!("still UV") };
        assert!(offset[0] < 0.0 && offset[1] == 0.0, "a drag right slides the image right: {offset:?}");
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
