//! The preview square of a 3D projection: a checker laid on a sphere, cylinder or cube through the fill's own projection
//!
//! Each pixel casts a ray at the shape and samples the checker with the compositor's projection math
//! ([`image_samples`] and [`projector_cover`]), so tiling, offset, rotation, Hardness, Angle, the 3D projection
//! settings, Shape crop and Backface culling show as they will on the model. Depth culling needs the model's depth
//! map and is not previewed

use egui::{Color32, ColorImage, Painter, Rect, TextureHandle, TextureOptions, pos2};
use texturecraft_engine::compose::frame_placement::{folded, upright_turn};
use texturecraft_engine::compose::pattern::{Coords, decal_axes, image_samples};
use texturecraft_engine::compose::projector_cover::projector_cover;
use texturecraft_engine::core::{Projection, Projector, ShapeCrop};
use texturecraft_engine::geom::glam::{EulerRot, Mat3, Vec2, Vec3};

use crate::theme::Tokens;

/// Largest preview side in pixels; the square is never drawn bigger than this
const MAX_SIDE_PX: usize = 256;
/// Rays per pixel along each axis (smooth outline)
const SUPERSAMPLE: usize = 2;
/// Checker cells across one repeat of the source along each axis
const CHECKER_CELLS: f32 = 4.0;
/// Half the view's width and height, in mesh radii
const VIEW_HALF: f32 = 1.0;
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

/// What the preview wraps the checker around
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PreviewShape {
    Sphere,
    Cylinder,
    Cube,
}

