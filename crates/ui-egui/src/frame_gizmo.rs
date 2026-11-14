//! The frame of the selected planar or warp projection in the 3D view: the centre square slides it, the axis arrows move it along its across, up and out directions, the ring turns it round its normal, a planar's corners scale it, the edge dots stretch one side, a warp's corners pull freely

use egui::{Color32, PointerButton, Pos2, Rect, Stroke, Ui, vec2};
use serde_json::{Value, json};
use texturecraft_engine::compose::axis_frame::placed_from_axis;
use texturecraft_engine::compose::frame_placement::folded;
use texturecraft_engine::compose::pattern::{decal_axes, decal_corners};
use texturecraft_engine::core::{Decal, EffectKind, Fill, LayerId, LayerKind, Projection};
use texturecraft_engine::geom::glam::{Vec2, Vec3};

use crate::TextureCraftApp;
use crate::theme::{AXIS_COLORS, Tokens};
use crate::viewport::project;

/// What a drag on the frame does
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum FrameGrab {
    /// The centre square: slide the frame across its own plane
    Slide,
    /// Along the frame's across (0), up (1) or out (2) direction
    Axis(usize),
    /// A planar's corner: scale it about its centre
    Scale,
    /// A planar's edge dot (top, right, bottom, left): stretch that side
    Side(usize),
    /// A warp's corner (top left, top right, bottom right, bottom left): pull it
    Corner(usize),
    /// The ring: turn it round its normal
    Turn,
}

impl FrameGrab {
    fn cursor(self, dragging: bool) -> egui::CursorIcon {
        match self {
            FrameGrab::Slide if dragging => egui::CursorIcon::Grabbing,
            FrameGrab::Slide => egui::CursorIcon::Grab,
            FrameGrab::Axis(_) | FrameGrab::Corner(_) | FrameGrab::Turn => egui::CursorIcon::Move,
            FrameGrab::Scale => egui::CursorIcon::ResizeNwSe,
            FrameGrab::Side(side) if side % 2 == 0 => egui::CursorIcon::ResizeVertical,
            FrameGrab::Side(_) => egui::CursorIcon::ResizeHorizontal,
        }
    }
}

/// A frame handle being dragged (kept in egui's memory between frames)
#[derive(Clone, Copy, Debug)]
struct FrameDrag {
    layer: LayerId,
    effect: Option<LayerId>,
    grab: FrameGrab,
    /// Where the drag started and the frame then
    from: Pos2,
    start: FrameGizmo,
    /// The drag made its undo step: later updates merge into it
    stepped: bool,
}

fn drag_id() -> egui::Id {
    egui::Id::unique("tc_frame_drag")
}

/// The selected fill's frame as the 3D view shows it
#[derive(Clone, Copy, Debug)]
pub(crate) struct FrameGizmo {
    pub layer: LayerId,
    /// The selected mask fill effect projecting from the frame, when it is in a mask
    pub effect: Option<LayerId>,
    /// The frame; an axis planar shows the frame it is placed as on its first drag
    pub decal: Decal,
    /// A warp's corners, across and up in frame sizes from its centre
    pub warp: Option<[[f32; 2]; 4]>,
    /// On screen: the corners (a warp's pulled ones), the edge dots and the centre
    pub corners: [Pos2; 4],
    pub sides: [Pos2; 4],
    pub center: Pos2,
    /// On screen, the tips of the across, up and out arrows (None when one points at the camera)
    pub arrows: [Option<Pos2>; 3],
    /// On screen, the turn ring round the centre in the frame's plane (None when it is edge on)
    pub ring: Option<[Pos2; TURN_RING_POINTS]>,
    /// In the world: the frame's centre, and its across, up and out directions
    origin: Vec3,
    axes: [Vec3; 3],
    /// Screen movement per world unit along each direction at the centre
    steps: [Option<egui::Vec2>; 3],
}

/// Screen length of the axis arrows, and their heads (points): long enough to grab each one apart from the others
pub(crate) const ARROW: f32 = 56.0;
const ARROW_HEAD: f32 = 14.0;
/// The shafts as the symmetry axis manipulator draws them: a coloured line over a dark halo
pub(crate) const SHAFT_WIDTH: f32 = 2.2;
pub(crate) const HALO: Color32 = Color32::from_black_alpha(128);
pub(crate) const HALO_EXTRA_WIDTH: f32 = 3.0;
/// The centre square's side, the corner and edge dot radii, a warp corner's radius (points)
pub(crate) const CENTER_SQUARE: f32 = 16.0;
const CORNER_DOT: f32 = 6.5;
const SIDE_DOT: f32 = 5.5;
const WARP_DOT: f32 = 8.0;
/// How near a handle or arrow a press grabs it (points)
pub(crate) const GRAB: f32 = 11.0;
/// Screen radius of the turn rings (frames and round projectors alike): past the arrow heads, so a ring is grabbed apart from the arrows
pub(crate) const TURN_RING: f32 = 84.0;
/// Points on a frame's turn ring, the first repeated at the end to close it
pub(crate) const TURN_RING_POINTS: usize = 49;
/// The smallest share of its size a dragged side keeps
const MIN_STRETCH: f32 = 0.02;

