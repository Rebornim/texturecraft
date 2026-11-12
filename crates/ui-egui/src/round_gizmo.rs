//! While Move on model is on, a tri-planar, spherical or cylindrical projection shows its projector round the whole model: a dashed box, sphere or cylinder with the centre square, X/Y/Z arrows and turn rings, and dots that resize it

use egui::{Color32, PointerButton, Pos2, Rect, Stroke, Ui};
use serde_json::{Value, json};
use texturecraft_engine::core::Projection;
use texturecraft_engine::geom::glam::{EulerRot, Mat3, Quat, Vec2, Vec3};

use crate::TextureCraftApp;
use crate::frame_gizmo::{ARROW, CENTER_SQUARE, GRAB, distance_to_segment, move_handles};
use crate::move_on_model::MoveOnModel;
use crate::theme::{AXIS_COLORS, Tokens};
use crate::viewport::project;

/// What a drag on the projector does
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum RoundGrab {
    /// The centre square: slide it across the view
    Slide,
    /// Along the projector's X (0), Y (1) or Z (2) direction
    Axis(usize),
    /// Turn it round its X, Y or Z direction
    Ring(usize),
    /// A size dot: scale it along the directions marked
    Size([bool; 3]),
}

impl RoundGrab {
    fn cursor(self, dragging: bool) -> egui::CursorIcon {
        match self {
            RoundGrab::Slide if dragging => egui::CursorIcon::Grabbing,
            RoundGrab::Slide => egui::CursorIcon::Grab,
            RoundGrab::Axis(_) | RoundGrab::Ring(_) => egui::CursorIcon::Move,
            RoundGrab::Size(_) => egui::CursorIcon::ResizeHorizontal,
        }
    }
}

/// Which shape the projector shows
#[derive(Clone, Copy, Debug, PartialEq)]
enum Shape {
    Box,
    Sphere,
    /// Round the axis 0, 1 or 2
    Cylinder(usize),
}

/// The projector of the fill Move on model moves, as the 3D view shows it
#[derive(Clone, Debug)]
pub(crate) struct RoundGizmo {
    target: MoveOnModel,
    /// Mesh centre and radius: the projector reads positions around it in radii
    mesh_center: Vec3,
    radius: f32,
    /// The projector's turn and per-axis scale
    turn: Mat3,
    stretch: Vec3,
    /// The projection's centre in the projector's units, and what offset puts it there (offset = -local × this)
    local: Vec3,
    offset_per_local: f32,
    /// On screen: the centre, the arrow tips, the turn rings, the dashed outline, the size dots and the poles (shown only)
    pub center: Pos2,
    pub arrows: [Option<Pos2>; 3],
    pub rings: [Vec<Pos2>; 3],
    outline: Vec<Vec<Pos2>>,
    pub dots: Vec<(Pos2, [bool; 3])>,
    poles: Vec<Pos2>,
    /// Screen movement per world unit along each projector direction at the centre
    steps: [Option<egui::Vec2>; 3],
}

/// How far round the model each shape reaches, in the mesh's half sizes: the box, the sphere, the cylinder's radius and half height
const BOX_REACH: f32 = 1.08;
const SPHERE_REACH: f32 = 1.2;
const CYLINDER_REACH: f32 = 1.1;
const CYLINDER_HEIGHT: f32 = 1.15;
/// Screen radius of the turn rings, their width and how near one a press grabs it (points)
const RING: f32 = 30.0;
const RING_WIDTH: f32 = 2.0;
const RING_GRAB: f32 = 5.0;
/// The size dots' and the poles' radii (points)
const SIZE_DOT: f32 = 5.0;
const POLE_DOT: f32 = 3.0;
/// Points on a drawn circle
pub(crate) const CIRCLE_POINTS: usize = 48;
/// The projector's scale limits (as `layer.fill.projection` takes them)
const MIN_STRETCH: f32 = 0.001;
const MAX_STRETCH: f32 = 1000.0;

/// A projector handle being dragged (kept in egui's memory between frames)
#[derive(Clone, Debug)]
struct RoundDrag {
    grab: RoundGrab,
    from: Pos2,
    start: RoundGizmo,
    /// The drag made its undo step: later updates merge into it
    stepped: bool,
}

fn drag_id() -> egui::Id {
    egui::Id::unique("tc_round_drag")
}

