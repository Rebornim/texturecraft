//! Move on model: hold the button and drag to slide the selected fill's projection, with Shift or the right button held to turn it, and turn the wheel to scale it. Letting go ends it, as one undo step; the 3D view keeps its own drags and wheel

use egui::{CursorIcon, Pos2, Rect};
use egui_phosphor::regular as ic;
use serde_json::{Value, json};
use texturecraft_engine::core::{EffectKind, LayerId, LayerKind, Projection};

use crate::TextureCraftApp;
use crate::frame_gizmo::FrameGizmo;
use crate::i18n::tr;
use crate::move_hold::MoveHold;
use crate::property_sheet::PropertySheet;
use crate::round_gizmo::RoundGizmo;

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

fn hold_id() -> egui::Id {
    egui::Id::unique("tc_move_on_model_hold")
}

/// Image repeats a drag across the view's height slides by
const SLIDE_PER_HEIGHT: f32 = 1.0;
/// Degrees a turning drag turns per point
const TURN_PER_POINT: f32 = 0.5;
/// Growth per wheel point (exponential)
const SCALE_PER_WHEEL_POINT: f32 = 0.0015;
/// Seconds the cursor shows a wheel step's zoom in or out
const WHEEL_CURSOR_SECONDS: f64 = 0.3;

/// The Move on model button of `target`'s projection rows; while it is held down it moves the projection
pub(crate) fn button(app: &mut TextureCraftApp, sheet: &mut PropertySheet<'_>, target: MoveOnModel) {
    let response = sheet.hold_button(ic::HAND, tr("Move on model"), app.moving_on_model == Some(target));
    let response = sheet.tip(response, tr("Hold and drag to slide the projection; Shift or the right button turns it, the wheel scales it"));
    let ctx = sheet.ui.ctx().clone();
    hold(app, &ctx, &response, target);
}

/// Whether a Move on model hold is under way, or let go this frame (that release is no click in the view); the 3D view then keeps its camera, brush and gizmos still. Ends a hold whose button went away
pub(crate) fn holding(app: &mut TextureCraftApp, ctx: &egui::Context) -> bool {
    let held = app.moving_on_model.is_some();
    if held && !ctx.input(|input| input.pointer.primary_down()) {
        end(app, ctx);
    }
    held
}

fn end(app: &mut TextureCraftApp, ctx: &egui::Context) {
    app.moving_on_model = None;
    ctx.data_mut(|data| data.remove::<MoveHold>(hold_id()));
}

/// A press on the button starts the hold; each frame until the left button lets go, pointer moves and wheel steps edit the projection
fn hold(app: &mut TextureCraftApp, ctx: &egui::Context, button: &egui::Response, target: MoveOnModel) {
    let (primary, turn, pointer, now) =
        ctx.input(|input| (input.pointer.primary_down(), input.modifiers.shift || input.pointer.secondary_down(), input.pointer.latest_pos(), input.time));
    let pass = ctx.cumulative_pass_nr();
    let mut held = ctx.data(|data| data.get_temp::<MoveHold>(hold_id())).filter(|held| held.target == target);
    if held.is_none()
        && primary
        && button.is_pointer_button_down_on()
        && let Some(at) = pointer
    {
        held = Some(MoveHold { target, last: at, stepped: false, pass: pass.wrapping_sub(1), wheeled: None });
    }
    let Some(mut held) = held.filter(|_| primary && target.projection(app).is_some()) else {
        if app.moving_on_model == Some(target) {
            end(app, ctx);
        }
        if button.hovered() {
            ctx.set_cursor_icon(CursorIcon::Grab);
        }
        return;
    };

    app.moving_on_model = Some(target);
    if held.pass == pass {
        return;
    }
    held.pass = pass;

    let rect = app.viewport_rect;
    if let Some(at) = pointer.filter(|at| *at != held.last) {
        if let Some((command, params)) = dragged(app, target, held.last, at, turn, rect) {
            step(app, &mut held, command, &params);
        }
        held.last = at;
    }
    // The wheel scales the projection instead of scrolling the panel or zooming the camera while the button is held
    let wheel = ctx.input_mut(|input| std::mem::take(&mut input.smooth_scroll_delta).y);
    if wheel != 0.0
        && let Some((command, params)) = resized(app, target, (SCALE_PER_WHEEL_POINT * wheel).exp(), rect)
    {
        step(app, &mut held, command, &params);
        held.wheeled = Some((now, wheel > 0.0));
    }

    ctx.set_cursor_icon(match held.wheeled {
        Some((at, grew)) if now - at < WHEEL_CURSOR_SECONDS => {
            if grew {
                CursorIcon::ZoomIn
            } else {
                CursorIcon::ZoomOut
            }
        }
        _ if turn => CursorIcon::ResizeHorizontal,
        _ => CursorIcon::Move,
    });
    ctx.data_mut(|data| data.insert_temp(hold_id(), held));
    // The panel drew before this edit: one more frame shows its fields and preview where the projection is now
    ctx.request_repaint();
}

