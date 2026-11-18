//! Filters drawn from the distance to the shapes' edge: Bevel, Bevel Smooth and Directional Distance

use rayon::prelude::*;
use texturecraft_core::{Bevel, BevelBlur, BevelCorner, DirectionalDistance, Filter, lum};

use crate::distance_envelope::DistanceEnvelope;
use crate::eval::{MIN_ALPHA, blur};

/// Samples a Directional Distance trail reads behind a texel at most (beyond, samples spread out)
const TRAIL_SAMPLES: i32 = 64;

/// From a texel's centre to the edge between it and the nearest texel across: half a texel short of their centres' distance
const EDGE_OFFSET: f32 = 0.5;

/// Components of a texel with color and coverage (red, green, blue, alpha)
const RGBA_COMPONENTS: usize = 4;

/// Color components of a texel, before its alpha
const COLOR_COMPONENTS: usize = 3;

/// Where a texel's alpha sits among its components
const ALPHA_INDEX: usize = 3;

/// What a texel becomes from its index, whether it is inside the shapes and its own value
type TexelResult<'a> = dyn Fn(usize, bool, f32) -> f32 + Sync + 'a;

/// Lay distance filter `filter` over premultiplied `data` (`width` × `height` texels of `components`; content's shapes from its luminance), `tile_resolution` texels across a UV tile, mixed in by `opacity`; its Distance Map `grays` (0..1 a texel) scales the distance texel by texel
pub(crate) fn distance_filter(
    data: &mut [f32],
    (cover, grays): (&[f32], &[f32]),
    components: usize,
    (width, height): (usize, usize),
    filter: &Filter,
    tile_resolution: u32,
    opacity: f32,
) {
    let map = |texel: usize| grays.get(texel).copied().unwrap_or(1.0);
    let value = |pixel: &[f32]| match pixel {
        [r, g, b, alpha] if *alpha > MIN_ALPHA => lum([r / alpha, g / alpha, b / alpha]),
        [value] => *value,
        _ => 0.0,
    };
    let values: Vec<f32> = data.par_chunks(components).map(value).collect();
    let shapes = |inside: &(dyn Fn(f32) -> bool + Sync)| -> Vec<bool> {
        values.par_iter().zip(cover.par_iter()).map(|(value, covered)| *covered > 0.0 && inside(*value)).collect()
    };
    let smoothed = |field: &mut [f32], texels: f32| {
        let radius = texels.round() as usize;
        if radius > 0 {
            blur(field, width, height, radius);
        }
    };

    // Per texel: whether it is inside the shapes, and what it becomes from its own value
    let (inside, result): (Vec<bool>, Box<TexelResult<'_>>) = match filter {
        Filter::DistanceBevel(bevel) => {
            let inside = shapes(&|value| value >= Bevel::EDGE);
            let edge = edge_distances(&inside, (width, height), bevel.corner == BevelCorner::Angular);
            let reach = bevel.reach(tile_resolution);
            let mut slope: Vec<f32> = inside.par_iter().zip(edge.par_iter()).map(|(inside, edge)| bevel.slope(*inside, *edge, reach)).collect();
            let mut blurred = slope.clone();
            smoothed(&mut blurred, bevel.smoothing_texels(tile_resolution));
            slope.par_iter_mut().zip(blurred.par_iter()).for_each(|(sharp, blurred)| {
                *sharp = match bevel.blur {
                    BevelBlur::Uniform => *blurred,
                    // Sharp at the slope's foot, as blurred as Uniform at its top
                    BevelBlur::NonUniform => *sharp + (blurred - *sharp) * *sharp,
                };
            });
            let bevel = *bevel;
            (inside, Box::new(move |texel, inside, value| bevel.apply(inside, value, slope.get(texel).copied().unwrap_or(1.0))))
        }
        Filter::BevelSmooth(smooth) => {
            let inside = shapes(&|value| smooth.inside(value));
            let edge = edge_distances(&inside, (width, height), false);
            let (reach, curve) = (smooth.reach(tile_resolution), smooth.curve());
            let mut ramp: Vec<f32> = inside
                .par_iter()
                .zip(edge.par_iter())
                .enumerate()
                .map(|(texel, (inside, edge))| smooth.value(curve, *inside, *edge, reach * map(texel)))
                .collect();
            smoothed(&mut ramp, smooth.smoothing_texels(tile_resolution));
            (inside, Box::new(move |texel, _, value| ramp.get(texel).copied().unwrap_or(value)))
        }
        Filter::DirectionalDistance(trail) => {
            let inside = shapes(&|value| value >= DirectionalDistance::EDGE);
            let along = trail_distances(&inside, (width, height), crate::eval::direction(trail.angle), trail.reach(tile_resolution));
            let trail = *trail;

            // A share of the full reach is a larger share of a texel's shorter one; no trail where the map is black, the shapes stay
            let shortened = move |texel: usize| {
                along.get(texel).copied().flatten().and_then(|along| match map(texel) {
                    _ if along <= 0.0 => Some(0.0),
                    gray if gray > 0.0 => Some(along / gray),
                    _ => None,
                })
            };
            (inside, Box::new(move |texel, _, _| trail.value(shortened(texel).filter(|along| *along <= 1.0))))
        }
        _ => return,
    };

    let has_alpha = components == RGBA_COMPONENTS;
    data.par_chunks_mut(components).zip(inside.par_iter()).zip(cover.par_iter()).enumerate().for_each(|(texel, ((pixel, inside), covered))| {
        if *covered <= 0.0 {
            return;
        }

        // Content is premultiplied by its coverage, which stays
        let coverage = if has_alpha { pixel.get(ALPHA_INDEX).copied().unwrap_or(0.0) } else { 1.0 };
        for slot in pixel.iter_mut().take(components.min(COLOR_COMPONENTS)) {
            let before = if has_alpha { if coverage > MIN_ALPHA { *slot / coverage } else { 0.0 } } else { *slot };
            let after = result(texel, *inside, before);
            *slot = (before + (after - before) * opacity) * coverage;
        }
    });
}