/// Draw the preview of `projection` on a mesh within `bounds` into `rect`, rendered once per projection, size and theme (None for projections without one: UV, warp)
pub(crate) fn projection_preview(painter: &Painter, rect: Rect, projection: &Projection, bounds: (Vec3, Vec3), tokens: &Tokens) -> Option<()> {
    let shown = shown_projection(projection, bounds)?;
    let ctx = painter.ctx();
    let side = ((rect.width() * ctx.pixels_per_point()).round() as usize).clamp(1, MAX_SIDE_PX);
    let key = format!("{side}{shown:?}{:?}{:?}{:?}", tokens.field, tokens.text, tokens.accent);
    let id = egui::Id::unique("projection_preview");
    // The picture and the key it was rendered for
    let cached = ctx.data(|data| data.get_temp::<(String, TextureHandle)>(id)).filter(|(rendered, _)| *rendered == key);
    let texture = match cached {
        Some((_, texture)) => texture,
        None => {
            let image = render(&shown, side, tokens);
            let texture = ctx.load_texture("projection-preview", image, TextureOptions::LINEAR);
            ctx.data_mut(|data| data.insert_temp(id, (key, texture.clone())));
            texture
        }
    };
    painter.image(texture.id(), rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
    painter.rect_stroke(rect, 0, egui::Stroke::new(1.0, tokens.field_border), egui::StrokeKind::Inside);
    Some(())
}

/// What the preview square last rendered (tests: a drag re-renders it)
#[cfg(test)]
pub(crate) fn shown_key(ctx: &egui::Context) -> Option<String> {
    ctx.data(|data| data.get_temp::<(String, TextureHandle)>(egui::Id::unique("projection_preview"))).map(|(key, _)| key)
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

/// Where a spherical or cylindrical projection's own shape sits: the preview draws its projector (turned, stretched and moved by the 3D projection settings, so its size dot and fields show) rather than a fixed shape its turn-and-scale-blind mapping would look the same on
#[derive(Clone, Copy, Debug)]
struct ShapePlace {
    turn: Mat3,
    stretch: Vec3,
    offset: Vec3,
}

impl ShapePlace {
    fn of(projection: &Projection) -> ShapePlace {
        let still = ShapePlace { turn: Mat3::IDENTITY, stretch: Vec3::ONE, offset: Vec3::ZERO };
        let (Projection::Spherical { offset, projector, .. } | Projection::Cylindrical { offset, projector, .. }) = projection else { return still };
        let [x, y, z] = projector.rotation.map(f32::to_radians);
        let stretch = Vec3::from(projector.scale).map(|s| if s.abs() < 1e-6 { 1e-6 } else { s });
        let place = ShapePlace { turn: Mat3::from_euler(EulerRot::ZYX, z, y, x), stretch, offset: Vec3::from(*offset) };
        if place.turn.is_finite() && place.stretch.is_finite() && place.offset.is_finite() { place } else { still }
    }

    /// The nearest hit of the ray from `origin` along unit `dir` on the placed shape, in the compositor's coordinates
    fn hit(&self, shape: PreviewShape, origin: Vec3, dir: Vec3) -> Option<Coords> {
        // Into the shape's own space: unturned, unstretched, its centre where the offset puts the projection's
        let to_shape = |v: Vec3| (self.turn.transpose() * v) / self.stretch;
        let shape_dir = to_shape(dir).try_normalize()?;
        let at = intersect(shape, to_shape(origin) + self.offset, shape_dir)?;
        let pos = self.turn * (self.stretch * (at.pos - self.offset));
        let nrm = (self.turn * (at.nrm / self.stretch)).try_normalize().unwrap_or(at.nrm);
        Some(hit(pos, nrm))
    }
}

fn shape_for(projection: &Projection) -> PreviewShape {
    match projection {
        Projection::Cylindrical { .. } => PreviewShape::Cylinder,
        Projection::Triplanar { .. } => PreviewShape::Cube,
        _ => PreviewShape::Sphere,
    }
}

/// `side²` pixels of the checker on the projection's shape over the `field` background
fn render(projection: &Projection, side: usize, tokens: &Tokens) -> ColorImage {
    let shape = shape_for(projection);
    let place = ShapePlace::of(projection);
    let (yaw, pitch) = (CAMERA_YAW.to_radians(), CAMERA_PITCH.to_radians());
    let eye = Vec3::new(pitch.cos() * yaw.sin(), pitch.sin(), pitch.cos() * yaw.cos());
    let forward = -eye;
    let right = forward.cross(Vec3::Y).normalize_or_zero();
    let up = right.cross(forward);
    let light = (-0.5 * right + 0.6 * up + 0.6 * eye).normalize_or_zero();

    let field = tokens.field;
    let (background, text) = (linear(field), linear(tokens.text));
    let shades = [CHECKER_DARK, CHECKER_LIGHT, UNCOVERED].map(|t| background.lerp(text, t));
    let rays = side * SUPERSAMPLE;
    // Per ray: the lit colour and how much of the projection lands there (None = the background)
    let samples: Vec<Option<(Vec3, f32)>> = (0..rays * rays)
        .map(|ray| {
            let (x, y) = (ray % rays, ray / rays);
            let across = ((x as f32 + 0.5) / rays as f32 * 2.0 - 1.0) * VIEW_HALF;
            let down = ((y as f32 + 0.5) / rays as f32 * 2.0 - 1.0) * VIEW_HALF;
            let origin = right * across - up * down + eye * 3.0;
            let hit = place.hit(shape, origin, forward)?;
            let shade = AMBIENT + (1.0 - AMBIENT) * hit.nrm.dot(light).max(0.0);
            let (color, cover) = surface(projection, &hit, shades);
            Some((color * shade, cover))
        })
        .collect();

    let edge = linear(tokens.accent);
    let mut image = ColorImage::new([side, side], vec![field; side * side]);
    for (pixel, out) in image.pixels.iter_mut().enumerate() {
        let (x, y) = (pixel % side, pixel / side);
        let mut sum = Vec3::ZERO;
        for sub in 0..SUPERSAMPLE * SUPERSAMPLE {
            let (rx, ry) = (x * SUPERSAMPLE + sub % SUPERSAMPLE, y * SUPERSAMPLE + sub / SUPERSAMPLE);
            let color = match samples.get(ry * rays + rx).copied().flatten() {
                Some((color, cover)) if on_edge(&samples, rays, rx, ry, cover) => edge.lerp(color, 0.5),
                Some((color, _)) => color,
                None => background,
            };
            sum += color;
        }
        *out = from_linear(sum / (SUPERSAMPLE * SUPERSAMPLE) as f32);
    }
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
            let image = render(&projection, 32, &tokens);
            assert_eq!(image.pixels[0], tokens.field, "{projection:?}: the corner is background");
            assert_ne!(image.pixels[16 * 32 + 16], tokens.field, "{projection:?}: the middle shows the shape");
        }
    }

    #[test]
    fn the_preview_follows_the_fill_settings() {
        let tokens = tokens();
        assert_ne!(render(&spherical(1.0), 32, &tokens).pixels, render(&spherical(3.0), 32, &tokens).pixels, "tiling changes the checker");

        let culled =
            Projector { backface: texturecraft_engine::core::Backface { on: true, ..Default::default() }, crop: ShapeCrop::Cropped, ..Projector::default() };
        assert_ne!(
            render(&cylindrical(Projector::default()), 32, &tokens).pixels,
            render(&cylindrical(culled), 32, &tokens).pixels,
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
        render(&shown_projection(projection, BOUNDS).unwrap(), 32, &tokens()).pixels
    }

    /// What the 3D view's gizmo changes shows here too: a sphere's or cylinder's size, turn and centre, a placed frame's size, stretch and turn
    #[test]
    fn the_preview_shows_the_projectors_size_turn_and_place() {
        let base = shown(&spherical(1.0));
        let bigger = Projector { scale: [1.15; 3], ..Projector::default() };
        let Projection::Spherical { uv, .. } = spherical(1.0) else { panic!("spherical() makes a spherical projection") };
        let sphere = |projector: Projector, offset: [f32; 3]| Projection::Spherical { scale: 1.0, uv, offset, projector };
        assert_ne!(shown(&sphere(bigger, [0.0; 3])), base, "a bigger sphere fills more of the square");
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
}
