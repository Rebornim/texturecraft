//! A generator's micro details: the curvature and occlusion its micro height (an image or an anchor point's height) and normal image add to its mesh maps, built once per map, image and settings

mod anchor_entry;
mod anchor_heights;
mod cache_key;
mod cached_bytes;
mod layout_map;
mod micro_cache;
mod micro_curvature;
mod slot;
mod slot_key;

use std::sync::{Arc, Mutex, Weak};

use anchor_entry::AnchorEntry;
pub use anchor_heights::AnchorHeights;
use cache_key::CacheKey;
pub use layout_map::LayoutMap;
use micro_cache::MicroCache;
pub use micro_curvature::MicroCurvature;
use rayon::prelude::*;
use slot::Slot;
use texturecraft_core::{BakedMap, LayerId, MicroCurvatureType, MicroDetails};

use crate::below::StackKey;
use crate::eval::{BLUR_BOX_PASSES, Inputs};

/// The resolution of a UV tile the reference's micro details were measured at: derivatives and radii are per texel at this size
const REFERENCE_TILE_TEXELS: f32 = 2048.0;

/// How steeply the micro height tilts the normal it is turned into, per unit of Height Details Intensity (fitted)
const HEIGHT_NORMAL_STRENGTH: f32 = 5.9;

/// Blur sizes of the Smooth type, in texels at 2K: the height against each of these blurred copies
const SMOOTH_SIGMAS: [f32; 7] = [1.0, 2.0, 4.0, 8.0, 16.0, 32.0, 64.0];

/// Below this sigma a blur is an explicit kernel; wider ones are three box passes
const BOX_BLUR_MIN_SIGMA: f32 = 2.0;

/// How many sigmas the sampled kernel reaches each side
const KERNEL_REACH_SIGMAS: f32 = 3.0;

/// Directions the occlusion looks along
const OCCLUSION_DIRECTIONS: usize = 16;

/// Most steps the occlusion takes along each direction; longer radii step further each time
const OCCLUSION_STEPS: usize = 16;

/// The occlusion radius in texels at 1K for AO Radius 1, and its exponent (fitted: radius 0.3 reaches about 6 texels, 1 about 48)
const OCCLUSION_RADIUS_TEXELS_1K: f32 = 47.675;
const OCCLUSION_RADIUS_POWER: f32 = 1.663;

/// Shorter radii at 1K occlude nothing: the reference is unchanged at AO Radius 0.1 (about 1 texel) and occludes from 0.2 (about 3)
const MIN_OCCLUSION_RADIUS_TEXELS_1K: f32 = 2.0;

/// How steep the micro height reads for occlusion per texel at 1K at AO Depth 1, and its exponent (fitted)
const OCCLUSION_DEPTH_SCALE_1K: f32 = 1473.8;
const OCCLUSION_DEPTH_POWER: f32 = 1.0654;

/// How dark a fully occluding horizon makes the occlusion (fitted)
const OCCLUSION_STRENGTH: f32 = 0.3232;

/// Most micro maps cached, and the bytes they may hold (the newest is kept whatever its size), like the curvature sizes they feed
const CACHE_ENTRIES: usize = 8;
const CACHE_BYTES: usize = 256 << 20;

/// The anchor heights prepared last, one per mesh maps, texture set and anchor
static ANCHOR_HEIGHTS: Mutex<Vec<AnchorEntry>> = Mutex::new(Vec::new());

/// The mesh maps (by address) and texture sets whose anchor heights are being built: the build composites the stack, which prepares them again
static BUILDING_ANCHOR_HEIGHTS: Mutex<Vec<(usize, usize)>> = Mutex::new(Vec::new());

/// A mesh map that tells one session's baked maps from another's (None without curvature or occlusion)
fn baked_identity(inp: &Inputs) -> Option<Weak<Vec<f32>>> {
    [BakedMap::Curvature, BakedMap::AmbientOcclusion].iter().find_map(|map| inp.baked.maps.get(map)).map(Arc::downgrade)
}