/// Every texel's distance (texels) to the edge between it and the nearest texel on the other side of the shapes `inside`: straight-line, or the larger axis distance when `angular`
fn edge_distances(inside: &[bool], (width, height): (usize, usize), angular: bool) -> Vec<f32> {
    let outside: Vec<bool> = inside.iter().map(|inside| !inside).collect();
    let (to_inside, to_outside) = if angular {
        (chessboard_distances(inside, (width, height)), chessboard_distances(&outside, (width, height)))
    } else {
        (straight_distances(inside, (width, height)), straight_distances(&outside, (width, height)))
    };
    inside
        .par_iter()
        .zip(to_inside.par_iter().zip(to_outside.par_iter()))
        .map(|(inside, (to_inside, to_outside))| (if *inside { *to_outside } else { *to_inside } - EDGE_OFFSET).max(0.0))
        .collect()
}

/// Every texel's straight-line distance (texels) to the nearest texel where `feature` holds; farther than the region when none does
fn straight_distances(feature: &[bool], (width, height): (usize, usize)) -> Vec<f32> {
    // Squared distances, separable: along the rows, then along the columns of those
    let far = ((width + height) as f32).powi(2);
    let mut rows: Vec<f32> = feature.iter().map(|feature| if *feature { 0.0 } else { far }).collect();
    rows.par_chunks_mut(width.max(1)).for_each_init(DistanceEnvelope::default, |envelope, row| envelope.lower(row));
    let mut columns: Vec<f32> = (0..width * height).map(|at| rows.get((at % height.max(1)) * width + at / height.max(1)).copied().unwrap_or(far)).collect();
    columns.par_chunks_mut(height.max(1)).for_each_init(DistanceEnvelope::default, |envelope, column| envelope.lower(column));
    (0..width * height).map(|at| columns.get((at % width.max(1)) * height + at / width.max(1)).copied().unwrap_or(far).sqrt()).collect()
}

