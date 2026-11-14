//! The selected tri-planar, spherical or cylindrical fill shows its projector round the whole model: a dashed box, sphere or cylinder with the centre square, X/Y/Z arrows and turn rings, and size handles: a dot scales it evenly (or a cylinder across its axis), a double arrow stretches it along one direction. Only a press on one of those handles moves it: any other drag paints or turns the camera

use egui::{Color32, PointerButton, Pos2, Rect, Stroke, Ui};
use serde_json::{Value, json};
use texturecraft_engine::core::Projection;
use texturecraft_engine::geom::glam::{EulerRot, Mat3, Quat, Vec2, Vec3};

use crate::TextureCraftApp;
use crate::frame_gizmo::{
    ARROW, HALO, HALO_EXTRA_WIDTH, SHAFT_WIDTH, TURN_RING, arrow_distance, center_distance, distance_to_segment, handle_dot, move_handles, nearest,
};
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

/// A handle that resizes the projector
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SizeHandle {
    pub at: Pos2,
    /// The projector directions it scales
    pub axes: [bool; 3],
    /// A double arrow's direction on screen; None for a dot
    pub along: Option<egui::Vec2>,
}

impl SizeHandle {
    /// How far screen point `p` is from the handle (a double arrow: from anywhere along it)
    fn distance(&self, p: Pos2) -> f32 {
        match self.along {
            Some(along) => distance_to_segment(p, self.at - along * SIZE_ARROW, self.at + along * SIZE_ARROW),
            None => self.at.distance(p),
        }
    }

