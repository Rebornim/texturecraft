//! The preview square of a 3D projection: a checker laid on a sphere, cylinder or cube through the fill's own projection
//!
//! Each pixel casts a ray at the shape and samples the checker with the compositor's projection math
//! ([`image_samples`] and [`projector_cover`]), so tiling, offset, rotation, Hardness, Angle, the 3D projection
//! settings, Shape crop and Backface culling show as they will on the model. Depth culling needs the model's depth
//! map and is not previewed. The projector is drawn round the shape as the 3D view's gizmo draws it round the model, and the
//! view zooms out until both fit, down to half the shape's size; a projector bigger than that runs off the square, marked on
//! each side it passes

use egui::{Color32, ColorImage, Painter, Pos2, Rect, Shape, Stroke, TextureOptions, pos2, vec2};
use texturecraft_engine::compose::frame_placement::{folded, upright_turn};
use texturecraft_engine::compose::pattern::{Coords, decal_axes, image_samples};
use texturecraft_engine::compose::projector_cover::projector_cover;
use texturecraft_engine::core::{Projection, Projector, ShapeCrop};
use texturecraft_engine::geom::glam::{Vec2, Vec3};

use rayon::prelude::*;

use crate::preview_outline::ProjectorOutline;
use crate::theme::Tokens;

mod preview_shape;
mod rendered_preview;
mod screen;
mod view;

use preview_shape::PreviewShape;
use rendered_preview::RenderedPreview;
use screen::Screen;
use view::View;

/// Largest preview side in pixels; the square is never drawn bigger than this
const MAX_SIDE_PX: usize = 256;
/// Rays per pixel along each axis (smooth outline)
const SUPERSAMPLE: usize = 2;
/// Rays per pixel along each axis while a drag is held: a quarter of the rays, so the square keeps up with the pointer; the full picture follows on release
const DRAG_SUPERSAMPLE: usize = 1;
/// Checker cells across one repeat of the source along each axis
const CHECKER_CELLS: f32 = 4.0;
/// Half the view's width and height, in mesh radii: as near as the view comes, and as far as it zooms out (the shape at half its size)
const VIEW_HALF: f32 = 1.0;
const MAX_VIEW_HALF: f32 = 2.0;
/// Room left round the projector when the view zooms out to it
const FIT_MARGIN: f32 = 1.12;
/// How far in front of the middle the rays start, in mesh radii (past every shape and every outline point tested for cover)
const CAMERA_DISTANCE: f32 = 3.0;
/// Screen length of the pieces an outline is split into to find its hidden parts (points)
const OUTLINE_STEP: f32 = 3.0;
/// The outline's dashes where the shape hides it, and the edge marker's dashes (points)
const HIDDEN_DASH: f32 = 3.0;
const EDGE_DASH: f32 = 6.0;
/// The edge marker's inset from the square's side, its chevron's arm and the dot at a planar's direction tip (points)
const EDGE_INSET: f32 = 2.0;
const CHEVRON_ARM: f32 = 5.0;
const TIP_DOT: f32 = 2.2;
/// Camera turn around the up axis and down from level (degrees): three faces of the cube and the cylinder's top show
const CAMERA_YAW: f32 = 35.0;
const CAMERA_PITCH: f32 = 25.0;
/// Light that reaches faces turned away from the light
const AMBIENT: f32 = 0.3;
/// Shapes in mesh radii, inside the unit sphere the compositor's coordinates span
const SPHERE_RADIUS: f32 = 0.85;
const CYLINDER_RADIUS: f32 = 0.55;
const CYLINDER_HALF_HEIGHT: f32 = 0.7;
const CUBE_HALF: f32 = 0.55;
/// Checker shades, as fractions of the way from `field` to `text`
const CHECKER_DARK: f32 = 0.12;
const CHECKER_LIGHT: f32 = 1.0;
/// The shape where the projection does not reach, as a fraction of the way from `field` to `text`
const UNCOVERED: f32 = 0.45;