impl RoundGizmo {
    /// The projector of the tri-planar, spherical or cylindrical fill Move on model is moving
    pub(crate) fn of(app: &TextureCraftApp, rect: Rect) -> Option<RoundGizmo> {
        if app.ui.mode != crate::state::Mode::Painting {
            return None;
        }
        let target = app.moving_on_model.filter(|target| app.session.selected_layer() == Some(target.layer))?;
        let (shape, offset, projector, offset_per_local) = match target.projection(app)? {
            Projection::Triplanar { scale, offset, projector, .. } => (Shape::Box, offset, projector, scale),
            Projection::Spherical { offset, projector, .. } => (Shape::Sphere, offset, projector, 1.0),
            Projection::Cylindrical { axis, offset, projector, .. } => (Shape::Cylinder(usize::from(axis.min(2))), offset, projector, 1.0),
            _ => return None,
        };
        let (lo, hi) = app.session.mesh.bounds();
        let mesh_center = (lo + hi) * 0.5;
        let radius = ((hi - lo).length() * 0.5).max(1e-4);
        let [x, y, z] = projector.rotation.map(f32::to_radians);
        let turn = Mat3::from_euler(EulerRot::ZYX, z, y, x);
        let stretch = Vec3::from(projector.scale).max(Vec3::splat(MIN_STRETCH));
        let offset_per_local = if offset_per_local.abs() < 1e-6 { 1e-6 } else { offset_per_local };
        let local = -Vec3::from(offset) / offset_per_local;
        let mut gizmo = RoundGizmo {
            target,
            mesh_center,
            radius,
            turn,
            stretch,
            local,
            offset_per_local,
            center: Pos2::ZERO,
            arrows: [None; 3],
            rings: Default::default(),
            outline: Vec::new(),
            dots: Vec::new(),
            poles: Vec::new(),
            steps: [None; 3],
        };
        let origin = gizmo.world(local);
        if !origin.is_finite() {
            return None;
        }
        gizmo.center = project(app, rect, origin)?;
        let world = |point: Vec3| mesh_center + turn * (stretch * point) * radius;
        let at = |point: Vec3| project(app, rect, world(point));

        // The arrows run along the projector's turned directions; the rings turn round them
        let directions = gizmo.directions();
        let reach = radius * 0.5;
        gizmo.steps = directions.map(|direction| Some((project(app, rect, origin + direction * reach)? - gizmo.center) / reach));
        gizmo.arrows = gizmo.steps.map(|step| step.filter(|step| step.length() * reach > 1.0).map(|step| gizmo.center + step.normalized() * ARROW));
        let forward = app.session.camera.forward();
        let right = forward.cross(Vec3::Y).try_normalize().unwrap_or(Vec3::X);
        let pixels = (project(app, rect, origin + right * reach)? - gizmo.center).length() / reach;
        let ring = RING / pixels.max(f32::MIN_POSITIVE);
        gizmo.rings = directions.map(|direction| {
            let (u, v) = direction.any_orthonormal_pair();
            circle(|t| project(app, rect, origin + (u * t.x + v * t.y) * ring))
        });

        // The shape, sized round the mesh in the projector's units; a size dot past the view's edge waits at the edge, where it can be grabbed
        let view = rect.shrink(SIZE_DOT);
        let keep_in_view = |point: Pos2| point.clamp(view.min, view.max);
        let half = (hi - lo) * 0.5 / radius;
        // The view direction in the projector's units: what faces the camera there
        let facing = (turn.transpose() * forward / stretch).try_normalize().unwrap_or(Vec3::Z);
        match shape {
            Shape::Box => {
                let size = half * BOX_REACH;
                let corner = |index: usize| local + size * Vec3::new(sign(index, 0), sign(index, 1), sign(index, 2));
                for a in 0..8 {
                    for bit in [1, 2, 4] {
                        let b = a | bit;
                        if b != a
                            && let (Some(from), Some(to)) = (at(corner(a)), at(corner(b)))
                        {
                            gizmo.outline.push(vec![from, to]);
                        }
                    }
                }
            }
            Shape::Sphere => {
                let r = half.max_element() * SPHERE_REACH;
                let (u, v) = facing.any_orthonormal_pair();
                let silhouette = circle(|t| at(local + (u * t.x + v * t.y) * r));
                if let Some(side) = silhouette.iter().copied().max_by(|a, b| a.x.total_cmp(&b.x)) {
                    gizmo.dots.push((keep_in_view(side), [true; 3]));
                }
                gizmo.outline.push(silhouette);
                gizmo.outline.push(circle(|t| at(local + Vec3::new(t.x, 0.0, t.y) * r)));
                gizmo.poles.extend([at(local + Vec3::Y * r), at(local - Vec3::Y * r)].into_iter().flatten());
            }
            Shape::Cylinder(axis) => {
                let along = Vec3::AXES[axis];
                let (u, v) = (Vec3::AXES[(axis + 1) % 3], Vec3::AXES[(axis + 2) % 3]);
                let r = (half * (Vec3::ONE - along)).max_element() * CYLINDER_REACH;
                let h = half[axis] * CYLINDER_HEIGHT;
                let cap = |height: f32| circle(|t| at(local + along * height + (u * t.x + v * t.y) * r));
                let top = cap(h);
                if let Some(rim) = top.iter().copied().min_by(|a, b| a.y.total_cmp(&b.y)) {
                    let mut only_axis = [false; 3];
                    only_axis[axis] = true;
                    gizmo.dots.push((keep_in_view(rim), only_axis));
                }
                gizmo.outline.extend([top, cap(-h)]);
                // The two side lines, where the round side turns away from the camera
                if let Some(side) = along.cross(facing - along * facing.dot(along)).try_normalize() {
                    let mut sides = Vec::new();
                    for edge in [side, -side] {
                        let line = [at(local + edge * r + along * h), at(local + edge * r - along * h)];
                        if let [Some(a), Some(b)] = line {
                            gizmo.outline.push(vec![a, b]);
                            sides.push(a.lerp(b, 0.5));
                        }
                    }
                    if let Some(side) = sides.into_iter().max_by(|a, b| a.x.total_cmp(&b.x)) {
                        let mut across = [true; 3];
                        across[axis] = false;
                        gizmo.dots.push((keep_in_view(side), across));
                    }
                }
            }
        }
        Some(gizmo)
    }

