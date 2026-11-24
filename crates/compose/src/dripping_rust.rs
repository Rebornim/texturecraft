//! The Dripping Rust generator: rust grown from the occlusion, broken up by grunge, then run down the mesh in drips; built once per layout and settings

use std::sync::Arc;

use rayon::prelude::*;
use texturecraft_core::{BakedMap, DrippingRustParameters, GeneratorKind, WearParameters};
use texturecraft_geom::glam::Vec3;

use crate::eval::{Inputs, texel_coords};
use crate::field_cache::FieldCache;
use crate::histogram_scan::histogram_scan;
use crate::wear;
use crate::working_field::{REFERENCE_TILE_TEXELS, blur_blocks, box_radius};

/// The rust before any grunge: `RUST_FLOOR + RUST_GAIN * max(RUST_OCCLUSION − occlusion, 0)^RUST_POWER` (fitted to the reference)
const RUST_FLOOR: f32 = 0.315;
const RUST_GAIN: f32 = 3.19;
const RUST_OCCLUSION: f32 = 0.849;
const RUST_POWER: f32 = 2.38;

/// How much grunge breaks the rust up: the spread of the reference's noise (measured); the pattern itself is inferred
const GRUNGE_SPREAD: f32 = 0.19;

/// Blur sizes at Spreading Smoothness 1 and Drips Smoothness 1, as gaussian sigmas in texels of a 2K UV tile (fitted)
const SPREADING_SIGMA_2K: f32 = 47.0;
const DRIPS_SIGMA_2K: f32 = 11.5;

/// How far a drip runs at Drips Intensity 1, in texels of a 2K UV tile, and how it fades along its length: `1 − DRIP_FADE · along^DRIP_FADE_POWER` (fitted)
const DRIP_LENGTH_2K: f32 = 90.86;
const DRIP_FADE: f32 = 0.898;
const DRIP_FADE_POWER: f32 = 0.671;

/// Coverage below which a blurred texel has nothing to average
const MIN_COVERAGE: f32 = 1e-3;

static CACHE: FieldCache = FieldCache::new();

/// The rust before its spreading and contrast at every working texel of `inp` (0 outside the islands)
pub(crate) fn rust_field(inp: &Inputs, rust: &DrippingRustParameters, seed: u32) -> Arc<Vec<f32>> {
    let occlusion = inp.baked.maps.get(&BakedMap::AmbientOcclusion);
    let settings = vec![
        rust.spreading_smoothness.to_bits(),
        rust.drips_intensity.to_bits(),
        rust.drips_smoothness.to_bits(),
        rust.drips_samples,
        u32::from(rust.axis),
        seed,
    ];
    CACHE.get_or_build(inp, &occlusion.into_iter().collect::<Vec<_>>(), settings, |block| build(inp, rust, seed, block))
}

/// The value at a working texel: the rust spread and contrasted
pub(crate) fn rust_value(rust: &DrippingRustParameters, field: f32) -> f32 {
    histogram_scan(field, rust.spreading, rust.contrast)
}

fn build(inp: &Inputs, rust: &DrippingRustParameters, seed: u32, block: (u32, u32)) -> Vec<f32> {
    let gbuf = inp.gbuf;
    let (width, height) = (gbuf.width, gbuf.height);
    let cover: Vec<f32> = (0..width * height).map(|texel| if gbuf.covered(texel as usize) { 1.0 } else { 0.0 }).collect();
    let tile_texels = |x: u32, y: u32| block.0 as f32 * inp.block_scales.at(x, y);

    // The rust from the occlusion, broken up by the Dirt generator's grunge
    let grunge = WearParameters::defaults(GeneratorKind::Dirt);
    let rust_at = |texel: u32| {
        let (x, y) = (texel % width, texel / width);
        let occlusion = inp.baked.sample_working(BakedMap::AmbientOcclusion, x, y, (width, height), &inp.block_scales).map_or(1.0, |value| value[0]);
        let base = RUST_FLOOR + RUST_GAIN * (RUST_OCCLUSION - occlusion).max(0.0).powf(RUST_POWER);
        let noise = grunge.map_or(0.0, |grunge| wear::grunge(GeneratorKind::Dirt, &grunge, seed, &texel_coords(inp, x, y)));
        base + GRUNGE_SPREAD * noise
    };
    let mut field: Vec<f32> = (0..width * height).into_par_iter().map(|texel| if gbuf.covered(texel as usize) { rust_at(texel) } else { 0.0 }).collect();

    let radius_at = |sigma_2k: f32, smoothness: f32| move |x: u32, y: u32| box_radius(smoothness * sigma_2k * tile_texels(x, y) / REFERENCE_TILE_TEXELS);
    blur_over_cover(&mut field, &cover, (width, height), block, radius_at(SPREADING_SIGMA_2K, rust.spreading_smoothness));
    let mut field = drips(inp, rust, &field, (width, height), &tile_texels);
    blur_over_cover(&mut field, &cover, (width, height), block, radius_at(DRIPS_SIGMA_2K, rust.drips_smoothness));
    field.iter_mut().zip(&cover).for_each(|(value, covered)| *value = if *covered > 0.0 { value.clamp(0.0, 1.0) } else { 0.0 });
    field
}

