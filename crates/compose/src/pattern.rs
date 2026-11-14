//! Procedural patterns and projections.

use texturecraft_core::{Decal, Procedural, ProceduralKind, Projection, Projector, UvTransform};
use texturecraft_geom::glam::{EulerRot, Mat3, Vec2, Vec3};
use texturecraft_texel::noise;

/// Where a texel is, for pattern evaluation.
#[derive(Clone, Copy, Debug)]
pub struct Coords {
    pub uv: Vec2,
    /// World position divided by the scene radius (scale-independent).
    pub pos: Vec3,
    pub nrm: Vec3,
}

/// Raw pattern value at a 3D point, 0..1.
fn raw(kind: ProceduralKind, p: Vec3, seed: u32) -> f32 {
    let a = p.to_array();
    match kind {
        ProceduralKind::Clouds => noise::fbm(a, 6, seed, noise::value),
        ProceduralKind::Perlin => noise::fbm(a, 4, seed, noise::perlin),
        ProceduralKind::Noise => noise::value((p * 16.0).to_array(), seed),
        ProceduralKind::Cells => noise::worley(a, seed),
        ProceduralKind::Grunge => {
            let n = noise::fbm(a, 6, seed, noise::perlin) * 0.7 + noise::fbm((p * 3.7).to_array(), 3, seed ^ 0x55, noise::value) * 0.3;
            ((n - 0.45) * 4.0 + 0.5).clamp(0.0, 1.0)
        }
        ProceduralKind::Scratches => {
            // Long thin streaks: noise stretched along one axis, sharpened to lines.
            let q = Vec3::new(p.x * 0.15, p.y * 6.0, p.z * 6.0);
            let n = noise::perlin(q.to_array(), seed);
            let line = 1.0 - ((n - 0.5).abs() * 40.0).min(1.0);
            let gate = noise::value((p * 0.8).to_array(), seed ^ 0x99);
            line * ((gate - 0.45) * 4.0).clamp(0.0, 1.0)
        }
        ProceduralKind::Checker => {
            let c = p.floor();
            if (c.x + c.y + c.z).rem_euclid(2.0) < 1.0 { 1.0 } else { 0.0 }
        }
        ProceduralKind::Stripes => {
            let t = p.x.rem_euclid(1.0);
            if t < 0.5 { 1.0 } else { 0.0 }
        }
        ProceduralKind::Gradient => p.x.rem_euclid(1.0),
        ProceduralKind::Wood => {
            // Rings across Y (warped by low-frequency noise), latewood darkening sharply before
            // each ring boundary; fine fibers stretched along X.
            let warp = noise::fbm([p.x * 0.18, p.y * 0.6, p.z], 3, seed, noise::perlin);
            let t = ((p.y + (warp - 0.5) * 0.9) * 5.0).rem_euclid(1.0);
            let ring = 1.0 - smoothstep(0.45, 1.0, t);
            let fiber = noise::value([p.x * 2.0, p.y * 40.0, p.z], seed ^ 0x3C);
            let tone = noise::fbm([p.x * 0.6, p.y * 3.0, p.z], 2, seed ^ 0x71, noise::value);
            (ring * 0.6 + fiber * 0.25 + tone * 0.15).clamp(0.0, 1.0)
        }
        ProceduralKind::Bricks => {
            // Bricks 1 × 0.5 in running bond; per-brick tone in the top 15 % so low/high map to
            // mortar/brick colors without washing the bricks out.
            let row = (p.y * 2.0).floor();
            let x = p.x + if (row as i32).rem_euclid(2) == 1 { 0.5 } else { 0.0 };
            let (col, fx, fy) = (x.floor(), x.rem_euclid(1.0), (p.y * 2.0).rem_euclid(1.0));
            let d = fx.min(1.0 - fx).min(fy.min(1.0 - fy) * 0.5);
            let tone = noise::cell_hash(col as i32, row as i32, 0, seed) * 0.6 + noise::value((p * 9.0).to_array(), seed) * 0.4;
            smoothstep(0.02, 0.045, d) * (0.85 + 0.15 * tone)
        }
        ProceduralKind::Tiles => {
            let (c, f) = (p.floor(), p - p.floor());
            let d = f.x.min(1.0 - f.x).min(f.y.min(1.0 - f.y));
            smoothstep(0.015, 0.035, d) * (0.9 + 0.1 * noise::cell_hash(c.x as i32, c.y as i32, 0, seed))
        }
        ProceduralKind::Weave | ProceduralKind::Twill => {
            // Warp along X, weft along Y: in each cell the thread on top shows its round
            // cross-section, bulging mid-span. A plain weave alternates every cell; a 2/2 twill
            // floats over two cells and steps one cell per row, so the floats line up in diagonal
            // ribs, the weft darker than the warp.
            let c = (p * 2.0).floor();
            let f = p * 2.0 - c;
            let pi = std::f32::consts::PI;
            let twill = kind == ProceduralKind::Twill;
            let (period, warp_cells) = if twill { (4, 2) } else { (2, 1) };
            let warp_on_top = (c.x as i32).wrapping_add(c.y as i32).rem_euclid(period) < warp_cells;
            let (across, along) = if warp_on_top { (f.y, f.x) } else { (f.x, f.y) };
            let thread = (pi * across).sin().max(0.0).sqrt() * (0.8 + 0.2 * (pi * along).sin());
            let tone = if twill && !warp_on_top { 0.55 } else { 1.0 };
            let fiber = noise::value([p.x * 40.0, p.y * 40.0, p.z], seed);
            (thread * tone * (0.88 + 0.12 * fiber)).clamp(0.0, 1.0)
        }
        ProceduralKind::Leather => {
            let w = noise::perlin((p * 0.7).to_array(), seed ^ 0x2D) - 0.5;
            let (f1, f2, _) = noise::cellular([p.x + w * 0.4, p.y - w * 0.4, p.z], seed);
            let crease = smoothstep(0.0, 0.16, f2 - f1);
            let pebble = 1.0 - f1 * 0.5;
            let fine = noise::value((p * 7.0).to_array(), seed ^ 0x13);
            (crease * pebble * (0.85 + 0.15 * fine)).clamp(0.0, 1.0)
        }
        ProceduralKind::Hammered => {
            let dent = |q: Vec3, s: u32| {
                let (f1, _, _) = noise::cellular(q.to_array(), s);
                (f1 * 1.25).min(1.0).powi(2)
            };
            (dent(p, seed) * 0.75 + dent(p * 2.3 + Vec3::splat(5.1), seed ^ 0x5A) * 0.25).clamp(0.0, 1.0)
        }
        ProceduralKind::Brushed => {
            let a = noise::value([p.x * 1.5, p.y * 60.0, p.z], seed);
            let b = noise::value([p.x * 0.4, p.y * 18.0, p.z], seed ^ 0x77);
            (a * 0.6 + b * 0.4).clamp(0.0, 1.0)
        }
        ProceduralKind::Marble => {
            let turb = noise::fbm(a, 5, seed, noise::perlin);
            let veins = ((p.x + p.y * 0.5) * 2.5 + turb * 9.0).sin().abs().powf(0.3);
            let cloud = noise::fbm((p * 2.0).to_array(), 3, seed ^ 0x19, noise::value);
            (veins * 0.85 + cloud * 0.15).clamp(0.0, 1.0)
        }
        ProceduralKind::Voronoi => noise::cellular(a, seed).2,
        ProceduralKind::Streaks => {
            // Drips: thin columns along Y, each segment starting at a random height and fading
            // as it runs down (Y is up), over two offset column grids.
            let drip = |x: f32, salt: u32| {
                let cx = x * 6.0;
                let col = cx.floor() as i32;
                let h = |k: i32| noise::cell_hash(col, k, salt as i32, seed);
                let width = 0.08 + 0.25 * h(0);
                let across = 1.0 - smoothstep(width * 0.3, width, (cx - cx.floor() - 0.5 - (h(1) - 0.5) * 0.5).abs());
                let yy = p.y * (0.35 + 0.4 * h(2)) + h(3) * 7.0;
                let t = yy - yy.floor();
                let on = if noise::cell_hash(col, yy.floor() as i32, 3 + salt as i32, seed) > 0.5 { 1.0 } else { 0.0 };
                across * on * t.powf(1.6) * (1.0 - smoothstep(0.9, 1.0, t))
            };
            let fine = noise::value([p.x * 37.0, p.y * 1.3, p.z], seed ^ 0x2B);
            (drip(p.x, 0).max(drip(p.x + 0.37, 1) * 0.8) * (0.6 + 0.4 * fine)).clamp(0.0, 1.0)
        }
        ProceduralKind::Smudges => {
            // Sparse elliptical prints with concentric ridges, over soft greasy smudges.
            let w = noise::perlin((p * 1.3).to_array(), seed ^ 0x61) - 0.5;
            let q = Vec3::new(p.x * 1.15 + w * 0.3, p.y * 0.85 + w * 0.3, p.z);
            let (f1, _, id) = noise::cellular(q.to_array(), seed);
            let print = (1.0 - smoothstep(0.18 + 0.1 * id, 0.3 + 0.12 * id, f1)) * if id > 0.4 { 1.0 } else { 0.0 };
            let ridges = 0.5 + 0.5 * (f1 * 190.0 + w * 12.0).sin();
            let smudge = ((noise::fbm((p * 1.7).to_array(), 4, seed ^ 0x33, noise::value) - 0.55) * 3.0).clamp(0.0, 1.0);
            (print * ridges * 0.9 + smudge * 0.4).clamp(0.0, 1.0)
        }
        ProceduralKind::Pores => {
            let (f1, _, _) = noise::cellular(a, seed);
            let (g1, _, _) = noise::cellular((p * 2.7 + Vec3::splat(13.0)).to_array(), seed ^ 0x6B);
            smoothstep(0.08, 0.16, f1).min(smoothstep(0.06, 0.12, g1))
        }
        ProceduralKind::Chips => {
            let (f1, _, _) = noise::cellular((p * 2.0).to_array(), seed ^ 0x45);
            let n = noise::fbm(a, 5, seed, noise::perlin) + (f1 - 0.5) * 0.25;
            smoothstep(0.55, 0.58, n)
        }
    }
}