    /// A point in the projector's units, in the world
    fn world(&self, local: Vec3) -> Vec3 {
        self.mesh_center + self.turn * (self.stretch * local) * self.radius
    }

    /// The projector's turned X, Y and Z directions in the world
    fn directions(&self) -> [Vec3; 3] {
        Vec3::AXES.map(|axis| self.turn * axis)
    }

    /// The handle under screen point `p`
    pub(crate) fn grab(&self, p: Pos2) -> Option<RoundGrab> {
        if Rect::from_center_size(self.center, egui::Vec2::splat(CENTER_SQUARE)).expand(2.0).contains(p) {
            return Some(RoundGrab::Slide);
        }
        if let Some(axis) = (0..3).find(|&axis| self.arrows[axis].is_some_and(|tip| distance_to_segment(p, self.center, tip) <= GRAB)) {
            return Some(RoundGrab::Axis(axis));
        }
        if let Some((_, axes)) = self.dots.iter().find(|(dot, _)| dot.distance(p) <= GRAB) {
            return Some(RoundGrab::Size(*axes));
        }
        // Where rings cross, the nearest one
        let distance = |ring: &Vec<Pos2>| ring.windows(2).map(|pair| distance_to_segment(p, pair[0], pair[1])).fold(f32::INFINITY, f32::min);
        let (axis, nearest) = (0..3).map(|axis| (axis, distance(&self.rings[axis]))).min_by(|a, b| a.1.total_cmp(&b.1))?;
        (nearest <= RING_GRAB).then_some(RoundGrab::Ring(axis))
    }

    /// The `layer.fill.projection` params that slide the projector across the view from `from` to `to`, as its centre square does
    pub(crate) fn slid(&self, app: &TextureCraftApp, rect: Rect, from: Pos2, to: Pos2) -> Option<Value> {
        self.edit(app, rect, RoundGrab::Slide, from, to)
    }