/// Every texel's distance (texels, the larger of across and down) to the nearest texel where `feature` holds; farther than the region when none does
fn chessboard_distances(feature: &[bool], (width, height): (usize, usize)) -> Vec<f32> {
    let far = (width + height) as f32;
    let mut distances: Vec<f32> = feature.iter().map(|feature| if *feature { 0.0 } else { far }).collect();

    // One pass down from the texels above and left, one up from those below and right
    let step = |distances: &mut [f32], (x, y): (usize, usize), neighbours: [(isize, isize); 4]| {
        let nearest = neighbours
            .iter()
            .filter_map(|(dx, dy)| {
                let (neighbour_x, neighbour_y) = (x.checked_add_signed(*dx)?, y.checked_add_signed(*dy)?);
                (neighbour_x < width && neighbour_y < height).then(|| distances.get(neighbour_y * width + neighbour_x).copied()).flatten()
            })
            .fold(f32::MAX, f32::min);
        if let Some(slot) = distances.get_mut(y * width + x) {
            *slot = slot.min(nearest + 1.0);
        }
    };
    for y in 0..height {
        for x in 0..width {
            step(&mut distances, (x, y), [(-1, 0), (-1, -1), (0, -1), (1, -1)]);
        }
    }
    for y in (0..height).rev() {
        for x in (0..width).rev() {
            step(&mut distances, (x, y), [(1, 0), (1, 1), (0, 1), (-1, 1)]);
        }
    }
    distances
}

/// For every texel, how far (a share of `reach` texels) the nearest texel of the shapes `inside` lies behind it against `direction`, if one is in reach
fn trail_distances(inside: &[bool], (width, height): (usize, usize), direction: [f32; 2], reach: f32) -> Vec<Option<f32>> {
    let samples = (reach.ceil() as i32).clamp(1, TRAIL_SAMPLES);
    let step = reach / samples as f32;
    (0..width * height)
        .into_par_iter()
        .map(|texel| {
            let (x, y) = ((texel % width.max(1)) as f32, (texel / width.max(1)) as f32);
            (0..=samples).find_map(|sample| {
                let back = sample as f32 * step;
                let (sample_x, sample_y) = ((x - direction[0] * back).round(), (y - direction[1] * back).round());
                let hit = sample_x >= 0.0
                    && sample_y >= 0.0
                    && (sample_x as usize) < width
                    && (sample_y as usize) < height
                    && inside.get(sample_y as usize * width + sample_x as usize).copied().unwrap_or(false);
                hit.then(|| if reach > 0.0 { back / reach } else { 0.0 })
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distances_are_straight_or_chessboard() {
        // One feature texel in the middle of a 5 × 5 region
        let mut feature = vec![false; 25];
        feature[12] = true;
        let straight = straight_distances(&feature, (5, 5));
        let chessboard = chessboard_distances(&feature, (5, 5));
        assert_eq!((straight[12], straight[13], chessboard[13]), (0.0, 1.0, 1.0));
        assert!((straight[0] - 8f32.sqrt()).abs() < 1e-5, "{}", straight[0]);
        assert_eq!(chessboard[0], 2.0, "a corner is as far as its larger axis");
        let none = straight_distances(&[false; 4], (2, 2));
        assert!(none.iter().all(|distance| *distance >= 4.0), "no feature is beyond the region");
    }

    #[test]
    fn distances_keep_rows_and_columns_apart_in_a_region_wider_than_tall() {
        // One feature texel at column 1, row 0 of a 7 × 3 region
        let (width, height) = (7, 3);
        let mut feature = vec![false; width * height];
        feature[1] = true;
        let straight = straight_distances(&feature, (width, height));
        let chessboard = chessboard_distances(&feature, (width, height));

        let at = |x: usize, y: usize| y * width + x;
        assert_eq!((straight[at(1, 0)], straight[at(6, 0)], straight[at(1, 2)]), (0.0, 5.0, 2.0));
        assert!((straight[at(6, 2)] - 29f32.sqrt()).abs() < 1e-5, "{}", straight[at(6, 2)]);
        assert_eq!((chessboard[at(6, 0)], chessboard[at(0, 2)], chessboard[at(6, 2)]), (5.0, 2.0, 5.0));
    }

    #[test]
    fn a_trail_finds_the_nearest_shape_behind_a_texel_as_a_share_of_its_reach() {
        // A shape at column 0 of a 5 × 2 region; the trail runs towards +x, so a texel looks back towards -x
        let (width, height) = (5, 2);
        let inside: Vec<bool> = (0..width * height).map(|texel| texel % width == 0).collect();
        let trail = trail_distances(&inside, (width, height), [1.0, 0.0], 4.0);

        assert_eq!(trail[0], Some(0.0), "on the shape");
        assert_eq!(trail[2], Some(0.5), "two texels behind, half the reach");
        assert_eq!(trail[width + 4], Some(1.0), "at the reach's end");
    }
}