/// Keep anchor `anchor`'s heights for the micro details reading it, building them with `build` unless `stack` is the stack they were built for
pub(crate) fn prepare_anchor_heights(inp: &Inputs, anchor: LayerId, stack: StackKey, build: impl FnOnce() -> AnchorHeights) {
    let Some(baked) = baked_identity(inp) else { return };
    let matches = |entry: &AnchorEntry| entry.baked.ptr_eq(&baked) && entry.texture_set == inp.ts && entry.anchor == anchor;
    {
        let mut entries = ANCHOR_HEIGHTS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        entries.retain(|entry| entry.baked.strong_count() > 0);
        if entries.iter().any(|entry| matches(entry) && entry.stack == stack) {
            return;
        }
    }

    // Built outside the lock: the build composites the stack in parallel, and leaves the anchors it meets unprepared
    let building = (baked.as_ptr() as usize, inp.ts);
    {
        let mut active = BUILDING_ANCHOR_HEIGHTS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if active.contains(&building) {
            return;
        }
        active.push(building);
    }
    let heights = Arc::new(build());
    BUILDING_ANCHOR_HEIGHTS.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).retain(|active| *active != building);
    let mut entries = ANCHOR_HEIGHTS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    entries.retain(|entry| !matches(entry));
    entries.push(AnchorEntry { baked, texture_set: inp.ts, anchor, stack, heights });
    let excess = entries.len().saturating_sub(CACHE_ENTRIES);
    entries.drain(..excess);
}

/// The heights prepared for anchor `anchor` (None before they are)
fn anchor_heights(inp: &Inputs, anchor: LayerId) -> Option<Arc<AnchorHeights>> {
    let baked = baked_identity(inp)?;
    let entries = ANCHOR_HEIGHTS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    entries.iter().find(|entry| entry.baked.ptr_eq(&baked) && entry.texture_set == inp.ts && entry.anchor == anchor).map(|entry| entry.heights.clone())
}

/// The UV of each texel's center in a `size` block, row by row
fn block_uvs((width, height): (u32, u32)) -> impl Iterator<Item = [f32; 2]> {
    (0..height).flat_map(move |y| (0..width).map(move |x| [(x as f32 + 0.5) / width as f32, (y as f32 + 0.5) / height as f32]))
}

static CURVATURE_CACHE: MicroCache<MicroCurvature> = MicroCache::new();
static OCCLUSION_CACHE: MicroCache<LayoutMap> = MicroCache::new();

/// A baked map, its size and the size of one UV tile's block in it
type MapLayout = (Arc<Vec<f32>>, (u32, u32), (u32, u32));

/// The baked `map`'s layout (None when it isn't baked)
fn layout(inp: &Inputs, map: BakedMap) -> Option<MapLayout> {
    let data = inp.baked.maps.get(&map)?.clone();
    let size = (inp.baked.resolution, inp.baked.height(map));
    if size.0 == 0 || size.1 == 0 {
        return None;
    }

    let grid = inp.project.texture_set(inp.ts).ok().and_then(|set| set.tile_grid());
    let block = grid.map_or(size, |grid| (size.0 / grid.cols.max(1), size.1 / grid.rows.max(1)));
    Some((data, size, (block.0.max(1).min(size.0), block.1.max(1).min(size.1))))
}

/// `per_block` run on each UV tile block of a `size` map, its results placed into one map
fn by_blocks(size: (u32, u32), block: (u32, u32), same_in_every_block: bool, per_block: impl Fn((u32, u32), (u32, u32)) -> Vec<f32> + Sync) -> Vec<f32> {
    let blocks: Vec<(u32, u32)> = (0..size.1).step_by(block.1 as usize).flat_map(|y| (0..size.0).step_by(block.0 as usize).map(move |x| (x, y))).collect();

    // Every block is a whole UV tile, so one computation serves them all unless the slots differ per block; a clipped last block differs too
    let full = same_in_every_block.then(|| per_block((0, 0), block));
    let mut data = vec![0.0; (size.0 * size.1) as usize];
    for (left, top) in blocks {
        let clipped = (block.0.min(size.0 - left), block.1.min(size.1 - top));
        let own = if clipped == block && full.is_some() { None } else { Some(per_block((left, top), clipped)) };
        let Some(values) = own.as_deref().or(full.as_deref()) else { continue };
        for row in 0..clipped.1 {
            let source = (row * clipped.0) as usize;
            let target = ((top + row) * size.0 + left) as usize;
            if let (Some(target), Some(source)) = (data.get_mut(target..target + clipped.0 as usize), values.get(source..source + clipped.0 as usize)) {
                target.copy_from_slice(source);
            }
        }
    }
    data
}