    /// The `layer.fill.projection` params a drag of `grab` from `from` to `to` sets
    fn edit(&self, app: &TextureCraftApp, rect: Rect, grab: RoundGrab, from: Pos2, to: Pos2) -> Option<Value> {
        let origin = self.world(self.local);
        let (turn, stretch, center) = match grab {
            RoundGrab::Slide => {
                let forward = app.session.camera.forward();
                let moved = on_view_plane(app, rect, origin, forward, to)? - on_view_plane(app, rect, origin, forward, from)?;
                (self.turn, self.stretch, origin + moved)
            }
            RoundGrab::Axis(axis) => {
                let step = (*self.steps.get(axis)?)?;
                let distance = (to - from).dot(step) / step.length_sq().max(f32::MIN_POSITIVE);
                (self.turn, self.stretch, origin + self.directions()[axis] * distance)
            }
            RoundGrab::Ring(axis) => {
                let angle = (to - self.center).angle() - (from - self.center).angle();
                // Seen from the side its direction points to, a turn runs the other way round on screen
                let facing = self.directions()[axis].dot(app.session.camera.forward());
                let angle = if facing < 0.0 { -angle } else { angle };
                let turn = self.turn * Mat3::from_axis_angle(Vec3::AXES[axis], angle);
                (turn, self.stretch, origin)
            }
            RoundGrab::Size(axes) => {
                let scale = (to - self.center).length() / (from - self.center).length().max(1.0);
                let stretch = Vec3::from(axes.map(|on| if on { scale } else { 1.0 })) * self.stretch;
                (self.turn, stretch.clamp(Vec3::splat(MIN_STRETCH), Vec3::splat(MAX_STRETCH)), origin)
            }
        };
        // The centre stays where it was put: the offset follows the turn and scale
        let local = turn.transpose() * ((center - self.mesh_center) / self.radius) / stretch;
        let offset = -local * self.offset_per_local;
        let (z, y, x) = Quat::from_mat3(&turn).to_euler(EulerRot::ZYX);
        let rotation = [x, y, z].map(f32::to_degrees);
        let values = [offset.to_array(), rotation, stretch.to_array()];
        if !values.iter().flatten().all(|value| value.is_finite()) {
            return None;
        }

        let mut params = self.target.target();
        params["offset"] = json!(offset.to_array());
        params["projectorRotation"] = json!(rotation);
        params["projectorScale"] = json!(stretch.to_array());
        Some(params)
    }
}

/// -1 or 1: bit `bit` of a box corner's index
fn sign(index: usize, bit: usize) -> f32 {
    if index >> bit & 1 == 1 { 1.0 } else { -1.0 }
}

/// A closed circle of screen points from `at` (given the unit circle's points); empty when a point is off screen
fn circle(at: impl Fn(Vec2) -> Option<Pos2>) -> Vec<Pos2> {
    (0..=CIRCLE_POINTS)
        .map(|index| {
            let angle = index as f32 / CIRCLE_POINTS as f32 * std::f32::consts::TAU;
            at(Vec2::new(angle.cos(), angle.sin()))
        })
        .collect::<Option<Vec<_>>>()
        .unwrap_or_default()
}

/// Where screen point `p` meets the plane facing the camera through `origin`
fn on_view_plane(app: &TextureCraftApp, rect: Rect, origin: Vec3, forward: Vec3, p: Pos2) -> Option<Vec3> {
    let ndc = Vec2::new((p.x - rect.left()) / rect.width() * 2.0 - 1.0, 1.0 - (p.y - rect.top()) / rect.height() * 2.0);
    let ray = app.session.camera.ray(ndc, rect.width() / rect.height().max(1.0));
    let facing = ray.dir.dot(forward);
    if facing.abs() < 1e-4 {
        return None;
    }
    let hit = ray.origin + ray.dir * ((origin - ray.origin).dot(forward) / facing);
    hit.is_finite().then_some(hit)
}

/// Drags on the projector's handles (one undo step per drag, the projection updates live); returns whether a drag owns the pointer
pub(crate) fn input(app: &mut TextureCraftApp, ui: &Ui, response: &egui::Response, rect: Rect) -> bool {
    let ctx = ui.ctx();
    let gizmo = RoundGizmo::of(app, rect);
    let mut drag = ctx.data(|data| data.get_temp::<RoundDrag>(drag_id())).filter(|drag| gizmo.as_ref().is_some_and(|gizmo| gizmo.target == drag.start.target));
    if response.drag_started_by(PointerButton::Primary)
        && let (Some(gizmo), Some(from)) = (&gizmo, ctx.input(|input| input.pointer.press_origin()))
        && let Some(grab) = gizmo.grab(from)
    {
        drag = Some(RoundDrag { grab, from, start: gizmo.clone(), stepped: false });
    }

    let dragging = response.dragged_by(PointerButton::Primary) || response.drag_started_by(PointerButton::Primary);
    let Some(mut drag) = drag.filter(|_| dragging) else {
        ctx.data_mut(|data| data.remove::<RoundDrag>(drag_id()));
        return false;
    };

    if let Some(pointer) = response.interact_pointer_pos().filter(|_| ctx.input(|input| input.pointer.delta() != egui::Vec2::ZERO))
        && let Some(mut params) = drag.start.edit(app, rect, drag.grab, drag.from, pointer)
    {
        params["merge"] = json!(drag.stepped);
        drag.stepped |= texturecraft_engine::execute(&mut app.session, "layer.fill.projection", &params).is_ok();
    }
    ctx.data_mut(|data| data.insert_temp(drag_id(), drag));
    true
}