impl FrameGizmo {
    /// The frame of the selected fill layer, or of the selected mask fill effect, with a planar or warp projection
    pub(crate) fn of(app: &TextureCraftApp, rect: Rect) -> Option<FrameGizmo> {
        use texturecraft_engine::StackTarget;

        if app.ui.mode != crate::state::Mode::Painting {
            return None;
        }

        let layers = &app.session.doc.project;
        let selected = app.session.selected_layer()?;
        let (layer, effect, projection) = match app.session.target {
            StackTarget::Content => {
                let layer = layers.content_layer(selected)?;
                let LayerKind::Fill(Fill { projection, .. }) = &layer.kind else { return None };

                (layer.id, None, projection)
            }
            StackTarget::Effect(effect) => {
                // Only the selected layer's effective mask may supply a frame, including an instance's inherited mask
                let owner = layers.mask_owner(selected);
                let mask = layers.layer(owner).ok()?.mask.as_ref()?;
                let mask_effect = mask.effects.iter().find(|candidate| candidate.id == effect)?;
                let EffectKind::Fill { projection, .. } = &mask_effect.kind else { return None };

                (owner, Some(effect), projection)
            }
            StackTarget::Mask => return None,
        };
        let placed = placed_from_axis(projection, app.session.mesh.bounds());
        // Where the projection lands: a projector's own turn and scale moved into the frame, as the panel's 3D projection settings show it
        let landed = folded(placed.as_ref().unwrap_or(projection))?;
        let decal = *landed.frame()?;
        let warp = match projection {
            Projection::Warp { corners, .. } => Some(*corners),
            _ => None,
        };
        let (origin, across, up, out) = decal_axes(&decal)?;
        let at = |[x, y]: [f32; 2]| project(app, rect, origin + across * x * decal.size[0] + up * y * decal.size[1]);
        let corners = match warp {
            Some([a, b, c, d]) => [at(a)?, at(b)?, at(c)?, at(d)?],
            None => {
                let [a, b, c, d] = decal_corners(&decal)?.map(|corner| project(app, rect, corner));
                [a?, b?, c?, d?]
            }
        };
        let sides = [0, 1, 2, 3].map(|side| corners[side].lerp(corners[(side + 1) % 4], 0.5));
        let center = project(app, rect, origin)?;
        let reach = decal.size[0].max(decal.size[1]);
        let axes = [across, up, out];
        let steps = axes.map(|axis| Some((project(app, rect, origin + axis * reach)? - center) / reach.max(f32::MIN_POSITIVE)));
        let arrows = steps.map(|step| step.filter(|step| step.length() * reach > 1.0).map(|step| center + step.normalized() * ARROW));
        let ring = steps[0].zip(steps[1]).filter(|(a, b)| a.length() > f32::EPSILON && solve(*a, *b, egui::Vec2::X).is_some()).and_then(|(step, _)| {
            let radius = TURN_RING / step.length();
            let last = (TURN_RING_POINTS - 1) as f32;
            let points = (0..TURN_RING_POINTS).map(|index| {
                let (sin, cos) = (index as f32 / last * std::f32::consts::TAU).sin_cos();
                project(app, rect, origin + (across * cos + up * sin) * radius)
            });
            points.collect::<Option<Vec<_>>>()?.try_into().ok()
        });
        Some(FrameGizmo { layer, effect, decal, warp, corners, sides, center, arrows, ring, origin, axes, steps })
    }

    /// The handle under screen point `p`: the nearest one within reach, so a small frame's corners stay grabbable beside its arrows
    pub(crate) fn grab(&self, p: Pos2) -> Option<FrameGrab> {
        let mut handles = vec![(FrameGrab::Slide, center_distance(self.center, p))];
        handles.extend((0..3).filter_map(|axis| Some((FrameGrab::Axis(axis), arrow_distance(p, self.center, self.arrows[axis]?)))));
        let near = |points: [Pos2; 4], grab: fn(usize) -> FrameGrab| (0..4).map(move |index| (grab(index), points[index].distance(p)));
        match self.warp {
            // A mask effect's warp corners have no command to pull them
            Some(_) if self.effect.is_none() => handles.extend(near(self.corners, FrameGrab::Corner)),
            Some(_) => {}
            None => handles.extend(near(self.corners, |_| FrameGrab::Scale)),
        }
        handles.extend(near(self.sides, FrameGrab::Side));
        if let Some(ring) = &self.ring {
            handles.push((FrameGrab::Turn, ring.windows(2).map(|pair| distance_to_segment(p, pair[0], pair[1])).fold(f32::INFINITY, f32::min)));
        }
        nearest(handles)
    }

    /// The frame's offset after sliding it across its plane by screen movement `delta` (None when the frame is edge-on)
    pub(crate) fn slid_on_screen(&self, delta: egui::Vec2) -> Option<[f32; 2]> {
        let [across, up] = solve(self.steps[0]?, self.steps[1]?, delta)?;
        Some(self.slid(across, up))
    }

    /// Where screen point `p` meets the frame's plane, across and up in frame sizes from its centre
    fn on_plane(&self, app: &TextureCraftApp, rect: Rect, p: Pos2) -> Option<[f32; 2]> {
        let ndc = Vec2::new((p.x - rect.left()) / rect.width() * 2.0 - 1.0, 1.0 - (p.y - rect.top()) / rect.height() * 2.0);
        let ray = app.session.camera.ray(ndc, rect.width() / rect.height().max(1.0));
        let [across, up, out] = self.axes;
        let facing = ray.dir.dot(out);
        if facing.abs() < 1e-4 {
            return None;
        }
        let hit = ray.origin + ray.dir * ((self.origin - ray.origin).dot(out) / facing);
        let local = [(hit - self.origin).dot(across) / self.decal.size[0], (hit - self.origin).dot(up) / self.decal.size[1]];
        local.iter().all(|value| value.is_finite()).then_some(local)
    }