/// What the occlusion input is multiplied by: the micro height's occlusion in the baked AO map's layout (None without a baked AO map or a micro height)
pub fn micro_occlusion(inp: &Inputs, micro: &MicroDetails) -> Option<Arc<LayoutMap>> {
    let (occlusion, size, block) = layout(inp, BakedMap::AmbientOcclusion)?;
    let height = Slot::of(inp, micro.height.then_some(micro.height_source.as_ref())?);
    let settings = vec![micro.ao_radius.to_bits(), micro.ao_depth.to_bits()];
    let key = CacheKey { map: Arc::downgrade(&occlusion), height: Some(height.key()), normal: None, settings, block };

    Some(OCCLUSION_CACHE.get_or_build(key, || {
        let data = by_blocks(size, block, height.same_in_every_block(), |origin, block| occlusion_factor(micro, &height.in_block(origin, size), block));
        LayoutMap { data, size }
    }))
}

/// The fitted constants of a curvature type: what it adds everywhere, and its gain on the height and on the normal
fn type_constants(curvature_type: MicroCurvatureType) -> (f32, f32, f32) {
    match curvature_type {
        MicroCurvatureType::Standard => (0.00876, 12.36, 9.49),
        MicroCurvatureType::Sobel => (0.0, 5.72, 4.46),
        MicroCurvatureType::Smooth => (-0.00332, 0.969, 34.2),
    }
}

/// The curvature the micro details add over one `size` UV tile block
fn curvature_term(micro: &MicroDetails, height: Option<&Slot>, normal: Option<&Slot>, size: (u32, u32)) -> Vec<f32> {
    let (offset, height_gain, normal_gain) = type_constants(micro.curvature_type);
    let texel_scale = size.0 as f32 / REFERENCE_TILE_TEXELS;
    let mut term = vec![offset; (size.0 * size.1) as usize];

    if let Some(height) = height {
        let heights = height.heights(size);
        let detail = match micro.curvature_type {
            MicroCurvatureType::Smooth => smooth_detail(&heights, size, texel_scale).into_iter().map(|detail| detail * micro.height_intensity).collect(),
            MicroCurvatureType::Standard | MicroCurvatureType::Sobel => {
                // The height turned into a normal, then that normal's divergence
                let strength = HEIGHT_NORMAL_STRENGTH * micro.height_intensity * texel_scale;
                let (slope_x, slope_y) = sobel(&heights, size);
                let tilted = |slope_x: f32, slope_y: f32| {
                    let (x, y) = (-strength * slope_x, -strength * slope_y);
                    let length = (x * x + y * y + 1.0).sqrt();
                    (x / length, y / length)
                };
                let (normal_x, normal_y): (Vec<f32>, Vec<f32>) = slope_x.par_iter().zip(slope_y.par_iter()).map(|(x, y)| tilted(*x, *y)).unzip();
                divergence(&normal_x, &normal_y, size, texel_scale)
            }
        };
        add_scaled(&mut term, &detail, micro.curvature_intensity * height_gain);
    }
    if let Some(normal) = normal {
        let (normal_x, normal_y) = normal.normals(size);
        add_scaled(&mut term, &divergence(&normal_x, &normal_y, size, texel_scale), micro.curvature_intensity * normal_gain);
    }
    term
}

fn add_scaled(target: &mut [f32], values: &[f32], gain: f32) {
    target.par_iter_mut().zip(values.par_iter()).for_each(|(target, value)| *target += value * gain);
}

/// The Smooth type's detail: the height above its blurred copies, averaged over [`SMOOTH_SIGMAS`]
fn smooth_detail(heights: &[f32], size: (u32, u32), texel_scale: f32) -> Vec<f32> {
    let mut detail = vec![0.0; heights.len()];
    for sigma in SMOOTH_SIGMAS {
        let blurred = gaussian_wrapped(heights, size, sigma * texel_scale);
        detail.par_iter_mut().zip(heights.par_iter().zip(blurred.par_iter())).for_each(|(detail, (height, blurred))| *detail += height - blurred);
    }
    let count = SMOOTH_SIGMAS.len() as f32;
    detail.par_iter_mut().for_each(|detail| *detail /= count);
    detail
}

/// The divergence of the `x` and `y` fields, per texel at 2K
fn divergence(x: &[f32], y: &[f32], size: (u32, u32), texel_scale: f32) -> Vec<f32> {
    let (dx, _) = sobel(x, size);
    let (_, dy) = sobel(y, size);
    dx.par_iter().zip(dy.par_iter()).map(|(dx, dy)| (dx + dy) * texel_scale).collect()
}