/// Draw the preview of `projection` on a mesh within `bounds` into `rect`, rendered once per projection, size and theme, with a quarter of the rays while a drag is held (None for projections without one: UV, warp)
pub(crate) fn projection_preview(painter: &Painter, rect: Rect, projection: &Projection, bounds: (Vec3, Vec3), tokens: &Tokens) -> Option<()> {
    let shown = shown_projection(projection, bounds)?;
    let ctx = painter.ctx();
    let side = ((rect.width() * ctx.pixels_per_point()).round() as usize).clamp(1, MAX_SIDE_PX);
    let shape = shape_for(&shown);
    let outline = ProjectorOutline::of(&shown, shape.half_size());
    let (resting, half) = resting_fit(&shown, shape);
    let key = format!("{side}{half}{shown:?}{:?}{:?}{:?}", tokens.field, tokens.text, tokens.accent);
    let held = ctx.input(|input| input.pointer.any_down());
    let id = egui::Id::unique("projection_preview");
    // The picture, the key and the rays per pixel it was rendered with; a drag's lighter picture is redone in full once the pointer is let go
    let cached = ctx.data(|data| data.get_temp::<RenderedPreview>(id));
    let up_to_date = cached.as_ref().is_some_and(|cached| cached.key == key && (held || cached.supersample == SUPERSAMPLE));
    let texture = match cached {
        Some(cached) if up_to_date => cached.texture,
        cached => {
            let supersample = if held { DRAG_SUPERSAMPLE } else { SUPERSAMPLE };
            let image = render(&shown, side, half, supersample, tokens);
            let texture = match cached {
                Some(RenderedPreview { mut texture, .. }) => {
                    texture.set(image, TextureOptions::LINEAR);
                    texture
                }
                None => ctx.load_texture("projection-preview", image, TextureOptions::LINEAR),
            };
            ctx.data_mut(|data| data.insert_temp(id, RenderedPreview { key, supersample, texture: texture.clone() }));
            texture
        }
    };
    painter.image(texture.id(), rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
    if let Some(outline) = &outline {
        let painter = painter.with_clip_rect(rect.intersect(painter.clip_rect()));
        let screen = Screen { view: View::new(), rect, half };
        draw_outline(&painter, &screen, shape, outline, tokens);
        let center = resting.as_ref().map_or(rect.center(), |resting| screen.at(resting.center));
        crate::preview_dots::preview_dots(&painter, center, screen.at(outline.center), tokens);
        edge_markers(&painter, rect, passed_sides(outline.points().chain([outline.center]).map(|point| screen.at(point)), rect), tokens);
    }
    painter.rect_stroke(rect, 0, egui::Stroke::new(1.0, tokens.field_border), egui::StrokeKind::Inside);
    Some(())
}

/// What the preview square last rendered (tests: a drag re-renders it)
#[cfg(test)]
pub(crate) fn shown_key(ctx: &egui::Context) -> Option<String> {
    ctx.data(|data| data.get_temp::<RenderedPreview>(egui::Id::unique("projection_preview"))).map(|rendered| rendered.key)
}

/// How the preview square was last rendered, with rays per pixel (tests: a drag renders it lighter, the release in full)
#[cfg(test)]
pub(crate) fn shown_supersample(ctx: &egui::Context) -> Option<usize> {
    ctx.data(|data| data.get_temp::<RenderedPreview>(egui::Id::unique("projection_preview"))).map(|rendered| rendered.supersample)
}

/// The projection the preview renders: a planar placed on the model shows straight onto the sphere's front as big against the sphere as its frame is against the mesh, stretched, turned round its normal and slid across its plane as the frame is, cropped to its rectangle
fn shown_projection(projection: &Projection, bounds: (Vec3, Vec3)) -> Option<Projection> {
    match projection {
        Projection::Planar { placed: Some(_), uv, .. } => {
            let landed = folded(projection)?;
            let Projection::Planar { placed: Some(frame), projector, .. } = &landed else { return None };
            // The sphere stands for the model: its radius is the model's largest half size
            let radius = ((bounds.1 - bounds.0) * 0.5).max_element().max(1e-4);
            let half = frame.size.map(|side| (SPHERE_RADIUS * side / (2.0 * radius)).max(1e-4));
            let turn = upright_turn(frame).unwrap_or(0.0);
            // Slid across its own plane away from the mesh's centre: off the sphere's middle by as much
            let (origin, across, up, _) = decal_axes(frame)?;
            let from_center = (origin - (bounds.0 + bounds.1) * 0.5) / radius * SPHERE_RADIUS;
            let offset = [-from_center.dot(across) / half[0], -from_center.dot(up) / half[1], 0.0];
            let projector = Projector { rotation: [0.0, 0.0, turn], scale: [half[0], half[1], 1.0], crop: ShapeCrop::Cropped, ..*projector };
            offset.iter().all(|value| value.is_finite()).then_some(Projection::Planar { scale: 1.0, axis: 2, offset, uv: *uv, projector, placed: None })
        }
        Projection::Planar { .. } | Projection::Spherical { .. } | Projection::Cylindrical { .. } | Projection::Triplanar { .. } => Some(projection.clone()),
        Projection::Warp { .. } | Projection::Uv { .. } | Projection::UvTile => None,
    }
}

/// The projector `shown` round `shape` back at Offset 0, and how far the view reaches to fit it there: sliding the projector never zooms
fn resting_fit(shown: &Projection, shape: PreviewShape) -> (Option<ProjectorOutline>, f32) {
    let resting = ProjectorOutline::of(&at_rest(shown), shape.half_size());
    let half = view_half(resting.as_ref());
    (resting, half)
}

/// `projection` with its projector back at Offset 0 (its turn and size kept)
fn at_rest(projection: &Projection) -> Projection {
    let mut resting = projection.clone();
    match &mut resting {
        Projection::Planar { offset, .. }
        | Projection::Triplanar { offset, .. }
        | Projection::Spherical { offset, .. }
        | Projection::Cylindrical { offset, .. } => {
            *offset = [0.0; 3];
        }
        Projection::Warp { .. } | Projection::Uv { .. } | Projection::UvTile => {}
    }
    resting
}

/// Which way the preview camera looks
pub(crate) fn view_direction() -> Vec3 {
    View::new().forward
}

/// How far the view reaches each way from the middle so the shape and the projector `outline` both fit: never nearer than today's framing, never so far the shape drops below half its size
fn view_half(outline: Option<&ProjectorOutline>) -> f32 {
    let view = View::new();
    let reach =
        outline.into_iter().flat_map(ProjectorOutline::points).map(|point| point.dot(view.right).abs().max(point.dot(view.up).abs())).fold(0.0, f32::max);
    if reach.is_finite() { (reach * FIT_MARGIN).clamp(VIEW_HALF, MAX_VIEW_HALF) } else { MAX_VIEW_HALF }
}

/// The outline's line in the text colour and the field-coloured line under it that keeps it readable on the checker, as the 3D view's gizmo draws its white outline on a dark shadow (the square's field is light in the light themes); fainter where the shape hides it
fn outline_strokes(hidden: bool, tokens: &Tokens) -> [Stroke; 2] {
    let (under, over) = if hidden { (0.35, 0.65) } else { (0.55, 1.0) };
    [Stroke::new(2.5, tokens.field.gamma_multiply(under)), Stroke::new(1.2, tokens.text.gamma_multiply(over))]
}

/// The projector outline over the preview: solid where the camera sees it, dashed where the shape hides it, a dot where a planar's image travels to
fn draw_outline(painter: &Painter, screen: &Screen, shape: PreviewShape, outline: &ProjectorOutline, tokens: &Tokens) {
    for line in &outline.lines {
        // Runs of the line that are all seen or all hidden
        let mut runs: Vec<(bool, Vec<Pos2>)> = Vec::new();
        for pair in line.windows(2) {
            let [from, to] = [pair[0], pair[1]];
            let pieces = ((screen.at(to) - screen.at(from)).length() / OUTLINE_STEP).ceil().clamp(1.0, 256.0) as usize;
            for piece in 0..pieces {
                let (start, end) = (from.lerp(to, piece as f32 / pieces as f32), from.lerp(to, (piece + 1) as f32 / pieces as f32));
                let hidden = screen.hidden(shape, start.lerp(end, 0.5));
                match runs.last_mut() {
                    Some((run_hidden, points)) if *run_hidden == hidden => points.push(screen.at(end)),
                    _ => runs.push((hidden, vec![screen.at(start), screen.at(end)])),
                }
            }
        }
        for (hidden, points) in runs {
            for stroke in outline_strokes(hidden, tokens) {
                if hidden {
                    painter.extend(Shape::dashed_line(&points, stroke, HIDDEN_DASH, HIDDEN_DASH));
                } else {
                    painter.add(Shape::line(points.clone(), stroke));
                }
            }
        }
    }
    if let Some(tip) = outline.tip {
        let [under, over] = outline_strokes(false, tokens);
        painter.circle_filled(screen.at(tip), TIP_DOT + 0.8, under.color);
        painter.circle_filled(screen.at(tip), TIP_DOT, over.color);
    }
}

/// The sides of `rect` some of `points` lie past: left, right, top, bottom
pub(crate) fn passed_sides(points: impl Iterator<Item = Pos2>, rect: Rect) -> [bool; 4] {
    let past = 0.5;
    points.fold([false; 4], |[left, right, top, bottom], p| {
        [left || p.x < rect.left() - past, right || p.x > rect.right() + past, top || p.y < rect.top() - past, bottom || p.y > rect.bottom() + past]
    })
}

/// A dashed line just inside each side of `rect` the projector runs past, with a chevron pointing out at its middle
pub(crate) fn edge_markers(painter: &Painter, rect: Rect, sides: [bool; 4], tokens: &Tokens) {
    let inner = rect.shrink(EDGE_INSET);
    let center = rect.center();
    // Per side: its two ends, the chevron's point and the way into the square
    let edges = [
        ([inner.left_top(), inner.left_bottom()], pos2(inner.left(), center.y), vec2(1.0, 0.0)),
        ([inner.right_top(), inner.right_bottom()], pos2(inner.right(), center.y), vec2(-1.0, 0.0)),
        ([inner.left_top(), inner.right_top()], pos2(center.x, inner.top()), vec2(0.0, 1.0)),
        ([inner.left_bottom(), inner.right_bottom()], pos2(center.x, inner.bottom()), vec2(0.0, -1.0)),
    ];
    for (passed, (ends, point, inward)) in sides.into_iter().zip(edges) {
        if !passed {
            continue;
        }
        let along = vec2(inward.y, inward.x);
        let chevron = vec![point + (inward - along) * CHEVRON_ARM, point, point + (inward + along) * CHEVRON_ARM];
        for stroke in outline_strokes(false, tokens) {
            painter.extend(Shape::dashed_line(&ends, stroke, EDGE_DASH, EDGE_DASH));
        }
        // The chevron on a clear patch, so the dashes and the shape do not run through it
        let patch = Rect::from_points(&chevron).expand(1.5);
        painter.rect_filled(patch, 0, tokens.field);
        for stroke in outline_strokes(false, tokens) {
            painter.add(Shape::line(chevron.clone(), stroke));
        }
    }
}

fn shape_for(projection: &Projection) -> PreviewShape {
    match projection {
        Projection::Cylindrical { .. } => PreviewShape::Cylinder,
        Projection::Triplanar { .. } => PreviewShape::Cube,
        _ => PreviewShape::Sphere,
    }
}

/// `side²` pixels of the checker on the projection's shape over the `field` background, the view `half` mesh radii across each way from the middle, `supersample²` rays per pixel cast on every core
fn render(projection: &Projection, side: usize, half: f32, supersample: usize, tokens: &Tokens) -> ColorImage {
    let shape = shape_for(projection);
    let View { eye, forward, right, up } = View::new();
    let light = (-0.5 * right + 0.6 * up + 0.6 * eye).normalize_or_zero();

    let field = tokens.field;
    let (background, text) = (linear(field), linear(tokens.text));
    let shades = [CHECKER_DARK, CHECKER_LIGHT, UNCOVERED].map(|t| background.lerp(text, t));
    let supersample = supersample.max(1);
    let rays = side * supersample;
    // Per ray: the lit colour and how much of the projection lands there (None = the background)
    let samples: Vec<Option<(Vec3, f32)>> = (0..rays * rays)
        .into_par_iter()
        .map(|ray| {
            let (x, y) = (ray % rays, ray / rays);
            let across = ((x as f32 + 0.5) / rays as f32 * 2.0 - 1.0) * half;
            let down = ((y as f32 + 0.5) / rays as f32 * 2.0 - 1.0) * half;
            let origin = right * across - up * down + eye * CAMERA_DISTANCE;
            let hit = intersect(shape, origin, forward)?;
            let shade = AMBIENT + (1.0 - AMBIENT) * hit.nrm.dot(light).max(0.0);
            let (color, cover) = surface(projection, &hit, shades);
            Some((color * shade, cover))
        })
        .collect();

    let edge = linear(tokens.accent);
    let mut image = ColorImage::new([side, side], vec![field; side * side]);
    image.pixels.par_iter_mut().enumerate().for_each(|(pixel, out)| {
        let (x, y) = (pixel % side, pixel / side);
        let mut sum = Vec3::ZERO;
        for sub in 0..supersample * supersample {
            let (rx, ry) = (x * supersample + sub % supersample, y * supersample + sub / supersample);
            let color = match samples.get(ry * rays + rx).copied().flatten() {
                Some((color, cover)) if on_edge(&samples, rays, rx, ry, cover) => edge.lerp(color, 0.5),
                Some((color, _)) => color,
                None => background,
            };
            sum += color;
        }
        *out = from_linear(sum / (supersample * supersample) as f32);
    });
    image
}

/// Where the projection's reach ends on the shape: covered here, not on a neighbouring ray of the shape
fn on_edge(samples: &[Option<(Vec3, f32)>], rays: usize, x: usize, y: usize, cover: f32) -> bool {
    if cover <= 0.5 {
        return false;
    }
    let neighbours = [(x.wrapping_sub(1), y), (x + 1, y), (x, y.wrapping_sub(1)), (x, y + 1)];
    neighbours.into_iter().any(|(nx, ny)| nx < rays && ny < rays && samples.get(ny * rays + nx).copied().flatten().is_some_and(|(_, cover)| cover <= 0.5))
}

/// The unlit colour at a hit and how much of the projection reaches it, from the dark and light checker and the uncovered shape `shades` in linear light
fn surface(projection: &Projection, hit: &Coords, shades: [Vec3; 3]) -> (Vec3, f32) {
    let cover = projector_cover(projection, hit).clamp(0.0, 1.0);
    let mut light = 0.0;
    for (uv, weight) in image_samples(projection, hit) {
        let cell = (uv * CHECKER_CELLS).floor();
        let even = (cell.x + cell.y).rem_euclid(2.0) < 0.5;
        light += weight * f32::from(u8::from(even));
    }
    let [dark, bright, uncovered] = shades;
    (uncovered.lerp(dark.lerp(bright, light.clamp(0.0, 1.0)), cover), cover)
}

/// The nearest hit of the ray from `origin` along unit `dir` on the preview shape
fn intersect(shape: PreviewShape, origin: Vec3, dir: Vec3) -> Option<Coords> {
    match shape {
        PreviewShape::Sphere => {
            let b = origin.dot(dir);
            let c = origin.length_squared() - SPHERE_RADIUS * SPHERE_RADIUS;
            let disc = b * b - c;
            if disc < 0.0 {
                return None;
            }
            let pos = origin + dir * (-b - disc.sqrt());
            Some(hit(pos, pos.normalize_or_zero()))
        }
        PreviewShape::Cube => {
            let (mut near, mut far, mut nrm) = (f32::NEG_INFINITY, f32::INFINITY, Vec3::ZERO);
            for axis in 0..3 {
                let (o, d) = (origin[axis], dir[axis]);
                if d.abs() < 1e-6 {
                    if o.abs() > CUBE_HALF {
                        return None;
                    }
                    continue;
                }
                let (t0, t1) = ((-CUBE_HALF - o) / d, (CUBE_HALF - o) / d);
                let (enter, exit) = (t0.min(t1), t0.max(t1));
                if enter > near {
                    near = enter;
                    nrm = Vec3::ZERO;
                    nrm[axis] = -d.signum();
                }
                far = far.min(exit);
            }
            (near <= far && near.is_finite()).then(|| hit(origin + dir * near, nrm))
        }
        PreviewShape::Cylinder => {
            let mut best: Option<(f32, Coords)> = None;
            let mut keep = |t: f32, hit: Coords| {
                if t > 0.0 && best.as_ref().is_none_or(|(best_t, _)| t < *best_t) {
                    best = Some((t, hit));
                }
            };
            // The side: x² + z² = r² within the height
            let (a, b) = (dir.x * dir.x + dir.z * dir.z, origin.x * dir.x + origin.z * dir.z);
            let c = origin.x * origin.x + origin.z * origin.z - CYLINDER_RADIUS * CYLINDER_RADIUS;
            let disc = b * b - a * c;
            if a > 1e-6 && disc >= 0.0 {
                let t = (-b - disc.sqrt()) / a;
                let pos = origin + dir * t;
                if pos.y.abs() <= CYLINDER_HALF_HEIGHT {
                    keep(t, hit(pos, Vec3::new(pos.x, 0.0, pos.z).normalize_or_zero()));
                }
            }
            // The caps
            if dir.y.abs() > 1e-6 {
                for sign in [1.0, -1.0] {
                    let t = (sign * CYLINDER_HALF_HEIGHT - origin.y) / dir.y;
                    let pos = origin + dir * t;
                    if pos.x * pos.x + pos.z * pos.z <= CYLINDER_RADIUS * CYLINDER_RADIUS {
                        keep(t, hit(pos, Vec3::Y * sign));
                    }
                }
            }
            best.map(|(_, hit)| hit)
        }
    }
}

/// A shape point the camera sees, in the compositor's coordinates (no texture coordinates)
fn hit(pos: Vec3, nrm: Vec3) -> Coords {
    Coords { uv: Vec2::ZERO, pos, nrm }
}

fn linear(color: Color32) -> Vec3 {
    let rgba = egui::Rgba::from(color);
    Vec3::new(rgba.r(), rgba.g(), rgba.b())
}

fn from_linear(color: Vec3) -> Color32 {
    egui::Rgba::from_rgb(color.x, color.y, color.z).into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use texturecraft_engine::core::{Projector, UvTransform};

    fn tokens() -> Tokens {
        Tokens::for_brightness(crate::theme::Brightness::MediumDark)
    }

    fn spherical(tiling: f32) -> Projection {
        Projection::Spherical {
            scale: 1.0,
            uv: UvTransform { tiling: [tiling; 2], ..UvTransform::default() },
            offset: [0.0; 3],
            projector: Projector::default(),
        }
    }

    fn cylindrical(projector: Projector) -> Projection {
        Projection::Cylindrical { scale: 1.0, axis: 1, uv: UvTransform::default(), offset: [0.0; 3], projector, angle: 360.0 }
    }

    #[test]
    fn the_shape_fills_the_middle_and_leaves_the_corners_to_the_background() {
        let tokens = tokens();
        for projection in [
            spherical(1.0),
            cylindrical(Projector::default()),
            Projection::Triplanar { scale: 1.0, sharpness: 8.0, offset: [0.0; 3], uv: UvTransform::default(), projector: Projector::default() },
        ] {
            let image = render(&projection, 32, VIEW_HALF, SUPERSAMPLE, &tokens);
            assert_eq!(image.pixels[0], tokens.field, "{projection:?}: the corner is background");
            assert_ne!(image.pixels[16 * 32 + 16], tokens.field, "{projection:?}: the middle shows the shape");
        }
    }

    #[test]
    fn the_preview_follows_the_fill_settings() {
        let tokens = tokens();
        assert_ne!(
            render(&spherical(1.0), 32, VIEW_HALF, SUPERSAMPLE, &tokens).pixels,
            render(&spherical(3.0), 32, VIEW_HALF, SUPERSAMPLE, &tokens).pixels,
            "tiling changes the checker"
        );

        let culled =
            Projector { backface: texturecraft_engine::core::Backface { on: true, ..Default::default() }, crop: ShapeCrop::Cropped, ..Projector::default() };
        assert_ne!(
            render(&cylindrical(Projector::default()), 32, VIEW_HALF, SUPERSAMPLE, &tokens).pixels,
            render(&cylindrical(culled), 32, VIEW_HALF, SUPERSAMPLE, &tokens).pixels,
            "culling shows on the cylinder"
        );
    }

    #[test]
    fn projections_without_a_shape_have_no_preview() {
        assert!(shown_projection(&Projection::UvTile, BOUNDS).is_none());
        assert!(shown_projection(&Projection::Uv { scale: [1.0; 2], rotation: 0.0, offset: [0.0; 2] }, BOUNDS).is_none());
        assert!(shown_projection(&spherical(1.0), BOUNDS).is_some());
    }

    const BOUNDS: (Vec3, Vec3) = (Vec3::NEG_ONE, Vec3::ONE);

    fn shown(projection: &Projection) -> Vec<Color32> {
        let shown = shown_projection(projection, BOUNDS).unwrap();
        let half = view_half(ProjectorOutline::of(&shown, shape_for(&shown).half_size()).as_ref());
        render(&shown, 32, half, SUPERSAMPLE, &tokens()).pixels
    }

    /// What the 3D view's gizmo changes shows here too: a sphere's or cylinder's size, turn and centre, a placed frame's size, stretch and turn
    #[test]
    fn the_preview_shows_the_projectors_size_turn_and_place() {
        let base = shown(&spherical(1.0));
        let bigger = Projector { scale: [1.15; 3], ..Projector::default() };
        let Projection::Spherical { uv, .. } = spherical(1.0) else { panic!("spherical() makes a spherical projection") };
        let sphere = |projector: Projector, offset: [f32; 3]| Projection::Spherical { scale: 1.0, uv, offset, projector };
        assert_ne!(shown(&sphere(bigger, [0.0; 3])), base, "a bigger sphere zooms the view out");
        assert_ne!(shown(&sphere(Projector { rotation: [0.0, 0.0, 40.0], ..Projector::default() }, [0.0; 3])), base, "a turned sphere turns its checker");
        assert_ne!(shown(&sphere(Projector::default(), [0.3, 0.0, 0.0])), base, "an offset moves it");
        assert_eq!(shown(&sphere(Projector::default(), [0.0; 3])), base, "unmoved: as before");
        assert_ne!(
            shown(&cylindrical(Projector { scale: [1.3, 1.0, 1.3], ..Projector::default() })),
            shown(&cylindrical(Projector::default())),
            "a wider cylinder"
        );

        let frame = texturecraft_engine::core::Decal {
            center: [0.0, 0.0, 1.0],
            normal: [0.0, 0.0, 1.0],
            up: [0.0, 1.0, 0.0],
            size: [1.0, 1.0],
            rotation: 0.0,
            offset: [0.0; 2],
            depth: 1.0,
        };
        let placed = |frame| Projection::decal(frame);
        let square = shown(&placed(frame));
        assert_ne!(shown(&placed(texturecraft_engine::core::Decal { size: [0.5, 0.5], ..frame })), square, "a smaller frame");
        assert_ne!(shown(&placed(texturecraft_engine::core::Decal { size: [1.5, 1.0], ..frame })), square, "a stretched frame");
        assert_ne!(shown(&placed(texturecraft_engine::core::Decal { rotation: 30.0, ..frame })), square, "a turned frame");
        assert_ne!(shown(&placed(texturecraft_engine::core::Decal { offset: [0.3, 0.0], ..frame })), square, "a slid frame");
    }

    fn fitted_half(projection: &Projection) -> f32 {
        let shown = shown_projection(projection, BOUNDS).unwrap();
        resting_fit(&shown, shape_for(&shown)).1
    }

    fn sized(scale: f32) -> Projection {
        let Projection::Spherical { uv, .. } = spherical(1.0) else { panic!("spherical() makes a spherical projection") };
        Projection::Spherical { scale: 1.0, uv, offset: [0.0; 3], projector: Projector { scale: [scale; 3], ..Projector::default() } }
    }

    /// The view zooms out until the projector fits, never nearer than the plain framing and never past half the shape's size; past that the projector runs off the square and each side it passes is marked
    #[test]
    fn the_view_fits_the_projector_down_to_half_the_shape() {
        assert_eq!(fitted_half(&sized(0.4)), VIEW_HALF, "a small projector keeps the plain framing");
        let default = fitted_half(&sized(1.0));
        assert!(default > VIEW_HALF && default < MAX_VIEW_HALF, "{default}");
        assert_eq!(fitted_half(&sized(3.0)), MAX_VIEW_HALF, "x3 stops at half the shape's size");

        let rect = Rect::from_min_size(Pos2::ZERO, vec2(100.0, 100.0));
        let sides = |scale: f32| {
            let shown = sized(scale);
            let outline = ProjectorOutline::of(&shown, PreviewShape::Sphere.half_size()).unwrap();
            let screen = Screen { view: View::new(), rect, half: fitted_half(&shown) };
            passed_sides(outline.points().map(|point| screen.at(point)), rect)
        };
        assert_eq!(sides(1.0), [false; 4], "a fitted projector stays inside the square");
        assert_eq!(sides(3.0), [true; 4], "an x3 sphere runs past every side");

        let triplanar = Projection::Triplanar {
            scale: 1.0,
            sharpness: 8.0,
            offset: [0.0; 3],
            uv: UvTransform::default(),
            projector: Projector { scale: [10.0; 3], ..Projector::default() },
        };
        assert_eq!(fitted_half(&triplanar), MAX_VIEW_HALF, "every kind fits the same way");
    }

    /// Sliding a projector never zooms the view: it fits the projector at Offset 0, the ring stays in the middle and the projector's centre slides away from it
    #[test]
    fn the_offset_slides_the_projector_without_zooming_the_view() {
        let rect = Rect::from_min_size(Pos2::ZERO, vec2(100.0, 100.0));
        let Projection::Spherical { scale, uv, projector, .. } = sized(1.0) else { panic!("sized() makes a spherical projection") };
        for offset in [[0.0, 0.0, 0.0], [0.3, 0.0, 0.0], [1.5, 0.8, 0.0]] {
            let slid = Projection::Spherical { scale, uv, offset, projector };
            assert_eq!(fitted_half(&slid), fitted_half(&sized(1.0)), "Offset {offset:?} keeps the view");
            let (resting, half) = resting_fit(&slid, PreviewShape::Sphere);
            let screen = Screen { view: View::new(), rect, half };
            let ring = screen.at(resting.unwrap().center);
            let dot = screen.at(ProjectorOutline::of(&slid, PreviewShape::Sphere.half_size()).unwrap().center);
            assert!(ring.distance(rect.center()) < 1e-3, "the ring marks the middle: {ring:?}");
            assert_eq!(dot.distance(ring) > 1.0, offset != [0.0; 3], "the filled dot leaves the ring only when slid: {dot:?}");
        }
    }

    /// A huge spherical projector no longer fills the square with its own ball: the shape keeps half its size in the middle with background round it
    #[test]
    fn a_huge_sphere_keeps_the_shape_in_the_square() {
        let tokens = tokens();
        let image = render(&sized(3.0), 32, fitted_half(&sized(3.0)), SUPERSAMPLE, &tokens);
        assert_eq!(image.pixels[0], tokens.field, "the corner is background");
        assert_eq!(image.pixels[16 * 32 + 4], tokens.field, "the shape is half its size: its side is background");
        assert_ne!(image.pixels[16 * 32 + 16], tokens.field, "the shape is in the middle");
    }

    /// The outline is dashed where the shape hides it: the equator's back half runs behind the ball
    #[test]
    fn the_shape_hides_the_back_of_the_outline() {
        let screen = Screen { view: View::new(), rect: Rect::from_min_size(Pos2::ZERO, vec2(100.0, 100.0)), half: 1.2 };
        let outline = ProjectorOutline::of(&sized(1.0), PreviewShape::Sphere.half_size()).unwrap();
        let hidden = |line: &Vec<Vec3>| line.iter().filter(|point| screen.hidden(PreviewShape::Sphere, **point)).count();
        assert_eq!(hidden(&outline.lines[0]), 0, "the rim is all seen");
        let equator = hidden(&outline.lines[1]);
        assert!(equator > 0 && equator < outline.lines[1].len(), "the equator is half hidden: {equator}");
    }

    /// Every kind at x0.4, x1, x2, x3 and x10 (projector scale), as the panel's square draws it
    #[test]
    #[ignore = "requires an offscreen GPU adapter and TC_UI_SNAPSHOTS"]
    fn preview_fit_snapshots() {
        let _language = crate::i18n::TEST_LANGUAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        use crate::theme::Brightness;
        let Some(directory) = std::env::var_os("TC_UI_SNAPSHOTS").map(std::path::PathBuf::from) else { return };
        std::fs::create_dir_all(&directory).unwrap();
        const SIZES: [f32; 5] = [0.4, 1.0, 2.0, 3.0, 10.0];
        const SQUARE: f32 = 104.0;
        const MARGIN: f32 = 4.0;
        type Kind = (&'static str, fn(f32) -> Projection);
        let kinds: [Kind; 4] = [
            ("planar", |s| Projection::Planar {
                scale: 1.0,
                axis: 2,
                offset: [0.0; 3],
                uv: UvTransform::default(),
                projector: Projector { scale: [s, s, 1.0], ..Projector::default() },
                placed: None,
            }),
            ("spherical", sized),
            ("cylindrical", |s| cylindrical(Projector { scale: [s; 3], ..Projector::default() })),
            ("triplanar", |s| Projection::Triplanar {
                scale: 1.0,
                sharpness: 8.0,
                offset: [0.0; 3],
                uv: UvTransform::default(),
                projector: Projector { scale: [s; 3], ..Projector::default() },
            }),
        ];
        // One square per picture: the preview keeps one rendered texture. The medium theme at every size, the others at two
        let themes = [(Brightness::MediumDark, "", &SIZES[..]), (Brightness::Dark, "-dark", &[1.0, 3.0][..]), (Brightness::Light, "-light", &[1.0, 3.0][..])];
        for ((brightness, theme, sizes), (name, make)) in themes.into_iter().flat_map(|theme| kinds.map(|kind| (theme, kind))) {
            for &scale in sizes {
                let projection = make(scale);
                let rect = Rect::from_min_size(pos2(MARGIN, MARGIN), vec2(SQUARE, SQUARE));
                let mut harness =
                    egui_kittest::Harness::builder().with_size(vec2(SQUARE + 2.0 * MARGIN, SQUARE + 2.0 * MARGIN)).with_pixels_per_point(2.0).wgpu().build_ui(
                        move |ui| {
                            let tokens = Tokens::for_brightness(brightness);
                            ui.painter().rect_filled(ui.ctx().content_rect(), 0, tokens.panel);
                            projection_preview(ui.painter(), rect, &projection, BOUNDS, &tokens);
                        },
                    );
                harness.run_steps(2);
                harness.render().unwrap().save(directory.join(format!("preview-fit-{name}-x{scale}{theme}.png"))).unwrap();
            }
        }
        // In the panel: a x3 sphere zoomed out to its floor, marked on every side
        let (mut harness, _) = crate::frame_gizmo::tests::fill_app_rendered(serde_json::json!({"projection": "spherical", "projectorScale": [3, 3, 3]}), true);
        harness.run_steps(4);
        harness.render().unwrap().save(directory.join("preview-fit-in-panel-spherical-x3.png")).unwrap();
    }

    /// One frame of `projection`'s preview square, with `events` as the frame's input
    fn preview_frame(ctx: &egui::Context, projection: &Projection, events: Vec<egui::Event>) {
        let tokens = tokens();
        let input = egui::RawInput { events, ..Default::default() };
        let mut output = ctx.run_ui(input, |ui| {
            projection_preview(ui.painter(), Rect::from_min_size(pos2(0.0, 0.0), vec2(64.0, 64.0)), projection, BOUNDS, &tokens);
        });
        // No renderer takes the uploads here
        output.textures_delta.clear();
    }

    #[test]
    fn a_drag_renders_the_square_lighter_and_the_release_renders_it_in_full() {
        let ctx = egui::Context::default();
        let at = pos2(10.0, 10.0);
        let button = |pressed| egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
        preview_frame(&ctx, &spherical(1.0), Vec::new());
        assert_eq!(shown_supersample(&ctx), Some(SUPERSAMPLE), "at rest: in full");

        preview_frame(&ctx, &spherical(1.0), vec![egui::Event::PointerMoved(at), button(true)]);
        assert_eq!(shown_supersample(&ctx), Some(SUPERSAMPLE), "a press alone keeps the full picture: nothing changed");
        preview_frame(&ctx, &spherical(2.0), Vec::new());
        assert_eq!(shown_supersample(&ctx), Some(DRAG_SUPERSAMPLE), "a change while held renders lighter");
        let dragged = shown_key(&ctx);

        preview_frame(&ctx, &spherical(2.0), vec![button(false)]);
        assert_eq!(shown_supersample(&ctx), Some(SUPERSAMPLE), "let go: the same picture in full");
        assert_eq!(shown_key(&ctx), dragged);
    }

    /// Render times of the preview square (prints ms per render at its largest side; run with --ignored --nocapture)
    #[test]
    #[ignore]
    fn preview_render_timings() {
        let tokens = tokens();
        let kinds = [
            ("sphere", spherical(2.0)),
            ("cylinder", cylindrical(Projector::default())),
            ("cube", Projection::Triplanar { scale: 1.0, sharpness: 8.0, offset: [0.0; 3], uv: UvTransform::default(), projector: Projector::default() }),
        ];
        for (name, projection) in kinds {
            let start = std::time::Instant::now();
            const RUNS: u32 = 10;
            for _ in 0..RUNS {
                std::hint::black_box(render(&projection, MAX_SIDE_PX, VIEW_HALF, SUPERSAMPLE, &tokens));
            }
            let full = start.elapsed().as_secs_f64() * 1000.0 / f64::from(RUNS);
            let start = std::time::Instant::now();
            for _ in 0..RUNS {
                std::hint::black_box(render(&projection, MAX_SIDE_PX, VIEW_HALF, DRAG_SUPERSAMPLE, &tokens));
            }
            let dragged = start.elapsed().as_secs_f64() * 1000.0 / f64::from(RUNS);
            println!("preview {name}: {full:.1} ms at rest, {dragged:.1} ms while dragged");
        }
    }
}