/// `values` raised to a triplanar `sharpness` (at least 1): whole sharpnesses up to 16 (the usual
/// ones) by repeated squaring, a few multiplies instead of three `powf` calls per texel.
#[inline]
fn sharpen(values: Vec3, sharpness: f32) -> Vec3 {
    const MAX_INTEGER_SHARPNESS: f32 = 16.0;
    let exponent = sharpness.max(1.0);
    if exponent > MAX_INTEGER_SHARPNESS || exponent.fract() != 0.0 {
        return values.powf(exponent);
    }

    let (mut remaining, mut base, mut result) = (exponent as u32, values, Vec3::ONE);
    while remaining > 0 {
        if remaining & 1 == 1 {
            result *= base;
        }
        base *= base;
        remaining >>= 1;
    }

    result
}

/// A triplanar plane whose weight, times the gain applied after blending, stays under this is
/// not evaluated: even two skipped planes change a value by less than 1/65536, under the u16
/// step layers are stored at. With sharp blends most texels need one plane, not three.
const TRIPLANAR_SKIP: f32 = 1.0 / 131_072.0;

/// Normalized triplanar blend weights (YZ, XZ, XY planes) for normal `nrm`; planes too light
/// to change a 0..1 value scaled by `gain` afterwards weigh 0 (see [`TRIPLANAR_SKIP`]).
fn triplanar_weights(nrm: Vec3, sharpness: f32, gain: f32) -> Vec3 {
    let weights = sharpen(nrm.abs(), sharpness);
    let weights = weights / (weights.x + weights.y + weights.z).max(1e-6);
    let skip_threshold = TRIPLANAR_SKIP / gain.max(1.0);
    let retained = Vec3::select(weights.cmplt(Vec3::splat(skip_threshold)), Vec3::ZERO, weights);

    // Preserve the blend if thresholding would discard every plane
    if retained == Vec3::ZERO { weights } else { retained }
}