/// The slope of `values` along X and Y per texel (3×3 Sobel), wrapping around the block like a tiling image
fn sobel(values: &[f32], (width, height): (u32, u32)) -> (Vec<f32>, Vec<f32>) {
    let (width, height) = (i64::from(width.max(1)), i64::from(height.max(1)));
    let at = |x: i64, y: i64| values.get((y.rem_euclid(height) * width + x.rem_euclid(width)) as usize).copied().unwrap_or(0.0);
    (0..width * height)
        .into_par_iter()
        .map(|texel| {
            let (x, y) = (texel % width, texel / width);
            let column = |dx: i64| at(x + dx, y - 1) + 2.0 * at(x + dx, y) + at(x + dx, y + 1);
            let row = |dy: i64| at(x - 1, y + dy) + 2.0 * at(x, y + dy) + at(x + 1, y + dy);
            ((column(1) - column(-1)) / 8.0, (row(1) - row(-1)) / 8.0)
        })
        .unzip()
}

/// `values` blurred to `sigma` texels, wrapping around the block like a tiling image
fn gaussian_wrapped(values: &[f32], (width, height): (u32, u32), sigma: f32) -> Vec<f32> {
    let rows = blur_lines(values, width as usize, sigma);
    let columns = blur_lines(&transposed(&rows, width as usize, height as usize), height as usize, sigma);
    transposed(&columns, height as usize, width as usize)
}

/// Each `length`-long line of `values` blurred to `sigma`, wrapping around
fn blur_lines(values: &[f32], length: usize, sigma: f32) -> Vec<f32> {
    let length = length.max(1);
    let mut out = vec![0.0; values.len()];
    out.par_chunks_mut(length).zip(values.par_chunks(length)).for_each(|(out, line)| {
        if sigma < BOX_BLUR_MIN_SIGMA {
            kernel_blur(line, out, sigma);
        } else {
            // Three box passes of radius r spread a value with variance r(r + 1)
            let radius = (((1.0 + 4.0 * sigma * sigma).sqrt() - 1.0) / 2.0).round().max(1.0) as usize;
            let mut current = line.to_vec();
            for _ in 0..BLUR_BOX_PASSES {
                box_blur(&current, out, radius);
                current.copy_from_slice(out);
            }
        }
    });
    out
}

/// `line` blurred into `out` by a sampled gaussian of `sigma`, wrapping around
fn kernel_blur(line: &[f32], out: &mut [f32], sigma: f32) {
    let reach = (KERNEL_REACH_SIGMAS * sigma).ceil().max(1.0) as i64;
    let weights: Vec<f32> = (-reach..=reach).map(|offset| (-(offset * offset) as f32 / (2.0 * sigma * sigma)).exp()).collect();
    let total: f32 = weights.iter().sum();
    let length = line.len().max(1) as i64;
    for (index, value) in out.iter_mut().enumerate() {
        let sum: f32 = (-reach..=reach)
            .zip(&weights)
            .map(|(offset, weight)| line.get((index as i64 + offset).rem_euclid(length) as usize).copied().unwrap_or(0.0) * weight)
            .sum();
        *value = sum / total;
    }
}

/// `line` averaged over `radius` texels each side into `out`, wrapping around
fn box_blur(line: &[f32], out: &mut [f32], radius: usize) {
    let length = line.len().max(1) as i64;
    let window = (2 * radius + 1) as f32;
    let at = |index: i64| line.get(index.rem_euclid(length) as usize).copied().unwrap_or(0.0);
    let mut sum: f32 = (-(radius as i64)..=radius as i64).map(at).sum();
    for (index, value) in out.iter_mut().enumerate() {
        *value = sum / window;
        let index = index as i64;
        sum += at(index + radius as i64 + 1) - at(index - radius as i64);
    }
}

fn transposed(values: &[f32], width: usize, height: usize) -> Vec<f32> {
    let height = height.max(1);
    (0..width * height).into_par_iter().map(|texel| values.get((texel % height) * width + texel / height).copied().unwrap_or(0.0)).collect()
}