    /// The frame's offset after moving `across` and `up` world units along its turned directions, unturned as `layer.decal.set` stores it
    fn slid(&self, across: f32, up: f32) -> [f32; 2] {
        let (sin, cos) = self.decal.rotation.to_radians().sin_cos();
        [self.decal.offset[0] + across * cos - up * sin, self.decal.offset[1] + across * sin + up * cos]
    }

    /// The command and params a drag of `grab` from `from` to `to` runs
    fn edit(&self, app: &TextureCraftApp, rect: Rect, grab: FrameGrab, from: Pos2, to: Pos2) -> Option<(&'static str, Value)> {
        let delta = to - from;
        let center = self.center;
        let mut params = match grab {
            FrameGrab::Slide => json!({"offset": self.slid_on_screen(delta)?}),
            FrameGrab::Axis(axis) => {
                let step = (*self.steps.get(axis)?)?;
                let distance = delta.dot(step) / step.length_sq().max(f32::MIN_POSITIVE);
                match axis {
                    0 => json!({"offset": self.slid(distance, 0.0)}),
                    1 => json!({"offset": self.slid(0.0, distance)}),
                    _ => json!({"position": (self.origin + self.axes[2] * distance).to_array(), "normal": self.axes[2].to_array()}),
                }
            }
            FrameGrab::Scale => {
                let scale = (to - center).length() / (from - center).length().max(1.0);
                json!({"size": [self.decal.size[0] * scale, self.decal.size[1] * scale]})
            }
            FrameGrab::Side(side) => {
                let direction = *self.sides.get(side)? - center;
                let stretch = ((to - center).dot(direction) / (from - center).dot(direction).max(1.0)).max(MIN_STRETCH);
                let mut size = self.decal.size;
                size[1 - side % 2] *= stretch;
                json!({"size": size})
            }
            FrameGrab::Turn => {
                // The sweep round the centre on the frame's own plane (across turning towards up is a positive turn), so a tilted or off-centre frame follows the pointer too
                let angle = |p: Pos2| self.on_plane(app, rect, p).map(|[across, up]| (up * self.decal.size[1]).atan2(across * self.decal.size[0]).to_degrees());
                let swept = angle(to)? - angle(from)?;
                let swept = (swept + HALF_TURN).rem_euclid(2.0 * HALF_TURN) - HALF_TURN;
                json!({"rotation": self.decal.rotation + swept})
            }
            FrameGrab::Corner(corner) => {
                let mut corners = self.warp?;
                *corners.get_mut(corner)? = self.on_plane(app, rect, to)?;
                return Some(("layer.fill.projection", json!({"layer": self.layer.0, "warpCorners": corners})));
            }
        };
        params["layer"] = json!(self.layer.0);
        if let Some(effect) = self.effect {
            params["effect"] = json!(effect.0);
        }
        Some(("layer.decal.set", params))
    }
}

/// Half a turn in degrees: a ring drag's sweep is kept within one either way
const HALF_TURN: f32 = 180.0;

/// `delta` as a mix of screen steps `a` and `b` (None when they are parallel on screen)
fn solve(a: egui::Vec2, b: egui::Vec2, delta: egui::Vec2) -> Option<[f32; 2]> {
    let det = a.x * b.y - a.y * b.x;
    if det.abs() < 1e-6 {
        return None;
    }
    Some([(delta.x * b.y - delta.y * b.x) / det, (a.x * delta.y - a.y * delta.x) / det])
}

/// How far `p` is from the centre square (0 inside it)
pub(crate) fn center_distance(center: Pos2, p: Pos2) -> f32 {
    Rect::from_center_size(center, egui::Vec2::splat(CENTER_SQUARE)).distance_to_pos(p)
}

/// How far `p` is from the arrow from `center` to `tip`, its head included
pub(crate) fn arrow_distance(p: Pos2, center: Pos2, tip: Pos2) -> f32 {
    distance_to_segment(p, center, tip + (tip - center).normalized() * ARROW_HEAD)
}

/// The handle nearest its distance, if one is within `GRAB` (the first listed wins a tie)
pub(crate) fn nearest<T: Copy>(handles: impl IntoIterator<Item = (T, f32)>) -> Option<T> {
    let mut best: Option<(T, f32)> = None;
    for (handle, distance) in handles {
        if distance <= GRAB && best.is_none_or(|(_, nearest)| distance < nearest) {
            best = Some((handle, distance));
        }
    }
    best.map(|(handle, _)| handle)
}

pub(crate) fn distance_to_segment(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_sq().max(f32::MIN_POSITIVE)).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

/// Whether the pointer is over one of the selected frame's handles (the brush cursor hides there)
pub(crate) fn hovered(app: &TextureCraftApp, ui: &Ui, rect: Rect) -> bool {
    let dragging = ui.ctx().data(|data| data.get_temp::<FrameDrag>(drag_id())).is_some();
    dragging || ui.ctx().pointer_hover_pos().is_some_and(|hover| FrameGizmo::of(app, rect).is_some_and(|gizmo| gizmo.grab(hover).is_some()))
}

/// Whether screen point `at` is on one of the selected frame's handles
pub(crate) fn handle_at(app: &TextureCraftApp, rect: Rect, at: Pos2) -> bool {
    rect.contains(at) && FrameGizmo::of(app, rect).is_some_and(|gizmo| gizmo.grab(at).is_some())
}