pub(crate) fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn shape(pr: &Procedural, v: f32) -> f32 {
    let v = ((v - 0.5) * pr.contrast.max(0.0) + 0.5 + pr.balance).clamp(0.0, 1.0);
    if pr.invert { 1.0 - v } else { v }
}

/// `q` turned by `rotation` degrees, scaled by `tiling` and moved by `offset`, as UV transformations place it
fn place(q: Vec2, tiling: Vec2, rotation: f32, offset: Vec2) -> Vec2 {
    let (sn, cs) = rotation.to_radians().sin_cos();
    Vec2::new(q.x * cs - q.y * sn, q.x * sn + q.y * cs) * tiling + offset
}

/// 0..1 around a cylindrical projection's axis (`b` over `a`), over an arc of `angle` degrees centred on the front
fn around(a: f32, b: f32, angle: f32) -> f32 {
    let u = b.atan2(a) / std::f32::consts::TAU + 0.5;
    if angle == 360.0 { u } else { (u - 0.5) * 360.0 / angle.max(1.0) + 0.5 }
}

fn place_uv(q: Vec2, uv: &UvTransform) -> Vec2 {
    place(q, Vec2::from(uv.tiling), uv.rotation, Vec2::from(uv.offset))
}

/// The middle of a frame's 0..1 texture coordinates: a frame's image turns round it
pub const FRAME_UV_CENTER: Vec2 = Vec2::splat(0.5);

/// `q` (0..1 across and down a frame) placed as `uv` lays a frame's image: turned round the frame's centre, then tiled and moved from its top left
pub fn place_frame_uv(q: Vec2, uv: &UvTransform) -> Vec2 {
    let tiling = Vec2::from(uv.tiling);
    place(q - FRAME_UV_CENTER, tiling, uv.rotation, Vec2::from(uv.offset)) + FRAME_UV_CENTER * tiling
}

/// `c` inside a projector turned and scaled by `projector`; the default projector returns `c` unchanged
pub fn projector_local(projector: &Projector, c: &Coords) -> Coords {
    if !projector.turns() {
        return *c;
    }
    let [x, y, z] = projector.rotation.map(f32::to_radians);
    let to_local = Mat3::from_euler(EulerRot::ZYX, z, y, x).transpose();
    let scale = Vec3::from(projector.scale).map(|s| if s.abs() < 1e-6 { 1e-6 } else { s });
    let nrm = (to_local * c.nrm * scale).try_normalize().unwrap_or(c.nrm);
    Coords { pos: to_local * c.pos / scale, nrm, ..*c }
}