/// What the occlusion is multiplied by over one `size` UV tile block: the horizon the micro height raises above its slope, looked for in [`OCCLUSION_DIRECTIONS`]
fn occlusion_factor(micro: &MicroDetails, height: &Slot, size: (u32, u32)) -> Vec<f32> {
    let tile_scale = size.0 as f32 / (REFERENCE_TILE_TEXELS / 2.0);
    let radius_1k = OCCLUSION_RADIUS_TEXELS_1K * micro.ao_radius.powf(OCCLUSION_RADIUS_POWER);
    let radius = (radius_1k * tile_scale).round() as usize;
    let count = (size.0 * size.1) as usize;
    if radius_1k < MIN_OCCLUSION_RADIUS_TEXELS_1K || radius < 1 {
        return vec![1.0; count];
    }

    let depth = OCCLUSION_DEPTH_SCALE_1K * micro.ao_depth.powf(OCCLUSION_DEPTH_POWER) * tile_scale;
    let heights = height.heights(size);
    let (width, rows) = (i64::from(size.0.max(1)), i64::from(size.1.max(1)));
    let at = |x: i64, y: i64| heights.get((y.rem_euclid(rows) * width + x.rem_euclid(width)) as usize).copied().unwrap_or(0.0);
    let directions: Vec<(f32, f32)> = (0..OCCLUSION_DIRECTIONS)
        .map(|index| std::f32::consts::TAU * index as f32 / OCCLUSION_DIRECTIONS as f32)
        .map(|angle| (angle.cos(), angle.sin()))
        .collect();
    // Up to OCCLUSION_STEPS distances spread over the radius
    let steps: Vec<f32> = if radius <= OCCLUSION_STEPS {
        (1..=radius).map(|step| step as f32).collect()
    } else {
        (1..=OCCLUSION_STEPS).map(|step| 1.0 + (radius - 1) as f32 * (step - 1) as f32 / (OCCLUSION_STEPS - 1) as f32).collect()
    };
    let sine = |slope: f32| slope / (1.0 + slope * slope).sqrt();

    (0..width * rows)
        .into_par_iter()
        .map(|texel| {
            let (x, y) = (texel % width, texel / width);
            let here = at(x, y);
            let gradient = ((at(x + 1, y) - at(x - 1, y)) / 2.0, (at(x, y + 1) - at(x, y - 1)) / 2.0);
            let occluded: f32 = directions
                .iter()
                .map(|(dx, dy)| {
                    let horizon = steps
                        .iter()
                        .map(|distance| (at(x + (dx * distance).round() as i64, y + (dy * distance).round() as i64) - here) / distance)
                        .fold(f32::NEG_INFINITY, f32::max);
                    let tangent = gradient.0 * dx + gradient.1 * dy;
                    (sine(horizon * depth) - sine(tangent * depth)).max(0.0)
                })
                .sum();
            1.0 - OCCLUSION_STRENGTH * occluded / OCCLUSION_DIRECTIONS as f32
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use texturecraft_core::GeneratorKind;

    use super::*;

    #[test]
    fn sobel_reads_a_ramp_as_its_slope() {
        let size = (8, 8);
        let ramp: Vec<f32> = (0..64).map(|texel| (texel % 8) as f32 * 0.1).collect();
        let (x, y) = sobel(&ramp, size);
        assert!((x[3 * 8 + 3] - 0.1).abs() < 1e-5);
        assert!(y[3 * 8 + 3].abs() < 1e-5);
    }

    #[test]
    fn wrapped_blurs_keep_the_mean() {
        let size = (32, 16);
        let values: Vec<f32> = (0..32 * 16).map(|texel| ((texel * 7919) % 13) as f32).collect();
        let mean = values.iter().sum::<f32>() / values.len() as f32;
        for sigma in [0.5, 1.0, 4.0] {
            let blurred = gaussian_wrapped(&values, size, sigma);
            assert!((blurred.iter().sum::<f32>() / blurred.len() as f32 - mean).abs() < 1e-3, "sigma {sigma}");
        }
    }

    #[test]
    fn a_flat_height_adds_only_the_types_offset() {
        let mut micro = MicroDetails::defaults(GeneratorKind::Curvature).unwrap();
        let flat = Slot::Uniform([0.4; 3]);
        for curvature_type in MicroCurvatureType::ALL {
            micro.curvature_type = curvature_type;
            let term = curvature_term(&micro, Some(&flat), Some(&Slot::Uniform([0.5, 0.5, 1.0])), (16, 16));
            assert!(term.iter().all(|value| (value - type_constants(curvature_type).0).abs() < 1e-6), "{curvature_type:?}");
        }
    }

    #[test]
    fn a_flat_height_occludes_nothing() {
        let micro = MicroDetails::defaults(GeneratorKind::AmbientOcclusion).unwrap();
        let factor = occlusion_factor(&micro, &Slot::Uniform([0.7; 3]), (64, 64));
        assert!(factor.iter().all(|value| (value - 1.0).abs() < 1e-6));
    }
}