    /// The resize cursor along its arrow, or a diagonal one for a dot
    fn cursor(&self) -> egui::CursorIcon {
        match self.along {
            Some(along) if along.y.abs() >= along.x.abs() => egui::CursorIcon::ResizeVertical,
            Some(_) => egui::CursorIcon::ResizeHorizontal,
            None => egui::CursorIcon::ResizeNeSw,
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
    pub(crate) target: MoveOnModel,
    /// Mesh centre and radius: the projector reads positions around it in radii
    mesh_center: Vec3,
    radius: f32,
    /// The projector's turn and per-axis scale
    turn: Mat3,
    stretch: Vec3,
    /// The projection's centre in the projector's units, and what offset puts it there (offset = -local × this)
    local: Vec3,
    offset_per_local: f32,
    /// On screen: the centre, the arrow tips, the turn rings, the dashed outline and the size handles
    pub center: Pos2,
    pub arrows: [Option<Pos2>; 3],
    pub rings: [Vec<Pos2>; 3],
    outline: Vec<Vec<Pos2>>,
    pub sizes: Vec<SizeHandle>,
    /// Screen movement per world unit along each projector direction at the centre
    steps: [Option<egui::Vec2>; 3],
}

/// How far round the model each shape reaches, in the mesh's half sizes: the box, the sphere, the cylinder's radius and half height
pub(crate) const BOX_REACH: f32 = 1.08;
pub(crate) const SPHERE_REACH: f32 = 1.2;
pub(crate) const CYLINDER_REACH: f32 = 1.1;
pub(crate) const CYLINDER_HEIGHT: f32 = 1.15;
/// The size dots' radius (points)
const SIZE_DOT: f32 = 7.0;
/// A size double arrow's half length, tip to middle, and its heads' length and half width (points)
const SIZE_ARROW: f32 = 12.0;
const SIZE_ARROW_HEAD: f32 = 7.0;
const SIZE_ARROW_HEAD_HALF_WIDTH: f32 = 5.0;
/// How far from the centre a box face's double arrow must land on screen to show: nearer, the face is seen head on and its arrow would sit on the centre square
const FACE_CLEARANCE: f32 = 40.0;
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
    /// The projector of the selected tri-planar, spherical or cylindrical fill
    pub(crate) fn of(app: &TextureCraftApp, rect: Rect) -> Option<RoundGizmo> {
        if app.ui.mode != crate::state::Mode::Painting {
            return None;
        }
        let target = MoveOnModel::selected(app)?;
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
            sizes: Vec::new(),
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
        let ring = TURN_RING / pixels.max(f32::MIN_POSITIVE);
        gizmo.rings = directions.map(|direction| {
            let (u, v) = direction.any_orthonormal_pair();
            circle(|t| project(app, rect, origin + (u * t.x + v * t.y) * ring))
        });

        // The shape, sized round the mesh in the projector's units; a size handle past the view's edge waits at the edge, where it can be grabbed
        let view = rect.shrink(SIZE_ARROW);
        let keep_in_view = |point: Pos2| point.clamp(view.min, view.max);
        let dot = |at: Pos2, axes: [bool; 3]| SizeHandle { at: keep_in_view(at), axes, along: None };
        let only = |axis: usize| {
            let mut axes = [false; 3];
            axes[axis] = true;
            axes
        };
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
                // The corner up and right on screen scales it evenly; a double arrow on the face turned towards the camera stretches it along each direction
                let corners = (0..8).filter_map(|index| at(corner(index)));
                if let Some(top_right) = corners.max_by(|a, b| (a.x - a.y).total_cmp(&(b.x - b.y))) {
                    gizmo.sizes.push(dot(top_right, [true; 3]));
                }
                for (axis, direction) in directions.iter().enumerate() {
                    let toward = if direction.dot(forward) > 0.0 { -1.0 } else { 1.0 };
                    let Some(face) = at(local + Vec3::AXES[axis] * size * toward) else { continue };
                    let out = face - gizmo.center;
                    if out.length() >= FACE_CLEARANCE {
                        gizmo.sizes.push(SizeHandle { at: keep_in_view(face), axes: only(axis), along: Some(out.normalized()) });
                    }
                }
            }
            Shape::Sphere => {
                let r = half.max_element() * SPHERE_REACH;
                let (u, v) = facing.any_orthonormal_pair();
                let silhouette = circle(|t| at(local + (u * t.x + v * t.y) * r));
                // A sphere only scales evenly: one dot, on its right side
                if let Some(side) = silhouette.iter().copied().max_by(|a, b| a.x.total_cmp(&b.x)) {
                    gizmo.sizes.push(dot(side, [true; 3]));
                }
                gizmo.outline.push(silhouette);
                gizmo.outline.push(circle(|t| at(local + Vec3::new(t.x, 0.0, t.y) * r)));
            }
            Shape::Cylinder(axis) => {
                let along = Vec3::AXES[axis];
                let (u, v) = (Vec3::AXES[(axis + 1) % 3], Vec3::AXES[(axis + 2) % 3]);
                let r = (half * (Vec3::ONE - along)).max_element() * CYLINDER_REACH;
                let h = half[axis] * CYLINDER_HEIGHT;
                let cap = |height: f32| circle(|t| at(local + along * height + (u * t.x + v * t.y) * r));
                let top = cap(h);
                // Its height: a double arrow along the axis (seen end on, a dot)
                let along_axis = match (at(local + along * h), at(local - along * h)) {
                    (Some(top), Some(bottom)) => Some(top - bottom).filter(|span| span.length() > 1.0).map(egui::Vec2::normalized),
                    _ => None,
                };
                if let Some(rim) = top.iter().copied().min_by(|a, b| a.y.total_cmp(&b.y)) {
                    gizmo.sizes.push(SizeHandle { at: keep_in_view(rim), axes: only(axis), along: along_axis });
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
                        gizmo.sizes.push(dot(side, across));
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

    /// The cursor over or dragging `grab`: a size handle's resize cursor runs along its arrow
    fn cursor(&self, grab: RoundGrab, dragging: bool) -> egui::CursorIcon {
        match grab {
            RoundGrab::Slide if dragging => egui::CursorIcon::Grabbing,
            RoundGrab::Slide => egui::CursorIcon::Grab,
            RoundGrab::Axis(_) | RoundGrab::Ring(_) => egui::CursorIcon::Move,
            RoundGrab::Size(axes) => self.sizes.iter().find(|size| size.axes == axes).map_or(egui::CursorIcon::ResizeNeSw, SizeHandle::cursor),
        }
    }

    /// The handle under screen point `p`: the nearest one within reach (where rings cross, the nearest ring)
    pub(crate) fn grab(&self, p: Pos2) -> Option<RoundGrab> {
        let mut handles = vec![(RoundGrab::Slide, center_distance(self.center, p))];
        handles.extend((0..3).filter_map(|axis| Some((RoundGrab::Axis(axis), arrow_distance(p, self.center, self.arrows[axis]?)))));
        handles.extend(self.sizes.iter().map(|size| (RoundGrab::Size(size.axes), size.distance(p))));
        let distance = |ring: &Vec<Pos2>| ring.windows(2).map(|pair| distance_to_segment(p, pair[0], pair[1])).fold(f32::INFINITY, f32::min);
        handles.extend((0..3).map(|axis| (RoundGrab::Ring(axis), distance(&self.rings[axis]))));
        nearest(handles)
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
                // How far the pointer went along the line from the centre through where the handle was grabbed
                let reach = from - self.center;
                let scale = (to - self.center).dot(reach) / reach.length_sq().max(1.0);
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

/// A size handle that stretches one way: a double arrow through `at` along `along`, `fill` over a dark halo like the axis arrows
fn size_arrow(painter: &egui::Painter, at: Pos2, along: egui::Vec2, fill: Color32) {
    let edge = Stroke::new(1.0, Color32::from_black_alpha(200));
    let side = along.rot90() * SIZE_ARROW_HEAD_HALF_WIDTH;
    let shaft = [at - along * (SIZE_ARROW - SIZE_ARROW_HEAD), at + along * (SIZE_ARROW - SIZE_ARROW_HEAD)];
    painter.line_segment(shaft, Stroke::new(SHAFT_WIDTH + HALO_EXTRA_WIDTH, HALO));
    painter.line_segment(shaft, Stroke::new(SHAFT_WIDTH, fill));
    for direction in [along, -along] {
        let base = at + direction * (SIZE_ARROW - SIZE_ARROW_HEAD);
        painter.add(egui::Shape::convex_polygon(vec![at + direction * SIZE_ARROW, base + side, base - side], fill, edge));
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

/// Whether the pointer is over one of the selected projector's handles or dragging one (the brush cursor hides there and a press there doesn't paint)
pub(crate) fn hovered(app: &TextureCraftApp, ui: &Ui, rect: Rect) -> bool {
    let dragging = ui.ctx().data(|data| data.get_temp::<RoundDrag>(drag_id())).is_some();
    dragging
        || ui.ctx().pointer_hover_pos().is_some_and(|hover| rect.contains(hover) && RoundGizmo::of(app, rect).is_some_and(|gizmo| gizmo.grab(hover).is_some()))
}

/// Whether screen point `at` is on one of the selected projector's handles
pub(crate) fn handle_at(app: &TextureCraftApp, rect: Rect, at: Pos2) -> bool {
    rect.contains(at) && RoundGizmo::of(app, rect).is_some_and(|gizmo| gizmo.grab(at).is_some())
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
        // The panel drew before this edit: one more frame shows its fields and preview where the projector is now
        ctx.request_repaint();
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
    // A held Move on model button owns the cursor
    let hovered = ui.ctx().pointer_hover_pos().filter(|hover| rect.contains(*hover) && app.moving_on_model.is_none()).and_then(|hover| gizmo.grab(hover));
    if let Some(grab) = active.or(hovered) {
        ui.ctx().set_cursor_icon(gizmo.cursor(grab, active.is_some()));
    }
    let lit = |grab: RoundGrab| active == Some(grab) || (active.is_none() && hovered == Some(grab));
    let fill = |grab: RoundGrab| if lit(grab) { tokens.viewport_accent } else { Color32::WHITE };
    let axis_color = |axis: usize, grab: RoundGrab| if lit(grab) { tokens.viewport_accent } else { AXIS_COLORS[axis] };
    for line in &gizmo.outline {
        for stroke in [Stroke::new(3.0, Color32::from_black_alpha(140)), Stroke::new(1.2, Color32::from_white_alpha(230))] {
            painter.add(egui::Shape::dashed_line(line, stroke, 6.0, 4.0));
        }
    }
    for (axis, ring) in gizmo.rings.iter().enumerate() {
        painter.add(egui::Shape::line(ring.clone(), Stroke::new(SHAFT_WIDTH + HALO_EXTRA_WIDTH, HALO)));
        painter.add(egui::Shape::line(ring.clone(), Stroke::new(SHAFT_WIDTH, axis_color(axis, RoundGrab::Ring(axis)))));
    }
    for size in &gizmo.sizes {
        let color = fill(RoundGrab::Size(size.axes));
        match size.along {
            Some(along) => size_arrow(&painter, size.at, along, color),
            None => handle_dot(&painter, size.at, SIZE_DOT, color),
        }
    }
    move_handles(&painter, gizmo.center, gizmo.arrows, |axis| axis_color(axis, RoundGrab::Axis(axis)), fill(RoundGrab::Slide));
}

#[cfg(test)]
mod tests {
    use egui::{Modifiers, vec2};
    use egui_kittest::kittest::Queryable;
    use texturecraft_engine::core::{LayerId, Projection, Projector};

    use super::*;
    use crate::frame_gizmo::tests::{drag, drag_frames, fill_app, panel_xyz, projection};

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
    fn round_projections_show_their_projector_while_their_fill_is_selected() {
        let _language = crate::i18n::TEST_LANGUAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        for kind in ["triplanar", "spherical", "cylindrical"] {
            let (mut harness, _) = fill_app(kind);
            harness.run_steps(2);
            let shown = gizmo(&harness);
            assert!(shown.arrows.iter().all(Option::is_some) && shown.rings.iter().all(|ring| ring.len() > 8), "{kind}: arrows and rings {shown:?}");
            assert!(!shown.outline.is_empty(), "{kind}: its shape is drawn");
            // A box's corner and a double arrow per face (the ball is seen from above and aside, no face head on); a sphere's one dot; a cylinder's top rim and side
            let size_handles = match kind {
                "triplanar" => 4,
                "spherical" => 1,
                _ => 2,
            };
            assert_eq!(shown.sizes.len(), size_handles, "{kind}: size handles {:?}", shown.sizes);
        }
        let (harness, _) = fill_app("planar");
        assert!(RoundGizmo::of(harness.state(), harness.state().viewport_rect).is_none(), "planar projections show their frame instead");
    }

    #[test]
    fn the_arrows_move_the_rings_turn_and_the_dots_scale_the_projector_one_step_per_drag() {
        let _language = crate::i18n::TEST_LANGUAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let (mut harness, layer) = fill_app("spherical");
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

        let dot = gizmo(&harness).sizes[0].at;
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
        let (mut harness, layer) = fill_app("cylindrical");
        let cylinder = gizmo(&harness);
        let SizeHandle { at: top, axes, along } = cylinder.sizes[0];
        assert_eq!(axes, [false, true, false], "the top handle works along the axis");
        let along = along.expect("the height handle is a double arrow");
        assert!(along.y.abs() > 0.99, "drawn upright, along the upright axis: {along:?}");
        assert_eq!(cylinder.cursor(RoundGrab::Size(axes), false), egui::CursorIcon::ResizeVertical);
        drag(&mut harness, top, cylinder.center + (top - cylinder.center) * 0.5, Modifiers::NONE);
        let (projector, _) = projector_and_offset(&projection(&harness, layer));
        assert!(projector.scale[1] < 0.7 && projector.scale[0] == 1.0 && projector.scale[2] == 1.0, "{:?}", projector.scale);

        let (mut harness, layer) = fill_app("triplanar");
        harness.state_mut().run("layer.fill.projection", json!({"layer": layer.0, "scale": 4})).unwrap();
        harness.run_steps(2);
        let box_gizmo = gizmo(&harness);
        drag(&mut harness, box_gizmo.center, box_gizmo.center + vec2(0.0, -30.0), Modifiers::NONE);
        let (_, offset) = projector_and_offset(&projection(&harness, layer));
        assert!(offset[1] < -0.1 && offset[0].abs() < 0.05, "sliding up shifts the texture by the tiling: {offset:?}");
        assert!(gizmo(&harness).center.distance(box_gizmo.center + vec2(0.0, -30.0)) < 1.5, "the centre sits under the pointer");
    }

    /// Every drag on the projector shows in the panel the same frame: its Offset, Rotation and Scale fields hold the projector's values and the preview square re-renders, every frame of the drag; one undo step per drag
    #[test]
    fn the_panel_and_its_preview_follow_every_frame_of_a_projector_drag() {
        let _language = crate::i18n::TEST_LANGUAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        for kind in ["spherical", "cylindrical"] {
            let (mut harness, layer) = fill_app(kind);
            let steps = harness.state().session.history_labels().len();
            let read = |harness: &egui_kittest::Harness<'_, TextureCraftApp>| {
                let (projector, offset) = projector_and_offset(&projection(harness, layer));
                let (row_offset, row_rotation, row_scale) = (panel_xyz(harness, "Offset"), panel_xyz(harness, "Rotation"), panel_xyz(harness, "Scale"));
                for (row, field, value) in
                    [("Offset", row_offset, offset), ("Rotation", row_rotation, projector.rotation), ("Scale", row_scale, projector.scale)]
                {
                    for axis in 0..3 {
                        assert!((field[axis] - value[axis]).abs() < 1e-3, "{kind}: the panel's {row} shows the projector's {value:?}, not {field:?}");
                    }
                }
            };

            let start = gizmo(&harness);
            let dot = start.sizes[0].at;
            drag_frames(&mut harness, layer, dot, start.center + (dot - start.center) * 1.5, Modifiers::NONE, true, read);
            let tip = gizmo(&harness).arrows[0].unwrap();
            let center = gizmo(&harness).center;
            drag_frames(&mut harness, layer, tip, tip + (tip - center).normalized() * 30.0, Modifiers::NONE, true, read);
            let turned = gizmo(&harness);
            let clear = |p: &&Pos2| turned.rings[..2].iter().flatten().map(|other| other.distance(**p)).fold(f32::INFINITY, f32::min);
            let on_ring = *turned.rings[2].iter().max_by(|a, b| clear(a).total_cmp(&clear(b))).unwrap();
            drag_frames(&mut harness, layer, on_ring, turned.center + (on_ring - turned.center).rot90(), Modifiers::NONE, true, read);
            let (projector, _) = projector_and_offset(&projection(&harness, layer));
            assert!(
                projector.scale.iter().copied().fold(0.0, f32::max) > 1.3 && projector.rotation[2].abs() > 30.0,
                "{kind}: resized and turned: {projector:?}"
            );
            assert_eq!(harness.state().session.history_labels().len(), steps + 3, "{kind}: one undo step per drag");
        }
    }

    /// A paint layer on the ball whose mask holds a fill effect (selected) projecting `kind`: painting there paints the mask
    fn mask_fill_app(kind: &str) -> (egui_kittest::Harness<'static, TextureCraftApp>, LayerId) {
        let session =
            texturecraft_engine::Session::new_project("Gizmo", texturecraft_engine::geom::primitives::sphere(1.0, 48, 24), "builtin:sphere", None, 128)
                .unwrap();
        let mut harness = egui_kittest::Harness::builder().with_size(vec2(1600.0, 960.0)).build_eframe(|context| TextureCraftApp::new(context, session));
        harness.run_steps(4);
        let mut png = Vec::new();
        image::RgbaImage::from_pixel(4, 4, image::Rgba([255, 255, 255, 255])).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        let image = harness.state_mut().session.import_resource_bytes("decal.png", png).unwrap();
        harness.state_mut().run("layer.add.decal", json!({"image": image, "channel": "mask", "size": 0.5})).unwrap();
        let texturecraft_engine::StackTarget::Effect(effect) = harness.state().session.target else { panic!("the mask fill is selected") };
        harness.state_mut().run("layer.fill.projection", json!({"effect": effect.0, "projection": kind})).unwrap();
        harness.run_steps(2);
        (harness, effect)
    }

    fn effect_projection(harness: &egui_kittest::Harness<'_, TextureCraftApp>, effect: LayerId) -> Projection {
        match &harness.state().session.doc.project.effect(effect).unwrap().1.kind {
            texturecraft_engine::core::EffectKind::Fill { projection, .. } => projection.clone(),
            other => panic!("a fill effect: {other:?}"),
        }
    }

    /// A point on the model clear of every handle of the shown gizmo
    fn on_the_model_off_the_handles(harness: &egui_kittest::Harness<'_, TextureCraftApp>) -> Pos2 {
        let app = harness.state();
        let rect = app.viewport_rect;
        let round = RoundGizmo::of(app, rect);
        let frame = crate::frame_gizmo::FrameGizmo::of(app, rect);
        let clear = |p: Pos2| {
            round.as_ref().is_none_or(|gizmo| gizmo.grab(p).is_none())
                && frame.as_ref().is_none_or(|gizmo| gizmo.grab(p).is_none())
                && app.session.pick(texturecraft_engine::geom::glam::Vec2::new(p.x - rect.left(), p.y - rect.top()), app.session.viewport).is_some()
        };
        (1..12)
            .flat_map(|ring| (0..16).map(move |step| rect.center() + egui::Vec2::angled(step as f32 / 16.0 * std::f32::consts::TAU) * (ring as f32 * 14.0)))
            .find(|p| clear(*p) && clear(*p + vec2(30.0, 0.0)))
            .expect("the model shows past the handles")
    }

    /// With painting on and every projection, a drag on the model away from the gizmo paints (the projection stays put); only a drag that starts on a handle moves the projection, and it paints nothing
    #[test]
    fn only_a_handle_moves_the_projection_and_any_other_drag_on_the_model_paints() {
        let _language = crate::i18n::TEST_LANGUAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        for kind in ["planar", "warp", "triplanar", "spherical", "cylindrical"] {
            let (mut harness, effect) = mask_fill_app(kind);
            let rect = harness.state().viewport_rect;
            let before = effect_projection(&harness, effect);
            let steps = harness.state().session.history_labels().len();

            let from = on_the_model_off_the_handles(&harness);
            drag(&mut harness, from, from + vec2(30.0, 0.0), Modifiers::NONE);
            assert_eq!(effect_projection(&harness, effect), before, "{kind}: a drag off the handles leaves the projection alone");
            assert_eq!(harness.state().session.history_labels().len(), steps + 1, "{kind}: and paints one stroke");

            let center = match RoundGizmo::of(harness.state(), rect) {
                Some(round) => round.center,
                None => crate::frame_gizmo::FrameGizmo::of(harness.state(), rect).expect("{kind}: a gizmo shows").center,
            };
            drag(&mut harness, center, center + vec2(0.0, -30.0), Modifiers::NONE);
            assert_ne!(effect_projection(&harness, effect), before, "{kind}: the centre square moves it");
            assert_eq!(harness.state().session.history_labels().len(), steps + 2, "{kind}: one step, no stroke");
        }
    }

    /// A sphere has one size handle (it only scales evenly); a tri-planar box scales evenly from its corner and along one direction from a face's double arrow
    #[test]
    fn a_sphere_scales_from_one_dot_and_a_tri_planar_box_from_its_corner_and_faces() {
        let _language = crate::i18n::TEST_LANGUAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let (mut harness, layer) = fill_app("spherical");
        let start = gizmo(&harness);
        assert_eq!(start.sizes.len(), 1, "one dot: {:?}", start.sizes);
        let dot = start.sizes[0].at;
        assert_eq!(start.grab(dot), Some(RoundGrab::Size([true; 3])));
        drag(&mut harness, dot, start.center + (dot - start.center) * 1.4, Modifiers::NONE);
        let (scaled, _) = projector_and_offset(&projection(&harness, layer));
        assert!(scaled.scale.iter().all(|scale| (scale - 1.4).abs() < 0.08), "it scales evenly: {:?}", scaled.scale);

        let (mut harness, layer) = fill_app("triplanar");
        let start = gizmo(&harness);
        let corner = start.sizes.iter().find(|size| size.axes == [true; 3]).expect("a corner dot").at;
        drag(&mut harness, corner, start.center + (corner - start.center) * 1.25, Modifiers::NONE);
        let (scaled, _) = projector_and_offset(&projection(&harness, layer));
        assert!(scaled.scale.iter().all(|scale| (scale - 1.25).abs() < 0.05), "the corner scales it evenly: {:?}", scaled.scale);
        assert!(gizmo(&harness).center.distance(start.center) < 1.0, "scaling keeps the centre");

        let box_gizmo = gizmo(&harness);
        let across = *box_gizmo.sizes.iter().find(|size| size.axes == [true, false, false]).expect("an X face arrow");
        assert!(across.along.is_some_and(|along| along.x.abs() > 0.95), "it points across: {across:?}");
        assert_eq!(
            box_gizmo.grab(across.at + across.along.unwrap_or_default() * (SIZE_ARROW - 1.0)),
            Some(RoundGrab::Size([true, false, false])),
            "grabbed from its tip"
        );
        drag(&mut harness, across.at, box_gizmo.center + (across.at - box_gizmo.center) * 0.5, Modifiers::NONE);
        let (stretched, _) = projector_and_offset(&projection(&harness, layer));
        assert!((stretched.scale[0] - scaled.scale[0] * 0.5).abs() < 0.05, "the X face halves it across: {:?}", stretched.scale);
        assert!(
            (stretched.scale[1] - scaled.scale[1]).abs() < 1e-4 && (stretched.scale[2] - scaled.scale[2]).abs() < 1e-4,
            "and only across: {:?}",
            stretched.scale
        );
    }

    /// A plain left press on the projector's handle while a colour picker is open only closes the picker: no stroke, no slide, and the press after it works again
    #[test]
    fn a_press_that_closes_the_colour_picker_neither_paints_nor_slides() {
        let _language = crate::i18n::TEST_LANGUAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let (mut harness, layer) = fill_app("triplanar");
        let before = projection(&harness, layer);
        let from = gizmo(&harness).center;
        harness.get_by_role_and_label(egui::Role::Tab, "Material").click();
        harness.run_steps(25);
        harness.get_by_role_and_label(egui::Role::Button, "Base color").click();
        harness.run_steps(2);
        assert!(crate::color_picker::open_window(&harness.ctx).is_some(), "the picker is open");
        let steps = harness.state().session.history_labels().len();

        drag(&mut harness, from, from + vec2(40.0, 0.0), Modifiers::NONE);
        assert!(crate::color_picker::open_window(&harness.ctx).is_none(), "the press closed the picker");
        assert_eq!(projection(&harness, layer), before, "and slid nothing");
        assert_eq!(harness.state().session.history_labels().len(), steps, "nor made an undo step");

        drag(&mut harness, from, from + vec2(40.0, 0.0), Modifiers::NONE);
        assert_ne!(projection(&harness, layer), before, "the next press slides again");
    }
}