/// Drags on the selected frame's handles (one undo step per drag, the projection updates live); returns whether a frame drag owns the pointer
pub(crate) fn input(app: &mut TextureCraftApp, ui: &Ui, response: &egui::Response, rect: Rect) -> bool {
    let ctx = ui.ctx();
    let gizmo = FrameGizmo::of(app, rect);
    let on_frame = |drag: &FrameDrag| gizmo.as_ref().is_some_and(|gizmo| gizmo.layer == drag.layer && gizmo.effect == drag.effect);
    let mut drag = ctx.data(|data| data.get_temp::<FrameDrag>(drag_id())).filter(on_frame);
    if response.drag_started_by(PointerButton::Primary)
        && let (Some(gizmo), Some(from)) = (&gizmo, ctx.input(|input| input.pointer.press_origin()))
        && let Some(grab) = gizmo.grab(from)
    {
        drag = Some(FrameDrag { layer: gizmo.layer, effect: gizmo.effect, grab, from, start: *gizmo, stepped: false });
    }

    let dragging = response.dragged_by(PointerButton::Primary) || response.drag_started_by(PointerButton::Primary);
    let Some(mut drag) = drag.filter(|_| dragging) else {
        ctx.data_mut(|data| data.remove::<FrameDrag>(drag_id()));
        if let (Some(gizmo), Some(hover)) = (&gizmo, response.hover_pos())
            && let Some(grab) = gizmo.grab(hover)
        {
            ctx.set_cursor_icon(grab.cursor(false));
        }
        return false;
    };

    ctx.set_cursor_icon(drag.grab.cursor(true));
    if let Some(pointer) = response.interact_pointer_pos().filter(|_| ctx.input(|input| input.pointer.delta() != egui::Vec2::ZERO))
        && let Some((command, mut params)) = drag.start.edit(app, rect, drag.grab, drag.from, pointer)
    {
        params["merge"] = json!(drag.stepped);
        // Off the plane or past the size limits: the frame waits where it is
        drag.stepped |= texturecraft_engine::execute(&mut app.session, command, &params).is_ok();
        // The panel drew before this edit: one more frame shows its fields and preview where the frame is now
        ctx.request_repaint();
    }
    ctx.data_mut(|data| data.insert_temp(drag_id(), drag));
    true
}

/// The selected frame over the 3D view: dashed outline, corner and edge dots, and the centre square with its axis arrows
pub(crate) fn overlay(app: &TextureCraftApp, ui: &Ui, rect: Rect) {
    let Some(gizmo) = FrameGizmo::of(app, rect) else { return };

    let tokens = Tokens::get(ui.ctx());
    let painter = ui.painter_at(rect);
    let active = ui.ctx().data(|data| data.get_temp::<FrameDrag>(drag_id())).map(|drag| drag.grab);
    let hovered = ui.ctx().pointer_hover_pos().filter(|hover| rect.contains(*hover)).and_then(|hover| gizmo.grab(hover));
    let lit = |grab: FrameGrab| active == Some(grab) || (active.is_none() && hovered == Some(grab));
    let shadow = Stroke::new(3.0, Color32::from_black_alpha(140));
    let dot_fill = |grab: FrameGrab| if lit(grab) { tokens.viewport_accent } else { Color32::WHITE };

    let mut outline = gizmo.corners.to_vec();
    outline.push(gizmo.corners[0]);
    for stroke in [shadow, Stroke::new(1.2, Color32::from_white_alpha(230))] {
        painter.add(egui::Shape::dashed_line(&outline, stroke, 6.0, 4.0));
    }
    if let Some(ring) = &gizmo.ring {
        let color = if lit(FrameGrab::Turn) { tokens.viewport_accent } else { AXIS_COLORS[2] };
        painter.add(egui::Shape::line(ring.to_vec(), Stroke::new(SHAFT_WIDTH + HALO_EXTRA_WIDTH, HALO)));
        painter.add(egui::Shape::line(ring.to_vec(), Stroke::new(SHAFT_WIDTH, color)));
    }
    match gizmo.warp {
        Some(_) => {
            for (index, corner) in gizmo.corners.into_iter().enumerate() {
                handle_dot(&painter, corner, WARP_DOT, dot_fill(FrameGrab::Corner(index)));
            }
        }
        None => {
            for corner in gizmo.corners {
                handle_dot(&painter, corner, CORNER_DOT, dot_fill(FrameGrab::Scale));
            }
        }
    }
    for (index, side) in gizmo.sides.into_iter().enumerate() {
        handle_dot(&painter, side, SIDE_DOT, dot_fill(FrameGrab::Side(index)));
    }

    let arrow_color = |axis: usize| if lit(FrameGrab::Axis(axis)) { tokens.viewport_accent } else { AXIS_COLORS[axis] };
    move_handles(&painter, gizmo.center, gizmo.arrows, arrow_color, dot_fill(FrameGrab::Slide));
}