/// The projector over the 3D view: dashed shape, rings, arrows, centre square and dots; the cursor shows what the handle under it does
pub(crate) fn overlay(app: &TextureCraftApp, ui: &Ui, rect: Rect) {
    let Some(gizmo) = RoundGizmo::of(app, rect) else { return };

    let tokens = Tokens::get(ui.ctx());
    let painter = ui.painter_at(rect);
    let active = ui.ctx().data(|data| data.get_temp::<RoundDrag>(drag_id())).map(|drag| drag.grab);
    let hovered = ui.ctx().pointer_hover_pos().filter(|hover| rect.contains(*hover)).and_then(|hover| gizmo.grab(hover));
    if let Some(grab) = active.or(hovered) {
        ui.ctx().set_cursor_icon(grab.cursor(active.is_some()));
    }
    let lit = |grab: RoundGrab| active == Some(grab) || (active.is_none() && hovered == Some(grab));
    let fill = |grab: RoundGrab| if lit(grab) { tokens.viewport_accent } else { Color32::WHITE };
    let axis_color = |axis: usize, grab: RoundGrab| if lit(grab) { tokens.viewport_accent } else { AXIS_COLORS[axis] };
    let edge = Stroke::new(1.0, Color32::from_black_alpha(200));

    for line in &gizmo.outline {
        for stroke in [Stroke::new(3.0, Color32::from_black_alpha(140)), Stroke::new(1.2, Color32::from_white_alpha(230))] {
            painter.add(egui::Shape::dashed_line(line, stroke, 6.0, 4.0));
        }
    }
    for pole in &gizmo.poles {
        painter.circle_filled(*pole, POLE_DOT, Color32::WHITE);
    }
    for (dot, axes) in &gizmo.dots {
        painter.circle(*dot, SIZE_DOT, fill(RoundGrab::Size(*axes)), edge);
    }
    for (axis, ring) in gizmo.rings.iter().enumerate() {
        painter.add(egui::Shape::line(ring.clone(), Stroke::new(RING_WIDTH, axis_color(axis, RoundGrab::Ring(axis)))));
    }
    move_handles(&painter, gizmo.center, gizmo.arrows, |axis| axis_color(axis, RoundGrab::Axis(axis)), fill(RoundGrab::Slide));
}

#[cfg(test)]
mod tests {
    use egui::{Modifiers, vec2};
    use egui_kittest::kittest::Queryable;
    use texturecraft_engine::core::{LayerId, Projection, Projector};

    use super::*;
    use crate::frame_gizmo::tests::{drag, fill_app, projection};

    fn moving(projection_id: &str) -> (egui_kittest::Harness<'static, TextureCraftApp>, LayerId) {
        let (mut harness, layer) = fill_app(projection_id);
        harness.state_mut().moving_on_model = Some(MoveOnModel::layer(layer));
        harness.run_steps(2);
        (harness, layer)
    }

    fn gizmo(harness: &egui_kittest::Harness<'_, TextureCraftApp>) -> RoundGizmo {
        RoundGizmo::of(harness.state(), harness.state().viewport_rect).unwrap()
    }

    fn projector_and_offset(projection: &Projection) -> (Projector, [f32; 3]) {
        match projection {
            Projection::Triplanar { projector, offset, .. }
            | Projection::Spherical { projector, offset, .. }
            | Projection::Cylindrical { projector, offset, .. } => (*projector, *offset),
            other => panic!("a round projection: {other:?}"),
        }
    }

    #[test]
    fn round_projections_show_their_projector_only_while_moved_on_the_model() {
        for kind in ["triplanar", "spherical", "cylindrical"] {
            let (mut harness, _) = fill_app(kind);
            assert!(RoundGizmo::of(harness.state(), harness.state().viewport_rect).is_none(), "{kind}: hidden until Move on model");
            harness.get_by_label("Move on model").click();
            harness.run_steps(2);
            let shown = gizmo(&harness);
            assert!(shown.arrows.iter().all(Option::is_some) && shown.rings.iter().all(|ring| ring.len() > 8), "{kind}: arrows and rings {shown:?}");
            assert!(!shown.outline.is_empty(), "{kind}: its shape is drawn");
            let size_dots = if kind == "triplanar" {
                0
            } else if kind == "spherical" {
                1
            } else {
                2
            };
            assert_eq!(shown.dots.len(), size_dots, "{kind}: size dots");
        }
        let (harness, _) = moving("planar");
        assert!(RoundGizmo::of(harness.state(), harness.state().viewport_rect).is_none(), "planar projections show their frame instead");
    }