/// Evaluate a procedural through a projection, at every texel (its resolution aside).
pub fn procedural(pattern: &Procedural, projection: &Projection, coords: &Coords) -> f32 {
    evaluate(pattern, projection, coords, None)
}

/// Evaluate a procedural through a projection on a texture set `set` texels across a tile: a pattern with a resolution is read between the samples of that grid, as a texture cooked at that size
pub fn procedural_sized(pattern: &Procedural, projection: &Projection, coords: &Coords, set: u32) -> f32 {
    evaluate(pattern, projection, coords, pattern.resolution.map(|resolution| resolution.size(set.max(1))))
}

/// The 2D pattern at `q`, bilinear between the centres of a `grid` × `grid` sampling of each `scale` × `scale` cell of pattern space (one projection tile)
fn flat_on_grid(kind: ProceduralKind, seed: u32, q: Vec2, scale: f32, grid: u32) -> f32 {
    let n = grid as f32;
    let g = q / scale * n - Vec2::splat(0.5);
    let (i, f) = (g.floor(), g - g.floor());
    let at = |dx: f32, dy: f32| raw(kind, ((i + Vec2::new(dx, dy) + Vec2::splat(0.5)) / n * scale).extend(0.0), seed);
    let (top, bottom) = (at(0.0, 0.0) * (1.0 - f.x) + at(1.0, 0.0) * f.x, at(0.0, 1.0) * (1.0 - f.x) + at(1.0, 1.0) * f.x);
    top * (1.0 - f.y) + bottom * f.y
}

fn evaluate(pattern: &Procedural, projection: &Projection, coords: &Coords, grid: Option<u32>) -> f32 {
    let pattern_scale = pattern.scale.max(1e-4);

    // Evaluate two-dimensional patterns on a slice of the volume
    let flat = |q: Vec2| match grid {
        Some(grid) if grid > 0 => flat_on_grid(pattern.kind, pattern.seed, q, pattern_scale, grid),
        _ => raw(pattern.kind, q.extend(0.0), pattern.seed),
    };
    let value = match projection {
        Projection::UvTile => flat((coords.uv - coords.uv.floor()) * pattern_scale),
        Projection::Uv { scale, rotation, offset } => flat(place(coords.uv, Vec2::from(*scale), *rotation, Vec2::from(*offset)) * pattern_scale),
        Projection::Triplanar { scale, sharpness, offset, uv, projector } => {
            let coords = &projector_local(projector, coords);
            let p = coords.pos * *scale * pattern_scale + Vec3::from(*offset);

            // Shape scales the blend by the contrast
            let w = triplanar_weights(coords.nrm, *sharpness, pattern.contrast);
            let planes = [(w.x, Vec2::new(p.z, p.y)), (w.y, Vec2::new(p.x, p.z)), (w.z, Vec2::new(p.x, p.y))];
            planes.into_iter().filter(|(w, _)| *w > 0.0).map(|(w, q)| flat(place_uv(q, uv)) * w).sum()
        }
        // The coordinates are already in the placed or warped frame (see [`placed_uv`] and [`crate::warp::warp_uv`])
        Projection::Planar { uv, placed: Some(_), .. } | Projection::Warp { uv, .. } => flat(place_frame_uv(coords.uv, uv) * pattern_scale),
        Projection::Planar { scale, axis, offset, uv, projector, .. } => {
            let p = projector_local(projector, coords).pos * *scale * pattern_scale + Vec3::from(*offset);
            let q = match axis {
                0 => Vec2::new(p.z, p.y),
                1 => Vec2::new(p.x, p.z),
                _ => Vec2::new(p.x, p.y),
            };
            flat(place_uv(q, uv))
        }
        Projection::Spherical { scale, uv, offset, projector } => {
            let d = (projector_local(projector, coords).pos + Vec3::from(*offset)).try_normalize().unwrap_or(Vec3::Y);
            let u = d.z.atan2(d.x) / std::f32::consts::TAU + 0.5;
            let v = d.y.clamp(-1.0, 1.0).acos() / std::f32::consts::PI;
            flat(place_uv(Vec2::new(u * 4.0, v * 2.0) * *scale * pattern_scale, uv))
        }
        Projection::Cylindrical { scale, axis, uv, offset, projector, angle } => {
            let p = projector_local(projector, coords).pos + Vec3::from(*offset);
            let (a, h, b) = match axis {
                0 => (p.y, p.x, p.z),
                2 => (p.x, p.z, p.y),
                _ => (p.x, p.y, p.z),
            };
            let u = around(a, b, *angle);
            flat(place_uv(Vec2::new(u * 4.0, h) * *scale * pattern_scale, uv))
        }
    };

    shape(pattern, value)
}