/// The centre square (filled `square`) with its X/Y/Z arrows to `arrows`, coloured by `arrow_color`; shafts and heads over a dark halo like the symmetry manipulator's
pub(crate) fn move_handles(painter: &egui::Painter, center: Pos2, arrows: [Option<Pos2>; 3], arrow_color: impl Fn(usize) -> Color32, square: Color32) {
    let edge = Stroke::new(1.0, Color32::from_black_alpha(200));
    for (axis, tip) in arrows.into_iter().enumerate() {
        let Some(tip) = tip else { continue };

        let color = arrow_color(axis);
        let direction = (tip - center).normalized();
        let side = direction.rot90() * ARROW_HEAD * 0.45;
        let start = center + direction * CENTER_SQUARE * 0.5;
        painter.line_segment([start, tip], Stroke::new(SHAFT_WIDTH + HALO_EXTRA_WIDTH, HALO));
        painter.line_segment([start, tip], Stroke::new(SHAFT_WIDTH, color));
        painter.add(egui::Shape::convex_polygon(vec![tip + direction * ARROW_HEAD, tip + side, tip - side], color, edge));
    }
    let square_rect = Rect::from_center_size(center, vec2(CENTER_SQUARE, CENTER_SQUARE));
    painter.rect(square_rect, 2.0, square, edge, egui::StrokeKind::Middle);
}

/// A handle dot of `radius`: `fill` over a dark rim, so it reads on a light or dark model
pub(crate) fn handle_dot(painter: &egui::Painter, at: Pos2, radius: f32, fill: Color32) {
    painter.circle_filled(at, radius + 1.5, HALO);
    painter.circle(at, radius, fill, Stroke::new(1.0, Color32::from_black_alpha(200)));
}

#[cfg(test)]
pub(crate) mod tests {
    use egui::{Modifiers, PointerButton, Pos2, vec2};
    use egui_kittest::kittest::Queryable;
    use serde_json::json;
    use texturecraft_engine::Session;
    use texturecraft_engine::core::{LayerId, LayerKind, Projection};

    use super::*;

    /// The ball with a selected fill layer projecting `projection` (with `fields` of layer.fill.projection)
    pub(crate) fn fill_app(projection: &str) -> (egui_kittest::Harness<'static, TextureCraftApp>, LayerId) {
        fill_app_with(json!({"projection": projection}))
    }

    pub(crate) fn fill_app_with(fields: serde_json::Value) -> (egui_kittest::Harness<'static, TextureCraftApp>, LayerId) {
        fill_app_rendered(fields, false)
    }

    pub(crate) fn fill_app_rendered(mut fields: serde_json::Value, render: bool) -> (egui_kittest::Harness<'static, TextureCraftApp>, LayerId) {
        let session = Session::new_project("Frame", texturecraft_engine::geom::primitives::sphere(1.0, 48, 24), "builtin:sphere", None, 128).unwrap();
        let builder = egui_kittest::Harness::builder().with_size(vec2(1600.0, 960.0));
        let builder = if render { builder.wgpu() } else { builder };
        let mut harness = builder.build_eframe(|context| TextureCraftApp::new(context, session));
        harness.run_steps(4);
        let layer = harness.state_mut().run("layer.add.fill", json!({})).unwrap()["layer"].as_u64().unwrap();
        fields["layer"] = json!(layer);
        harness.state_mut().run("layer.fill.projection", fields).unwrap();
        harness.run_steps(2);
        (harness, LayerId(layer))
    }

    pub(crate) fn projection(harness: &egui_kittest::Harness<'_, TextureCraftApp>, layer: LayerId) -> Projection {
        match &harness.state().session.doc.project.layer(layer).unwrap().kind {
            LayerKind::Fill(fill) => fill.projection.clone(),
            _ => panic!("a fill layer"),
        }
    }

    /// A primary-button drag from `from` to `to` over a few frames, with `modifiers` held
    pub(crate) fn drag(harness: &mut egui_kittest::Harness<'_, TextureCraftApp>, from: Pos2, to: Pos2, modifiers: Modifiers) {
        harness.event_modifiers(egui::Event::PointerMoved(from), modifiers);
        harness.event_modifiers(egui::Event::PointerButton { pos: from, button: PointerButton::Primary, pressed: true, modifiers }, modifiers);
        harness.run_steps(2);
        for step in 1..=4 {
            harness.event_modifiers(egui::Event::PointerMoved(from.lerp(to, step as f32 / 4.0)), modifiers);
            harness.run_steps(1);
        }
        harness.event_modifiers(egui::Event::PointerButton { pos: to, button: PointerButton::Primary, pressed: false, modifiers }, modifiers);
        harness.run_steps(2);
    }