    #[test]
    fn the_arrows_move_the_rings_turn_and_the_dots_scale_the_projector_one_step_per_drag() {
        let _language = crate::i18n::TEST_LANGUAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let (mut harness, layer) = moving("spherical");
        let steps = harness.state().session.history_labels().len();

        let start = gizmo(&harness);
        let tip = start.arrows[0].unwrap();
        drag(&mut harness, tip, tip + (tip - start.center).normalized() * 30.0, Modifiers::NONE);
        let (projector, offset) = projector_and_offset(&projection(&harness, layer));
        assert!(offset[0] < -0.05 && offset[1].abs() < 1e-3 && offset[2].abs() < 1e-3, "the X arrow moves the centre along X: {offset:?}");
        assert_eq!(projector.rotation, [0.0; 3]);
        let moved = gizmo(&harness);
        assert!(moved.center.x > start.center.x + 20.0, "the centre follows: {moved:?}");

        // The Z ring (seen face on) turns it round Z; the centre stays where it is
        let clear = |p: &&Pos2| moved.rings[..2].iter().flatten().map(|other| other.distance(**p)).fold(f32::INFINITY, f32::min);
        let on_ring = *moved.rings[2].iter().max_by(|a, b| clear(a).total_cmp(&clear(b))).unwrap();
        assert_eq!(moved.grab(on_ring), Some(RoundGrab::Ring(2)));
        drag(&mut harness, on_ring, moved.center + (on_ring - moved.center).rot90(), Modifiers::NONE);
        let (turned, _) = projector_and_offset(&projection(&harness, layer));
        assert!(turned.rotation[2].abs() > 30.0 && turned.rotation[0].abs() < 1e-2, "{:?}", turned.rotation);
        assert!(gizmo(&harness).center.distance(moved.center) < 1.0, "turning keeps the centre");

        let (dot, _) = gizmo(&harness).dots[0];
        let center = gizmo(&harness).center;
        assert_eq!(gizmo(&harness).grab(dot), Some(RoundGrab::Size([true; 3])));
        drag(&mut harness, dot, center + (dot - center) * 1.5, Modifiers::NONE);
        let (scaled, _) = projector_and_offset(&projection(&harness, layer));
        assert!(scaled.scale.iter().all(|scale| (scale - 1.5).abs() < 0.05), "the side dot scales it evenly: {:?}", scaled.scale);
        assert!(gizmo(&harness).center.distance(center) < 1.0, "scaling keeps the centre");
        assert_eq!(harness.state().session.history_labels().len(), steps + 3, "one undo step per drag");
    }

    #[test]
    fn a_cylinders_top_dot_stretches_its_height_and_a_tri_planar_box_moves_by_its_texture_offset() {
        let _language = crate::i18n::TEST_LANGUAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let (mut harness, layer) = moving("cylindrical");
        let cylinder = gizmo(&harness);
        let (top, axes) = cylinder.dots[0];
        assert_eq!(axes, [false, true, false], "the top dot works along the axis");
        drag(&mut harness, top, cylinder.center + (top - cylinder.center) * 0.5, Modifiers::NONE);
        let (projector, _) = projector_and_offset(&projection(&harness, layer));
        assert!(projector.scale[1] < 0.7 && projector.scale[0] == 1.0 && projector.scale[2] == 1.0, "{:?}", projector.scale);

        let (mut harness, layer) = moving("triplanar");
        harness.state_mut().run("layer.fill.projection", json!({"layer": layer.0, "scale": 4})).unwrap();
        harness.run_steps(2);
        let box_gizmo = gizmo(&harness);
        drag(&mut harness, box_gizmo.center, box_gizmo.center + vec2(0.0, -30.0), Modifiers::NONE);
        let (_, offset) = projector_and_offset(&projection(&harness, layer));
        assert!(offset[1] < -0.1 && offset[0].abs() < 0.05, "sliding up shifts the texture by the tiling: {offset:?}");
        assert!(gizmo(&harness).center.distance(box_gizmo.center + vec2(0.0, -30.0)) < 1.5, "the centre sits under the pointer");
    }
}