/// Texture coordinates (and blend weights) at which an image is sampled for a projection
/// (unused slots weigh 0; no allocation per texel).
pub fn image_samples(projection: &Projection, coords: &Coords) -> [(Vec2, f32); 3] {
    let one = |uv: Vec2| [(uv, 1.0), (Vec2::ZERO, 0.0), (Vec2::ZERO, 0.0)];
    match projection {
        Projection::UvTile => one(coords.uv - coords.uv.floor()),
        Projection::Uv { scale, rotation, offset } => one(place(coords.uv, Vec2::from(*scale), *rotation, Vec2::from(*offset))),
        Projection::Triplanar { scale, sharpness, offset, uv, projector } => {
            let coords = &projector_local(projector, coords);
            let p = coords.pos * *scale + Vec3::from(*offset);

            // Float images can exceed 1, so even a tiny plane weight can carry visible detail
            let weights = coords.nrm.abs().powf(sharpness.max(1.0));
            let weights = weights / (weights.x + weights.y + weights.z).max(1e-6);
            [(place_uv(Vec2::new(p.z, -p.y), uv), weights.x), (place_uv(Vec2::new(p.x, p.z), uv), weights.y), (place_uv(Vec2::new(p.x, -p.y), uv), weights.z)]
        }
        Projection::Planar { uv, placed: Some(_), .. } | Projection::Warp { uv, .. } => one(place_frame_uv(coords.uv, uv)),
        Projection::Planar { scale, axis, offset, uv, projector, .. } => {
            let p = projector_local(projector, coords).pos * *scale + Vec3::from(*offset);
            let q = match axis {
                0 => Vec2::new(p.z, -p.y),
                1 => Vec2::new(p.x, p.z),
                _ => Vec2::new(p.x, -p.y),
            };
            one(place_uv(q * 0.5 + Vec2::splat(0.5), uv))
        }
        Projection::Spherical { scale, uv, offset, projector } => {
            let d = (projector_local(projector, coords).pos + Vec3::from(*offset)).try_normalize().unwrap_or(Vec3::Y);
            let u = d.z.atan2(d.x) / std::f32::consts::TAU + 0.5;
            let v = d.y.clamp(-1.0, 1.0).acos() / std::f32::consts::PI;
            one(place_uv(Vec2::new(u, v) * *scale, uv))
        }
        Projection::Cylindrical { scale, axis, uv, offset, projector, angle } => {
            let p = projector_local(projector, coords).pos + Vec3::from(*offset);
            let (a, h, b) = match axis {
                0 => (p.y, p.x, p.z),
                2 => (p.x, p.z, p.y),
                _ => (p.x, p.y, p.z),
            };
            let u = around(a, b, *angle);
            one(place_uv(Vec2::new(u, 0.5 - h * 0.5) * *scale, uv))
        }
    }
}

/// A decal's frame: origin (center plus offset), unit right, up and normal (None when degenerate).
pub fn decal_axes(d: &Decal) -> Option<(Vec3, Vec3, Vec3, Vec3)> {
    if !d.size.iter().all(|v| v.is_finite() && *v > 0.0) {
        return None;
    }
    let n = Vec3::from(d.normal).try_normalize()?;
    let up0 = (Vec3::from(d.up) - n * n.dot(Vec3::from(d.up))).try_normalize().unwrap_or_else(|| n.any_orthonormal_vector());
    let right0 = up0.cross(n);
    let origin = Vec3::from(d.center) + right0 * d.offset[0] + up0 * d.offset[1];
    let (sn, cs) = d.rotation.to_radians().sin_cos();
    let right = right0 * cs + up0 * sn;
    let up = up0 * cs - right0 * sn;
    origin.is_finite().then_some((origin, right, up, n))
}

/// The decal's corners in world space (top left, top right, bottom right, bottom left).
pub fn decal_corners(d: &Decal) -> Option<[Vec3; 4]> {
    let (o, r, u, _) = decal_axes(d)?;
    let (hw, hh) = (d.size[0] * 0.5, d.size[1] * 0.5);
    Some([o - r * hw + u * hh, o + r * hw + u * hh, o + r * hw - u * hh, o - r * hw - u * hh])
}

/// World point `p` in frame `d` turned and scaled by `projector`: across and up in frame sizes, out along its normal in world units
pub fn frame_local(d: &Decal, projector: &Projector, p: Vec3) -> Option<Vec3> {
    let (o, r, u, n) = decal_axes(d)?;
    let q = p - o;
    let mut local = Vec3::new(q.dot(r), q.dot(u), q.dot(n));
    if projector.turns() {
        local = projector_local(projector, &Coords { uv: Vec2::ZERO, pos: local, nrm: Vec3::Z }).pos;
    }
    let local = Vec3::new(local.x / d.size[0].max(1e-6), local.y / d.size[1].max(1e-6), local.z);
    local.is_finite().then_some(local)
}