/// Runs one edit of the hold; every edit after the first joins the first's undo step
fn step(app: &mut TextureCraftApp, held: &mut MoveHold, command: &str, params: &Value) {
    let mut params = params.clone();
    params["merge"] = json!(held.stepped);
    held.stepped |= texturecraft_engine::execute(&mut app.session, command, &params).is_ok();
}

/// The selected planar or warp frame of a fill layer, which moves by its frame
fn frame_of(app: &TextureCraftApp, target: MoveOnModel, rect: Rect) -> Option<FrameGizmo> {
    FrameGizmo::of(app, rect).filter(|gizmo| target.effect.is_none() && gizmo.layer == target.layer && gizmo.effect.is_none())
}

/// A projection's UV tiling, rotation and offset (None for Fill (Match per UV Tile))
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

/// The edit a pointer move from `from` to `to` makes: a frame or projector slides by it, a UV image slides; with `turn` it turns instead
fn dragged(app: &TextureCraftApp, target: MoveOnModel, from: Pos2, to: Pos2, turn: bool, rect: Rect) -> Option<(&'static str, Value)> {
    let delta = to - from;
    if let Some(gizmo) = frame_of(app, target, rect) {
        let mut params =
            if turn { json!({"rotation": gizmo.decal.rotation - delta.x * TURN_PER_POINT}) } else { json!({"offset": gizmo.slid_on_screen(delta)?}) };
        params["layer"] = json!(target.layer.0);
        return Some(("layer.decal.set", params));
    }

    if !turn && let Some(round) = RoundGizmo::of(app, rect).filter(|round| round.target == target) {
        return Some(("layer.fill.projection", round.slid(app, rect, from, to)?));
    }

    let (_, rotation, offset) = placement(&target.projection(app)?)?;
    let slide = delta * SLIDE_PER_HEIGHT / rect.height().max(1.0);
    let offset = [offset[0] - slide.x, offset[1] - slide.y];
    let mut params = match (turn, target.projection(app)?) {
        (true, _) => json!({"rotation": rotation + delta.x * TURN_PER_POINT}),
        (false, Projection::Uv { .. }) => json!({"offset": offset}),
        (false, _) => json!({"uvOffset": offset}),
    };
    merge_target(&mut params, target);
    Some(("layer.fill.projection", params))
}