/// `values` blurred within each UV tile block by the radius `radius_at` gives, weighted by `cover` so the empty space around the islands doesn't bleed in
fn blur_over_cover(values: &mut [f32], cover: &[f32], size: (u32, u32), block: (u32, u32), radius_at: impl Fn(u32, u32) -> usize + Copy) {
    let mut sums: Vec<f32> = values.iter().zip(cover).map(|(value, covered)| value * covered).collect();
    let mut weights = cover.to_vec();
    blur_blocks(&mut sums, size, block, radius_at);
    blur_blocks(&mut weights, size, block, radius_at);
    values.par_iter_mut().zip(sums.par_iter().zip(weights.par_iter())).zip(cover.par_iter()).for_each(|((value, (sum, weight)), covered)| {
        if *covered > 0.0 && *weight > MIN_COVERAGE {
            *value = sum / weight;
        }
    });
}

/// Each texel takes the most rust found uphill along the drip axis, fading with how far up it lies
fn drips(inp: &Inputs, rust: &DrippingRustParameters, field: &[f32], (width, height): (u32, u32), tile_texels: &(impl Fn(u32, u32) -> f32 + Sync)) -> Vec<f32> {
    let gbuf = inp.gbuf;
    if rust.drips_samples == 0 || rust.drips_intensity <= 0.0 {
        return field.to_vec();
    }

    // The drip axis across the mesh's bounds at every covered texel
    let (low, high) = inp.bounds;
    let extent = (high - low).max(Vec3::splat(f32::EPSILON));
    let axis = usize::from(rust.axis.min(2));
    let along_axis: Vec<Option<f32>> = (0..width * height)
        .into_par_iter()
        .map(|texel| gbuf.surface(inp.mesh, texel as usize).map(|surface| ((surface.pos - low) / extent).to_array()[axis]))
        .collect();
    let axis_at = |x: i64, y: i64| -> Option<f32> {
        if x < 0 || y < 0 || x >= i64::from(width) || y >= i64::from(height) {
            return None;
        }
        along_axis.get((y * i64::from(width) + x) as usize).copied().flatten()
    };
    let field_at = |x: i64, y: i64| -> Option<f32> {
        if x < 0 || y < 0 || x >= i64::from(width) || y >= i64::from(height) || !gbuf.covered((y * i64::from(width) + x) as usize) {
            return None;
        }
        field.get((y * i64::from(width) + x) as usize).copied()
    };
    // Central differences, one-sided at an island's border
    let slope = |before: Option<f32>, here: f32, after: Option<f32>| match (before, after) {
        (Some(before), Some(after)) => (after - before) * 0.5,
        (None, Some(after)) => after - here,
        (Some(before), None) => here - before,
        (None, None) => 0.0,
    };

    let samples = rust.drips_samples;
    (0..width * height)
        .into_par_iter()
        .map(|texel| {
            let (x, y) = (i64::from(texel % width), i64::from(texel / width));
            let here = field.get(texel as usize).copied().unwrap_or(0.0);
            let Some(axis_here) = axis_at(x, y) else { return here };
            let uphill = (slope(axis_at(x - 1, y), axis_here, axis_at(x + 1, y)), slope(axis_at(x, y - 1), axis_here, axis_at(x, y + 1)));
            let length = (uphill.0 * uphill.0 + uphill.1 * uphill.1).sqrt();
            if length <= f32::EPSILON {
                return here;
            }

            let reach = rust.drips_intensity * DRIP_LENGTH_2K * tile_texels(texel % width, texel / width) / REFERENCE_TILE_TEXELS;
            (1..=samples).fold(here, |most, step| {
                let along = step as f32 / samples as f32;
                let distance = reach * along;
                let (sample_x, sample_y) = (x as f32 + distance * uphill.0 / length, y as f32 + distance * uphill.1 / length);
                match field_at(sample_x.round() as i64, sample_y.round() as i64) {
                    Some(value) => most.max(value * (1.0 - DRIP_FADE * along.powf(DRIP_FADE_POWER))),
                    None => most,
                }
            })
        })
        .collect()
}
