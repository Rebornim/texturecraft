//! Filters drawn from the distance to the shapes' edge: Bevel, Bevel Smooth and Directional Distance

use rayon::prelude::*;
use texturecraft_core::{BevelBlur, BevelCorner, Filter, lum};

use crate::eval::{MIN_ALPHA, blur};

/// Samples a Directional Distance trail reads behind a texel at most (beyond, samples spread out)
const TRAIL_SAMPLES: i32 = 64;

/// From a texel's centre to the edge between it and the nearest texel across: half a texel short of their centres' distance
const EDGE_OFFSET: f32 = 0.5;

/// What a texel becomes from its index, whether it is inside the shapes and its own value
type TexelResult<'a> = dyn Fn(usize, bool, f32) -> f32 + Sync + 'a;

/// Lay distance filter `filter` over premultiplied `data` (`width` × `height` texels of `comps`; content's shapes from its luminance), `res` texels across a UV tile, mixed in by `opacity`; its Distance Map `grays` (0..1 a texel) scales the distance texel by texel
pub(crate) fn distance_filter(
    data: &mut [f32],
    (cover, grays): (&[f32], &[f32]),
    comps: usize,
    (width, height): (usize, usize),
    filter: &Filter,
    res: u32,
    opacity: f32,
) {
    let map = |texel: usize| grays.get(texel).copied().unwrap_or(1.0);
    let value = |pixel: &[f32]| match pixel {
        [r, g, b, alpha] if *alpha > MIN_ALPHA => lum([r / alpha, g / alpha, b / alpha]),
        [value] => *value,
        _ => 0.0,
    };
    let values: Vec<f32> = data.par_chunks(comps).map(value).collect();
    let shapes = |inside: &(dyn Fn(f32) -> bool + Sync)| -> Vec<bool> {
        values.par_iter().zip(cover.par_iter()).map(|(value, covered)| *covered > 0.0 && inside(*value)).collect()
    };
    let smoothed = |field: &mut Vec<f32>, texels: f32| {
        let radius = texels.round() as usize;
        if radius > 0 {
            blur(field, width, height, radius);
        }
    };

    // Per texel: whether it is inside the shapes, and what it becomes from its own value
    let (inside, result): (Vec<bool>, Box<TexelResult<'_>>) = match filter {
        Filter::DistanceBevel(bevel) => {
            let inside = shapes(&|value| value >= texturecraft_core::Bevel::EDGE);
            let edge = edge_distances(&inside, (width, height), bevel.corner == BevelCorner::Angular);
            let reach = bevel.reach(res);
            let mut slope: Vec<f32> = inside.par_iter().zip(edge.par_iter()).map(|(inside, edge)| bevel.slope(*inside, *edge, reach)).collect();
            let mut blurred = slope.clone();
            smoothed(&mut blurred, bevel.smoothing_texels(res));
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
            let reach = smooth.reach(res);
            let mut ramp: Vec<f32> =
                inside.par_iter().zip(edge.par_iter()).enumerate().map(|(texel, (inside, edge))| smooth.value(*inside, *edge, reach * map(texel))).collect();
            smoothed(&mut ramp, smooth.smoothing_texels(res));
            (inside, Box::new(move |texel, _, value| ramp.get(texel).copied().unwrap_or(value)))
        }
        Filter::DirectionalDistance(trail) => {
            let inside = shapes(&|value| value >= texturecraft_core::DirectionalDistance::EDGE);
            let along = trail_distances(&inside, (width, height), crate::eval::direction(trail.angle), trail.reach(res));
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

    data.par_chunks_mut(comps).zip(inside.par_iter()).zip(cover.par_iter()).enumerate().for_each(|(texel, ((pixel, inside), covered))| {
        if *covered <= 0.0 {
            return;
        }

        // Content is premultiplied by its coverage, which stays
        let cov = if comps == 4 { pixel.get(3).copied().unwrap_or(0.0) } else { 1.0 };
        for slot in pixel.iter_mut().take(comps.min(3)) {
            let before = if comps == 4 { if cov > MIN_ALPHA { *slot / cov } else { 0.0 } } else { *slot };
            let after = result(texel, *inside, before);
            *slot = (before + (after - before) * opacity) * cov;
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
    rows.par_chunks_mut(width.max(1)).for_each_init(Envelope::default, |envelope, row| envelope.lower(row));
    let mut columns: Vec<f32> = (0..width * height).map(|at| rows.get((at % height.max(1)) * width + at / height.max(1)).copied().unwrap_or(far)).collect();
    columns.par_chunks_mut(height.max(1)).for_each_init(Envelope::default, |envelope, column| envelope.lower(column));
    (0..width * height).map(|at| columns.get((at % width.max(1)) * height + at / width.max(1)).copied().unwrap_or(far).sqrt()).collect()
}

/// The lower envelope of the parabolas over a line of squared distances, which gives each place's squared distance to the nearest feature along it and the line before
#[derive(Default)]
struct Envelope {
    /// The parabolas still lowest somewhere: their vertex and where their stretch begins
    parabolas: Vec<(usize, f32)>,
    /// The line as it was
    heights: Vec<f32>,
}

impl Envelope {
    /// Replace `line` with each place's squared distance to the nearest feature
    fn lower(&mut self, line: &mut [f32]) {
        self.heights.clear();
        self.heights.extend_from_slice(line);
        self.parabolas.clear();
        let heights = &self.heights;
        let lifted = |at: usize| heights.get(at).copied().unwrap_or(f32::MAX) + (at * at) as f32;
        for at in 0..heights.len() {
            let mut start = f32::NEG_INFINITY;
            while let Some(&(vertex, from)) = self.parabolas.last() {
                let crossing = (lifted(at) - lifted(vertex)) / (2.0 * (at - vertex) as f32);
                if crossing <= from {
                    self.parabolas.pop();
                } else {
                    start = crossing;
                    break;
                }
            }
            self.parabolas.push((at, start));
        }
        let mut current = 0;
        for (at, out) in line.iter_mut().enumerate() {
            while self.parabolas.get(current + 1).is_some_and(|(_, from)| *from < at as f32) {
                current += 1;
            }
            if let Some(&(vertex, _)) = self.parabolas.get(current) {
                *out = (at as f32 - vertex as f32).powi(2) + heights.get(vertex).copied().unwrap_or(f32::MAX);
            }
        }
    }
}

/// Every texel's distance (texels, the larger of across and down) to the nearest texel where `feature` holds; farther than the region when none does
fn chessboard_distances(feature: &[bool], (width, height): (usize, usize)) -> Vec<f32> {
    let far = (width + height) as f32;
    let mut distances: Vec<f32> = feature.iter().map(|feature| if *feature { 0.0 } else { far }).collect();

    // One pass down from the texels above and left, one up from those below and right
    let step = |distances: &mut Vec<f32>, (x, y): (usize, usize), neighbours: [(isize, isize); 4]| {
        let nearest = neighbours
            .iter()
            .filter_map(|(dx, dy)| {
                let (nx, ny) = (x.checked_add_signed(*dx)?, y.checked_add_signed(*dy)?);
                (nx < width && ny < height).then(|| distances.get(ny * width + nx).copied()).flatten()
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
                let (sx, sy) = ((x - direction[0] * back).round(), (y - direction[1] * back).round());
                let hit = sx >= 0.0
                    && sy >= 0.0
                    && (sx as usize) < width
                    && (sy as usize) < height
                    && inside.get(sy as usize * width + sx as usize).copied().unwrap_or(false);
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
}