/// The edit a wheel step growing the projection by `growth` makes: a frame grows, other projections' images repeat less
fn resized(app: &TextureCraftApp, target: MoveOnModel, growth: f32, rect: Rect) -> Option<(&'static str, Value)> {
    if let Some(gizmo) = frame_of(app, target, rect) {
        let [width, height] = gizmo.decal.size;
        return Some(("layer.decal.set", json!({"layer": target.layer.0, "size": [width * growth, height * growth]})));
    }

    let (tiling, _, _) = placement(&target.projection(app)?)?;
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
    use egui::{Modifiers, PointerButton, vec2};
    use egui_kittest::kittest::Queryable;
    use texturecraft_engine::core::{Projection, UvTransform};

    use super::*;
    use crate::frame_gizmo::tests::{drag, fill_app, projection};

    type Harness = egui_kittest::Harness<'static, TextureCraftApp>;

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

    fn button_event(harness: &mut Harness, at: Pos2, button: PointerButton, pressed: bool, modifiers: Modifiers) {
        harness.event_modifiers(egui::Event::PointerButton { pos: at, button, pressed, modifiers }, modifiers);
        harness.run_steps(2);
    }

    /// Presses the left button on Move on model and holds it; returns where
    fn press_the_button(harness: &mut Harness) -> Pos2 {
        let at = harness.get_by_label("Move on model").rect().center();
        harness.event(egui::Event::PointerMoved(at));
        harness.run_steps(1);
        button_event(harness, at, PointerButton::Primary, true, Modifiers::NONE);
        at
    }

    /// Moves the pointer from `from` to `to` as a mouse sends it, one small move per frame
    fn move_pointer(harness: &mut Harness, from: Pos2, to: Pos2, modifiers: Modifiers) {
        const STEPS: usize = 30;
        for step in 1..=STEPS {
            harness.event_modifiers(egui::Event::PointerMoved(from.lerp(to, step as f32 / STEPS as f32)), modifiers);
            harness.run_steps(1);
        }
    }

    fn wheel(harness: &mut Harness, at: Pos2) {
        harness.event(egui::Event::PointerMoved(at));
        harness.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: vec2(0.0, 120.0),
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Unknown,
            modifiers: Modifiers::NONE,
        });
        harness.run_steps(6);
    }

    /// Holding the button and dragging (the pointer runs off the button into the 3D view) carries the projector with the pointer; letting go ends it
    #[test]
    fn holding_the_button_and_dragging_carries_the_projector_as_one_undo_step() {
        let _language = crate::i18n::TEST_LANGUAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let (mut harness, layer) = fill_app("triplanar");
        let rect = harness.state().viewport_rect;
        let before = RoundGizmo::of(harness.state(), rect).unwrap();
        let (steps, camera) = (harness.state().session.history_labels().len(), harness.state().session.camera);

        let from = press_the_button(&mut harness);
        assert_eq!(harness.state().moving_on_model, Some(MoveOnModel::layer(layer)), "a press starts the hold");
        let moved = vec2(-300.0, -20.0);
        move_pointer(&mut harness, from, from + moved, Modifiers::NONE);
        let after = RoundGizmo::of(harness.state(), rect).unwrap();
        assert!((after.center - (before.center + moved)).length() < 2.0, "the projector follows the pointer: {:?} -> {:?}", before.center, after.center);
        let slid = projection(&harness, layer);
        assert_ne!(projector_offset(&slid), [0.0; 3], "the projector itself moved: {slid:?}");
        assert_eq!(uv_of(&slid), UvTransform::default(), "not only the image inside it");

        button_event(&mut harness, from + moved, PointerButton::Primary, false, Modifiers::NONE);
        assert_eq!(harness.state().moving_on_model, None, "letting go ends it");
        assert_eq!(harness.state().session.history_labels().len(), steps + 1, "one undo step per hold");
        assert_eq!(harness.state().session.camera, camera, "the camera kept still");

        // The 3D view has its drags back: a drag on the model off the projector's handles moves nothing
        let off = rect.center() + vec2(-100.0, 80.0);
        drag(&mut harness, off, off + vec2(60.0, 0.0), Modifiers::NONE);
        assert_eq!(projection(&harness, layer), slid, "no mode is left on");
    }

    /// While held, Shift or the right button turns the image and the wheel scales it, all in the hold's one undo step; the camera neither orbits nor zooms. Released, the wheel zooms again
    #[test]
    fn shift_or_the_right_button_turns_and_the_wheel_scales_while_held() {
        let _language = crate::i18n::TEST_LANGUAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let (mut harness, layer) = fill_app("triplanar");
        let rect = harness.state().viewport_rect;
        let (steps, camera) = (harness.state().session.history_labels().len(), harness.state().session.camera);
        let from = press_the_button(&mut harness);

        let turning = from + vec2(-40.0, 0.0);
        move_pointer(&mut harness, from, turning, Modifiers::SHIFT);
        let shifted = uv_of(&projection(&harness, layer));
        assert!(shifted.rotation > 0.0, "Shift turns the image: {shifted:?}");
        assert_eq!(projector_offset(&projection(&harness, layer)), [0.0; 3], "and does not slide it");

        // The right button pressed over the 3D view while the left still holds the button
        let over_view = rect.center() + vec2(-100.0, 80.0);
        move_pointer(&mut harness, turning, over_view, Modifiers::NONE);
        let slid = projection(&harness, layer);
        button_event(&mut harness, over_view, PointerButton::Secondary, true, Modifiers::NONE);
        move_pointer(&mut harness, over_view, over_view + vec2(-40.0, 0.0), Modifiers::NONE);
        let turned = projection(&harness, layer);
        assert!(uv_of(&turned).rotation < shifted.rotation, "the right button turns it: {shifted:?} -> {turned:?}");
        assert_eq!(projector_offset(&turned), projector_offset(&slid), "without sliding");
        button_event(&mut harness, over_view + vec2(-40.0, 0.0), PointerButton::Secondary, false, Modifiers::NONE);

        wheel(&mut harness, over_view);
        let scaled = uv_of(&projection(&harness, layer));
        assert!(scaled.tiling[0] < uv_of(&turned).tiling[0] && scaled.tiling[0] == scaled.tiling[1], "the wheel up makes the image bigger: {scaled:?}");
        assert_eq!(harness.state().session.camera, camera, "the camera neither orbited nor zoomed");
        assert!(harness.state().properties_popup.is_none(), "the right click opened no Properties popup");

        button_event(&mut harness, over_view, PointerButton::Primary, false, Modifiers::NONE);
        assert_eq!(harness.state().session.history_labels().len(), steps + 1, "the whole hold is one undo step");
        wheel(&mut harness, over_view);
        assert_ne!(harness.state().session.camera, camera, "let go, the wheel zooms the camera");
        assert_eq!(uv_of(&projection(&harness, layer)), scaled, "and leaves the projection alone");
    }

    #[test]
    fn a_hold_slides_a_uv_image_and_a_frame_across_its_plane() {
        let _language = crate::i18n::TEST_LANGUAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let (mut harness, layer) = fill_app("uv");
        let from = press_the_button(&mut harness);
        wheel(&mut harness, from);
        assert_eq!(harness.get_by_label("Move on model").rect().center(), from, "a wheel step over the panel while held scales, it does not scroll the panel");
        let Projection::Uv { scale, .. } = projection(&harness, layer) else { panic!("still UV") };
        assert!(scale[0] < 1.0, "the wheel up makes the image bigger: {scale:?}");
        move_pointer(&mut harness, from, from + vec2(-60.0, 0.0), Modifiers::NONE);
        button_event(&mut harness, from + vec2(-60.0, 0.0), PointerButton::Primary, false, Modifiers::NONE);
        let Projection::Uv { offset, .. } = projection(&harness, layer) else { panic!("still UV") };
        assert!(offset[0] > 0.0 && offset[1] == 0.0, "a drag left slides the image left: {offset:?}");

        let (mut harness, layer) = fill_app("warp");
        let from = press_the_button(&mut harness);
        move_pointer(&mut harness, from, from + vec2(0.0, -40.0), Modifiers::NONE);
        button_event(&mut harness, from + vec2(0.0, -40.0), PointerButton::Primary, false, Modifiers::NONE);
        let Projection::Warp { frame, .. } = projection(&harness, layer) else { panic!("still a warp") };
        assert!(frame.offset[1] > 0.0 && frame.offset[0].abs() < frame.offset[1] * 0.2, "a drag up slides the frame up: {frame:?}");
    }
}