    /// A drag as a mouse sends it, one small move per frame; after each frame `check` looks at the app and (with `preview`) the 3D preview square must have re-rendered
    #[track_caller]
    pub(crate) fn drag_frames(
        harness: &mut egui_kittest::Harness<'_, TextureCraftApp>,
        layer: LayerId,
        from: Pos2,
        to: Pos2,
        modifiers: Modifiers,
        preview: bool,
        mut check: impl FnMut(&egui_kittest::Harness<'_, TextureCraftApp>),
    ) {
        harness.event_modifiers(egui::Event::PointerMoved(from), modifiers);
        harness.event_modifiers(egui::Event::PointerButton { pos: from, button: PointerButton::Primary, pressed: true, modifiers }, modifiers);
        harness.run_steps(2);
        let mut shown = crate::projection_preview::shown_key(&harness.ctx);
        let mut was = projection(harness, layer);
        let mut moved = 0;
        for step in 1..=6 {
            harness.event_modifiers(egui::Event::PointerMoved(from.lerp(to, step as f32 / 6.0)), modifiers);
            harness.step();
            let now = projection(harness, layer);
            // Until the pointer passes the drag threshold nothing moves
            if now == was {
                assert_eq!(moved, 0, "frame {step}: once started, every frame of the drag moves it");
                continue;
            }
            moved += 1;
            check(harness);
            let key = crate::projection_preview::shown_key(&harness.ctx);
            assert!(!preview || (key.is_some() && key != shown), "frame {step}: the preview square re-rendered for the new values");
            (shown, was) = (key, now);
        }
        assert!(moved >= 4, "the drag moved it on {moved} frames");
        harness.event_modifiers(egui::Event::PointerButton { pos: to, button: PointerButton::Primary, pressed: false, modifiers }, modifiers);
        harness.run_steps(2);
        check(harness);
    }

    /// The panel's X, Y and Z fields of a 3D projection settings row
    pub(crate) fn panel_xyz(harness: &egui_kittest::Harness<'_, TextureCraftApp>, row: &str) -> [f32; 3] {
        use egui_kittest::kittest::NodeT;
        ["X", "Y", "Z"].map(|tag| harness.get_by_label(&format!("{row} {tag}")).accesskit_node().numeric_value().unwrap() as f32)
    }

    fn gizmo(harness: &egui_kittest::Harness<'_, TextureCraftApp>) -> FrameGizmo {
        FrameGizmo::of(harness.state(), harness.state().viewport_rect).unwrap()
    }

    #[test]
    fn an_axis_planar_is_placed_on_its_first_drag_and_its_handles_move_size_and_stretch_it() {
        let _language = crate::i18n::TEST_LANGUAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        // Repeating three times across the mesh, the frame is smaller than the ball
        let (mut harness, layer) = fill_app_with(json!({"projection": "planar", "scale": 3}));
        let axis = gizmo(&harness);
        assert!(matches!(projection(&harness, layer), Projection::Planar { placed: None, .. }), "a new planar projects along its axis");
        assert!(axis.warp.is_none() && axis.arrows.iter().all(Option::is_some), "a planar frame with its three arrows: {axis:?}");

        drag(&mut harness, axis.center, axis.center + vec2(30.0, 0.0), Modifiers::NONE);
        let Projection::Planar { placed: Some(slid), .. } = projection(&harness, layer) else { panic!("placed on the first drag") };
        assert!(slid.offset[0] > 0.0 && slid.offset[1].abs() < slid.offset[0] * 0.2, "the centre square slides it right: {slid:?}");
        assert_eq!(slid.size, axis.decal.size, "sliding keeps its size");
        assert_eq!(harness.state().session.history_labels().last().map(String::as_str), Some("Edit decal"));
        let steps = harness.state().session.history_labels().len();

        let placed = gizmo(&harness);
        drag(&mut harness, placed.sides[1], placed.center + (placed.sides[1] - placed.center) * 0.5, Modifiers::NONE);
        let Projection::Planar { placed: Some(stretched), .. } = projection(&harness, layer) else { panic!("still placed") };
        assert!(stretched.size[0] < slid.size[0] * 0.75 && stretched.size[1] == slid.size[1], "the right edge dot narrows it only: {stretched:?}");

        let narrowed = gizmo(&harness);
        drag(&mut harness, narrowed.corners[2], narrowed.center + (narrowed.corners[2] - narrowed.center) * 1.5, Modifiers::NONE);
        let Projection::Planar { placed: Some(scaled), .. } = projection(&harness, layer) else { panic!("still placed") };
        assert!(scaled.size[1] > stretched.size[1] * 1.2, "a corner scales it: {scaled:?}");
        assert_eq!(harness.state().session.history_labels().len(), steps + 2, "one undo step per drag");
    }

    #[test]
    fn a_warp_corner_follows_the_pointer_on_the_frame_plane() {
        let _language = crate::i18n::TEST_LANGUAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let (mut harness, layer) = fill_app("warp");
        let warp = gizmo(&harness);
        assert_eq!(warp.warp, Some(Projection::FRAME_CORNERS));
        let to = warp.corners[1] + vec2(-40.0, 30.0);
        drag(&mut harness, warp.corners[1], to, Modifiers::NONE);
        let Projection::Warp { corners, frame, .. } = projection(&harness, layer) else { panic!("still a warp") };
        assert_eq!((frame, corners[0], corners[2], corners[3]), (warp.decal, [-0.5, 0.5], [0.5, -0.5], [-0.5, -0.5]), "only that corner moves");
        assert!(corners[1][0] < 0.45 && corners[1][1] < 0.45, "top right pulled in: {corners:?}");
        assert!(gizmo(&harness).corners[1].distance(to) < 1.5, "the corner sits under the pointer");
    }

    /// The panel's Rotation turns a frame and its Scale sizes it, so the gizmo does both: the ring turns a planar or warp frame round its normal, following the pointer round the centre; a warp's edge dots size its frame as a planar's do
    #[test]
    fn the_ring_turns_a_frame_and_a_warps_edge_dots_size_it() {
        let _language = crate::i18n::TEST_LANGUAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        for kind in ["warp", "planar"] {
            let (mut harness, _) = fill_app(kind);
            let start = gizmo(&harness);
            let ring = start.ring.expect("{kind}: a frame facing the camera shows its ring");
            let on_ring = *ring
                .iter()
                .filter(|at| start.grab(**at) == Some(FrameGrab::Turn))
                .max_by(|a, b| a.y.total_cmp(&b.y))
                .expect("a ring point clear of the other handles");
            let steps = harness.state().session.history_labels().len();
            let quarter = start.center + (on_ring - start.center).rot90();
            drag(&mut harness, on_ring, quarter, Modifiers::NONE);
            let turned = gizmo(&harness);
            let sweep = |from: egui::Vec2, to: egui::Vec2| (to.angle() - from.angle()).to_degrees();
            let pointer = sweep(on_ring - start.center, quarter - start.center);
            let corner = sweep(start.corners[1] - start.center, turned.corners[1] - turned.center);
            let off = (corner - pointer + 180.0).rem_euclid(360.0) - 180.0;
            assert!(off.abs() < 6.0, "{kind}: the frame turns a quarter with the pointer: corner {corner}°, pointer {pointer}°");
            assert!(turned.center.distance(start.center) < 1.0, "{kind}: round its centre");
            assert_eq!(harness.state().session.history_labels().len(), steps + 1, "{kind}: one undo step");
        }

        let (mut harness, layer) = fill_app("warp");
        let warp = gizmo(&harness);
        assert_eq!(warp.grab(warp.sides[1]), Some(FrameGrab::Side(1)));
        drag(&mut harness, warp.sides[1], warp.center + (warp.sides[1] - warp.center) * 0.5, Modifiers::NONE);
        let Projection::Warp { frame, corners, .. } = projection(&harness, layer) else { panic!("still a warp") };
        assert!(frame.size[0] < warp.decal.size[0] * 0.75 && (frame.size[1] - warp.decal.size[1]).abs() < 1e-4, "the right edge dot narrows it: {frame:?}");
        assert_eq!(corners, Projection::FRAME_CORNERS, "its corners stay where they are on the frame");
    }

    #[test]
    fn round_projections_have_no_frame() {
        let _language = crate::i18n::TEST_LANGUAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        for projection in ["uv", "triplanar", "spherical", "cylindrical"] {
            let (harness, _) = fill_app(projection);
            assert!(FrameGizmo::of(harness.state(), harness.state().viewport_rect).is_none(), "{projection}");
        }
    }

    /// The planar and warp frames over the ball, a handle hovered, Move on model lit, and the round projectors
    #[test]
    #[ignore = "requires an offscreen GPU adapter and TC_UI_SNAPSHOTS"]
    fn frame_gizmo_snapshots() {
        let _language = crate::i18n::TEST_LANGUAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(directory) = std::env::var_os("TC_UI_SNAPSHOTS").map(std::path::PathBuf::from) else { return };

        std::fs::create_dir_all(&directory).unwrap();
        let checker = |harness: &mut egui_kittest::Harness<'_, TextureCraftApp>, layer: LayerId| {
            let fields =
                json!({"layer": layer.0, "channel": "baseColor", "procedural": "Checker", "scale": 4, "low": [0.2, 0.2, 0.2], "high": [0.6, 0.6, 0.6]});
            harness.state_mut().run("layer.fill.set", fields).unwrap();
            harness.state_mut().run("layer.fill.projection", json!({"layer": layer.0, "shapeCrop": "cropped"})).unwrap();
        };
        let (mut harness, layer) = fill_app_rendered(json!({"projection": "planar", "scale": 3}), true);
        checker(&mut harness, layer);
        harness.run_steps(4);
        let planar = gizmo(&harness);
        harness.hover_at(planar.sides[1]);
        harness.run_steps(4);
        harness.render().unwrap().save(directory.join("move-planar-side-hover.png")).unwrap();

        // Move on model held down and dragged into the 3D view: the button stays lit and the frame follows
        let button = harness.get_by_label("Move on model").rect().center();
        harness.event(egui::Event::PointerMoved(button));
        harness.event(egui::Event::PointerButton { pos: button, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
        harness.run_steps(2);
        for step in 1..=6 {
            harness.event(egui::Event::PointerMoved(button + vec2(-40.0, 10.0) * step as f32));
            harness.run_steps(1);
        }
        harness.run_steps(3);
        harness.render().unwrap().save(directory.join("move-planar-held.png")).unwrap();
        harness.event(egui::Event::PointerButton {
            pos: button + vec2(-240.0, 60.0),
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        });
        harness.run_steps(2);

        let (mut harness, layer) = fill_app_rendered(json!({"projection": "warp"}), true);
        checker(&mut harness, layer);
        let pulled = json!([[-0.35, 0.3], [0.4, 0.42], [0.42, -0.4], [-0.3, -0.32]]);
        harness.state_mut().run("layer.fill.projection", json!({"layer": layer.0, "warpCorners": pulled, "projectionDepth": 1})).unwrap();
        harness.run_steps(4);
        harness.hover_at(gizmo(&harness).corners[1]);
        harness.run_steps(4);
        harness.render().unwrap().save(directory.join("move-warp-corner-hover.png")).unwrap();

        // Mid drag: the panel's 3D projection settings and the preview square hold the frame the gizmo is dragging
        let (mut harness, layer) = fill_app_rendered(json!({"projection": "planar", "scale": 3}), true);
        checker(&mut harness, layer);
        harness.run_steps(4);
        let held = gizmo(&harness);
        let (from, to) = (held.sides[0], held.center + (held.sides[0] - held.center) * 0.6 + vec2(30.0, 0.0));
        harness.event(egui::Event::PointerMoved(from));
        harness.event(egui::Event::PointerButton { pos: from, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
        harness.run_steps(2);
        for step in 1..=6 {
            harness.event(egui::Event::PointerMoved(from.lerp(to, step as f32 / 6.0)));
            harness.run_steps(1);
        }
        harness.run_steps(3);
        harness.render().unwrap().save(directory.join("sync-planar-mid-drag.png")).unwrap();
        harness.event(egui::Event::PointerButton { pos: to, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
        harness.run_steps(2);

        let (mut harness, layer) = fill_app_rendered(json!({"projection": "spherical"}), true);
        checker(&mut harness, layer);
        harness.run_steps(4);
        let round = crate::round_gizmo::RoundGizmo::of(harness.state(), harness.state().viewport_rect).unwrap();
        let dot = round.sizes[0].at;
        let to = round.center + (dot - round.center) * 0.6;
        harness.event(egui::Event::PointerMoved(dot));
        harness.event(egui::Event::PointerButton { pos: dot, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
        harness.run_steps(2);
        for step in 1..=6 {
            harness.event(egui::Event::PointerMoved(dot.lerp(to, step as f32 / 6.0)));
            harness.run_steps(1);
        }
        harness.run_steps(3);
        harness.render().unwrap().save(directory.join("sync-sphere-mid-drag.png")).unwrap();

        // The round projections with their fill selected: the shape round it, a ring hovered (a sphere: its one dot; a cylinder: its height arrow)
        for kind in ["triplanar", "spherical", "cylindrical"] {
            let (mut harness, layer) = fill_app_rendered(json!({"projection": kind}), true);
            checker(&mut harness, layer);
            harness.run_steps(4);
            let round = crate::round_gizmo::RoundGizmo::of(harness.state(), harness.state().viewport_rect).unwrap();
            let hover = if kind == "triplanar" { round.rings[2][crate::round_gizmo::CIRCLE_POINTS / 8] } else { round.sizes[0].at };
            harness.hover_at(hover);
            harness.run_steps(4);
            harness.render().unwrap().save(directory.join(format!("move-{kind}.png"))).unwrap();
        }
    }

    /// The panel's 3D projection settings show the frame the gizmo drags (its centre, turn and size), and the preview square follows, every frame of every drag: resize, stretch, slide, turn, a warp's slide and corners; one undo step per drag
    #[test]
    fn the_panel_and_its_preview_follow_every_frame_of_a_frame_drag() {
        let _language = crate::i18n::TEST_LANGUAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        use texturecraft_engine::compose::frame_placement::placement;

        let check = |layer: LayerId| {
            move |harness: &egui_kittest::Harness<'_, TextureCraftApp>| {
                let Some(shown) = placement(&projection(harness, layer), harness.state().session.mesh.bounds()) else { panic!("placed on its first drag") };
                for (row, value) in [("Offset", shown.offset), ("Rotation", shown.rotation), ("Scale", shown.scale)] {
                    let field = panel_xyz(harness, row);
                    for axis in 0..3 {
                        assert!((field[axis] - value[axis]).abs() < 1e-3, "the panel's {row} shows the frame's {value:?}, not {field:?}");
                    }
                }
            }
        };

        let (mut harness, layer) = fill_app("planar");
        let steps = harness.state().session.history_labels().len();
        let center = gizmo(&harness).center;
        drag_frames(&mut harness, layer, center, center + vec2(30.0, -10.0), Modifiers::NONE, true, check(layer));
        let slid = placement(&projection(&harness, layer), harness.state().session.mesh.bounds()).unwrap();
        // The top edge dot and a corner inside the view (the frame starts bigger than the model)
        let side = gizmo(&harness);
        drag_frames(&mut harness, layer, side.sides[0], side.center + (side.sides[0] - side.center) * 0.5, Modifiers::NONE, true, check(layer));
        let narrowed = placement(&projection(&harness, layer), harness.state().session.mesh.bounds()).unwrap();
        assert!(narrowed.scale[1] < slid.scale[1] * 0.75 && (narrowed.scale[0] - slid.scale[0]).abs() < 1e-4, "{slid:?} -> {narrowed:?}");
        let corner = gizmo(&harness);
        let view = harness.state().viewport_rect.shrink(10.0);
        let inside = *corner.corners.iter().find(|at| view.contains(**at)).expect("a corner in the view");
        drag_frames(&mut harness, layer, inside, corner.center + (inside - corner.center) * 0.6, Modifiers::NONE, true, check(layer));
        let scaled = placement(&projection(&harness, layer), harness.state().session.mesh.bounds()).unwrap();
        assert!(scaled.scale[1] < narrowed.scale[1] * 0.8 && scaled.scale[0] < narrowed.scale[0] * 0.8, "a corner scales it: {scaled:?}");

        // Move on model held with Shift turns the frame round its normal
        let before = placement(&projection(&harness, layer), harness.state().session.mesh.bounds()).unwrap();
        let button = harness.get_by_label("Move on model").rect().center();
        drag_frames(&mut harness, layer, button, button + vec2(-60.0, 0.0), Modifiers::SHIFT, true, check(layer));
        let turned = placement(&projection(&harness, layer), harness.state().session.mesh.bounds()).unwrap();
        assert!((turned.rotation[2] - before.rotation[2]).abs() > 10.0, "turned: {before:?} -> {turned:?}");
        assert_eq!(harness.state().session.history_labels().len(), steps + 4, "one undo step per drag");

        let (mut harness, layer) = fill_app("warp");
        let warp = gizmo(&harness);
        drag_frames(&mut harness, layer, warp.center, warp.center + vec2(30.0, 25.0), Modifiers::NONE, false, check(layer));
        let warp = gizmo(&harness);
        drag_frames(&mut harness, layer, warp.corners[0], warp.corners[0] + vec2(-25.0, -15.0), Modifiers::NONE, false, check(layer));
        let Projection::Warp { corners, .. } = projection(&harness, layer) else { panic!("a warp") };
        assert_ne!(corners[0], Projection::FRAME_CORNERS[0], "the corner was pulled");
    }
}