/// How squarely a surface with normal `nrm` faces frame `d` turned by `projector` (the cosine of the turn)
pub fn frame_facing(d: &Decal, projector: &Projector, nrm: Vec3) -> f32 {
    let Some((_, r, u, n)) = decal_axes(d) else { return 0.0 };
    if !projector.turns() {
        return nrm.dot(n);
    }
    let turned = Vec3::new(nrm.dot(r), nrm.dot(u), nrm.dot(n));
    projector_local(projector, &Coords { uv: Vec2::ZERO, pos: Vec3::ZERO, nrm: turned }).nrm.z
}

/// Texture coordinates (v down) of a point at `local` in its frame (see [`frame_local`])
pub fn placed_uv(local: Vec3) -> Vec2 {
    Vec2::new(local.x + 0.5, 0.5 - local.y)
}

/// How much frame `d` covers a point at `local` when cropped to its shape: 0 outside the frame or past its depth, fading near the depth
pub fn frame_crop(d: &Decal, local: Vec3) -> f32 {
    if local.x.abs() > 0.5 || local.y.abs() > 0.5 {
        return 0.0;
    }
    depth_fade(local.z, d.depth)
}

/// How much a frame reaching `depth` in front of and behind it covers a point `z` out from it: 0 past the depth, fading near it
pub fn depth_fade(z: f32, depth: f32) -> f32 {
    let (lz, depth) = (z.abs(), depth.max(1e-6));
    if lz > depth {
        return 0.0;
    }
    1.0 - smoothstep(0.75 * depth, depth, lz)
}

/// World-space noise used by generators (seamless across UV islands).
pub fn world_noise(pos: Vec3, scale: f32, seed: u32) -> f32 {
    noise::fbm((pos * scale.max(1e-3)).to_array(), 4, seed, noise::perlin)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A frame's image turns round the frame's centre: the centre reads the same texel at any Rotation, and an unturned image reads exactly as before for any Tiling and Offset
    #[test]
    fn a_frame_image_turns_round_the_frame_centre() {
        let frame = Decal { center: [0.0; 3], normal: [0.0, 0.0, 1.0], up: [0.0, 1.0, 0.0], size: [1.0; 2], rotation: 0.0, offset: [0.0; 2], depth: 0.5 };
        let at = |uv: Vec2| Coords { uv, pos: Vec3::ZERO, nrm: Vec3::Z };
        let sample = |rotation: f32, tiling: [f32; 2], offset: [f32; 2], uv: Vec2| {
            image_samples(
                &Projection::Warp {
                    frame,
                    corners: Projection::FRAME_CORNERS,
                    depth: 0.1,
                    uv: UvTransform { tiling, rotation, offset },
                    projector: Projector::default(),
                },
                &at(uv),
            )[0]
            .0
        };
        for rotation in [37.0, 90.0, 180.0] {
            let center = sample(rotation, [1.0; 2], [0.0; 2], FRAME_UV_CENTER);
            assert!((center - FRAME_UV_CENTER).length() < 1e-5, "{rotation}°: the centre stays put, not swung round the top left: {center}");
        }
        let corner = sample(90.0, [1.0; 2], [0.0; 2], Vec2::ZERO);
        assert!((corner - Vec2::new(1.0, 0.0)).length() < 1e-5, "a quarter turn brings the top left to the top right: {corner}");
        let placed = Projection::Planar {
            scale: 1.0,
            axis: 2,
            offset: [0.0; 3],
            uv: UvTransform { rotation: 90.0, ..UvTransform::default() },
            projector: Projector::default(),
            placed: Some(frame),
        };
        assert!((image_samples(&placed, &at(FRAME_UV_CENTER))[0].0 - FRAME_UV_CENTER).length() < 1e-5, "a placed planar frame turns the same way");
        for (tiling, offset, uv) in [([2.0, 3.0], [0.25, -0.5], Vec2::new(0.1, 0.8)), ([1.0, 1.0], [0.0, 0.0], Vec2::new(0.7, 0.2))] {
            let before = uv * Vec2::from(tiling) + Vec2::from(offset);
            assert!((sample(0.0, tiling, offset, uv) - before).length() < 1e-6, "unturned: as before");
        }
    }

    #[test]
    fn every_pattern_and_projection_stays_in_range() {
        let projs = [
            Projection::default(),
            Projection::Triplanar { scale: 1.0, sharpness: 4.0, offset: [0.0; 3], uv: Default::default(), projector: Default::default() },
            Projection::Planar { scale: 1.0, axis: 1, offset: [0.0; 3], uv: Default::default(), projector: Default::default(), placed: None },
            Projection::Spherical { scale: 1.0, uv: Default::default(), offset: [0.0; 3], projector: Default::default() },
            Projection::Cylindrical { scale: 1.0, axis: 1, uv: Default::default(), offset: [0.0; 3], projector: Default::default(), angle: 360.0 },
            Projection::decal(Decal {
                center: [0.0; 3],
                normal: [0.0, 0.0, 1.0],
                up: [0.0, 1.0, 0.0],
                size: [1.0; 2],
                rotation: 0.0,
                offset: [0.0; 2],
                depth: 1.0,
            }),
        ];
        for k in ProceduralKind::ALL {
            let pr = Procedural::new(k);
            for proj in &projs {
                for i in 0..50 {
                    let t = i as f32 * 0.13;
                    let c = Coords {
                        uv: Vec2::new(t.fract(), (t * 0.7).fract()),
                        pos: Vec3::new(t.sin(), t.cos(), t * 0.1),
                        nrm: Vec3::new(0.3, 0.9, 0.1).normalize(),
                    };
                    let v = procedural(&pr, proj, &c);
                    assert!((0.0..=1.0).contains(&v), "{k:?} {proj:?} {v}");
                }
            }
        }
    }

    /// Skipping light triplanar planes stays within a u16 step of evaluating all three, at any
    /// contrast; near an axis it does skip.
    #[test]
    fn triplanar_plane_skipping_stays_under_one_u16_step() {
        let full = |pattern: &Procedural, sharpness: f32, coords: &Coords| {
            let position = coords.pos * pattern.scale;
            let weights = coords.nrm.abs().powf(sharpness);
            let weights = weights / (weights.x + weights.y + weights.z);
            shape(
                pattern,
                raw(pattern.kind, Vec3::new(position.z, position.y, 0.0), pattern.seed) * weights.x
                    + raw(pattern.kind, Vec3::new(position.x, position.z, 0.0), pattern.seed) * weights.y
                    + raw(pattern.kind, Vec3::new(position.x, position.y, 0.0), pattern.seed) * weights.z,
            )
        };
        let mut largest_difference = 0.0f32;
        for kind in ProceduralKind::ALL {
            for contrast in [1.0, 4.0, 16.0] {
                let pattern = Procedural { contrast, ..Procedural::new(kind) };
                for sharpness in [1.0, 4.0, 8.0, 16.0] {
                    let projection =
                        Projection::Triplanar { scale: 1.0, sharpness, offset: [0.0; 3], uv: UvTransform::default(), projector: Projector::default() };
                    for index in 0..400 {
                        // Cover axis aligned and oblique normals at varied positions
                        let position = index as f32 / 400.0;
                        let normal = Vec3::new(position * 1.3, 1.0, (position * 7.0).sin() * position).normalize();
                        let coords = Coords { uv: Vec2::ZERO, pos: Vec3::new((position * 11.0).sin(), (position * 5.0).cos(), position * 3.0), nrm: normal };
                        largest_difference = largest_difference.max((procedural(&pattern, &projection, &coords) - full(&pattern, sharpness, &coords)).abs());
                    }
                }
            }
        }

        assert!(largest_difference < 1.0 / 65535.0, "worst difference {largest_difference}");
        let near_axis = triplanar_weights(Vec3::new(0.2, 1.0, 0.1).normalize(), 8.0, 1.0);
        assert_eq!((near_axis.x, near_axis.z), (0.0, 0.0), "{near_axis}");

        // An oblique normal skips no plane
        let oblique = triplanar_weights(Vec3::ONE.normalize(), 8.0, 16.0);
        assert!((oblique - Vec3::splat(1.0 / 3.0)).abs().max_element() < 1e-6, "{oblique}");

        // Tiny weights must survive together when their sum exceeds a u16 step
        for sharpness in [47.0, 64.0] {
            let diagonal = triplanar_weights(Vec3::ONE.normalize(), sharpness, 1.0);
            assert!(diagonal.min_element() > 0.0, "sharpness {sharpness}: {diagonal}");
        }
    }

    #[test]
    fn triplanar_image_sampling_preserves_bright_float_pixels() {
        let image = crate::ImageRes::new(2, 2, vec![[0.0; 4], [0.0; 4], [0.0; 4], [100_000.0; 4]], false);
        let coords = Coords { uv: Vec2::ZERO, pos: Vec3::new(0.25, 0.25, 0.75), nrm: Vec3::new(0.2, 1.0, 0.1).normalize() };
        let projection = Projection::Triplanar { scale: 1.0, sharpness: 8.0, offset: [0.0; 3], uv: UvTransform::default(), projector: Projector::default() };
        let weights = coords.nrm.abs().powf(8.0);
        let expected = 100_000.0 * weights.x / (weights.x + weights.y + weights.z);

        let actual: f32 =
            image_samples(&projection, &coords).into_iter().filter(|(_, weight)| *weight > 0.0).map(|(uv, weight)| image.sample(uv.x, uv.y)[0] * weight).sum();

        assert!(expected > 0.25, "the light plane must make a visible contribution");
        assert!((actual - expected).abs() < 1.0 / 65535.0, "expected {expected}, got {actual}");
    }

    /// Repeated squaring agrees with `powf` to a few ulps for whole sharpnesses, and falls back
    /// to `powf` for the rest.
    #[test]
    fn sharpen_matches_powf() {
        for index in 0..=100 {
            let values = Vec3::new(index as f32 / 100.0, 1.0 - index as f32 / 100.0, (index as f32 * 0.37).fract());
            for sharpness in [0.0, 1.0, 2.0, 3.0, 4.0, 7.0, 8.0, 16.0] {
                let (actual, expected) = (sharpen(values, sharpness), values.powf(sharpness.max(1.0)));
                assert!((actual - expected).abs().max_element() <= 1e-6, "{values} ^ {sharpness}: {actual} vs {expected}");
            }

            for sharpness in [2.5, 17.0, 64.0] {
                assert_eq!(sharpen(values, sharpness), values.powf(sharpness));
            }
        }
    }

    /// Where a decal's planar projection lands at `p` facing `nrm`, None where it does not cover
    fn decal_uv(d: &Decal, p: Vec3, nrm: Vec3) -> Option<(Vec2, f32)> {
        let Projection::Planar { projector, .. } = Projection::decal(*d) else { return None };
        crate::projector_cover::placed_cover(d, &projector, p, nrm).filter(|(_, cover)| *cover > 0.0)
    }

    #[test]
    fn decals_cover_their_frame_facing_them() {
        let d = Decal { center: [0.0; 3], normal: [0.0, 0.0, 1.0], up: [0.0, 1.0, 0.0], size: [2.0, 1.0], rotation: 0.0, offset: [0.0; 2], depth: 0.5 };
        let z = Vec3::Z;
        let (uv, w) = decal_uv(&d, Vec3::new(0.5, 0.25, 0.0), z).unwrap();
        assert!((uv - Vec2::new(0.75, 0.25)).length() < 1e-5 && (w - 1.0).abs() < 1e-5, "{uv} {w}");
        assert!(decal_uv(&d, Vec3::new(1.1, 0.0, 0.0), z).is_none(), "outside the frame");
        assert!(decal_uv(&d, Vec3::ZERO, -z).is_none(), "back faces");
        assert!(decal_uv(&d, Vec3::new(0.0, 0.0, 0.6), z).is_none(), "beyond the depth");
        // Rotated a quarter turn and shifted: the frame turns with it.
        let r = Decal { rotation: 90.0, offset: [1.0, 0.0], ..d };
        let (uv, _) = decal_uv(&r, Vec3::new(1.0, 0.9, 0.0), z).unwrap();
        assert!((uv.x - 0.95).abs() < 1e-4 && (uv.y - 0.5).abs() < 1e-4, "{uv}");
        let c = decal_corners(&d).unwrap();
        assert!((c[0] - Vec3::new(-1.0, 0.5, 0.0)).length() < 1e-5 && (c[2] - Vec3::new(1.0, -0.5, 0.0)).length() < 1e-5);
        assert!(decal_axes(&Decal { normal: [0.0; 3], ..d }).is_none());
        assert!(decal_uv(&Decal { size: [f32::NAN; 2], ..d }, Vec3::ZERO, z).is_none());
    }

    #[test]
    fn twill_floats_run_in_diagonal_ribs() {
        // The value at the centre of cell (x, y); cells are half a unit.
        let at = |k, x: i32, y: i32| raw(k, Vec3::new(x as f32 * 0.5 + 0.25, y as f32 * 0.5 + 0.25, 0.0), 3);
        for y in 1..8 {
            let warp: Vec<bool> = (0..8).map(|x| at(ProceduralKind::Twill, x, y) > 0.7).collect();
            assert_eq!(warp.iter().filter(|w| **w).count(), 4, "row {y}: half the floats are warp");
            for x in 0..7 {
                assert_eq!(warp[x as usize], at(ProceduralKind::Twill, x + 1, y - 1) > 0.7, "the rib continues diagonally at ({x}, {y})");
            }
        }
        // A plain weave shows every top thread alike.
        assert!((0..8).all(|x| at(ProceduralKind::Weave, x, 0) > 0.7));
    }

    #[test]
    fn every_pattern_varies() {
        for k in ProceduralKind::ALL {
            let pr = Procedural::new(k);
            let (mut lo, mut hi) = (f32::MAX, f32::MIN);
            for i in 0..4096 {
                let uv = Vec2::new((i % 64) as f32 / 64.0, (i / 64) as f32 / 64.0);
                let v = procedural(&pr, &Projection::default(), &Coords { uv, pos: Vec3::ZERO, nrm: Vec3::Y });
                (lo, hi) = (lo.min(v), hi.max(v));
            }
            assert!(hi - lo > 0.3, "{k:?} is flat: {lo}..{hi}");
        }
    }
}
