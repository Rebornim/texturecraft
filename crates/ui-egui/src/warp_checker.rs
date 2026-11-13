//! The checker a warp preview bends into its frame: laid on the frame as the fill's Tiling, Rotation and Offset lay the image, so turning or tiling the image shows

use egui::Pos2;

use crate::fill_placement::Placement;

/// Checker cells along each axis of one repeat of the image (an untiled, unturned image shows 4 × 4 cells across the frame)
pub(crate) const CELLS_PER_TILE: f32 = 4.0;
/// Most repeats drawn across the frame: beyond this the checker is too fine to read and too many cells to draw each frame
const MAX_TILES: f32 = 6.0;

/// The checker cells inside a frame whose corners land at `quad` (top left, top right, bottom right, bottom left), each with whether it is a light cell:
/// a cell's corners in image coordinates go back to the frame (undo the offset, the tiling, the turn), are cut to the frame and bent with its corners
pub(crate) fn warp_checker(quad: [Pos2; 4], placement: Placement) -> Vec<(Vec<Pos2>, bool)> {
    let [top_left, top_right, bottom_right, bottom_left] = quad;
    let at = |[u, v]: [f32; 2]| -> Pos2 {
        let top = top_left + (top_right - top_left) * u;
        let bottom = bottom_left + (bottom_right - bottom_left) * u;
        top + (bottom - top) * v
    };
    let tiling = placement.tiling.map(|tiles| if tiles.is_finite() { tiles.clamp(1e-3, MAX_TILES) } else { 1.0 });
    // The checker repeats every tile, so only the offset within one tile matters (and the cell numbers stay small)
    let offset = placement.offset.map(|offset| if offset.is_finite() { offset.rem_euclid(1.0) } else { 0.0 });
    let rotation = if placement.rotation.is_finite() { placement.rotation.rem_euclid(360.0) } else { 0.0 };
    let (sin, cos) = rotation.to_radians().sin_cos();
    // The frame (0..1 across and down) → image coordinates in cells: turn, tile, offset, as the compositor places a warp's image
    let to_cells = |[u, v]: [f32; 2]| -> [f32; 2] {
        let (x, y) = (u * cos - v * sin, u * sin + v * cos);
        [(x * tiling[0] + offset[0]) * CELLS_PER_TILE, (y * tiling[1] + offset[1]) * CELLS_PER_TILE]
    };
    let to_frame = |[x, y]: [f32; 2]| -> [f32; 2] {
        let (x, y) = ((x / CELLS_PER_TILE - offset[0]) / tiling[0], (y / CELLS_PER_TILE - offset[1]) / tiling[1]);
        [x * cos + y * sin, -x * sin + y * cos]
    };
    let corners = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]].map(to_cells);
    let low = |axis: usize| corners.iter().map(|c| c[axis]).fold(f32::INFINITY, f32::min).floor() as i32;
    let high = |axis: usize| corners.iter().map(|c| c[axis]).fold(f32::NEG_INFINITY, f32::max).ceil() as i32;
    let mut cells = Vec::new();
    for x in low(0)..high(0) {
        for y in low(1)..high(1) {
            let (x0, y0) = (x as f32, y as f32);
            let outline = clip_to_frame([[x0, y0], [x0 + 1.0, y0], [x0 + 1.0, y0 + 1.0], [x0, y0 + 1.0]].map(to_frame).to_vec());
            if area(&outline) > MIN_CELL_AREA {
                cells.push((outline.into_iter().map(at).collect(), (x + y).rem_euclid(2) == 0));
            }
        }
    }
    cells
}

/// Least share of the frame a cut cell must cover to be drawn (slivers left by rounding at the frame's sides are not)
const MIN_CELL_AREA: f32 = 1e-6;

/// The area of polygon `outline` (shoelace)
fn area(outline: &[[f32; 2]]) -> f32 {
    let twice: f32 = outline.iter().zip(outline.iter().cycle().skip(1)).map(|(a, b)| a[0] * b[1] - b[0] * a[1]).sum();
    twice.abs() * 0.5
}

/// The part of convex polygon `polygon` (frame coordinates) inside the frame's 0..1 square (Sutherland–Hodgman, one side at a time)
fn clip_to_frame(mut polygon: Vec<[f32; 2]>) -> Vec<[f32; 2]> {
    // Each side of the square: the axis it bounds, and whether points must stay below (1) or above (0) it
    for (axis, bound, below) in [(0, 0.0, false), (0, 1.0, true), (1, 0.0, false), (1, 1.0, true)] {
        let inside = |p: &[f32; 2]| if below { p[axis] <= bound } else { p[axis] >= bound };
        let mut kept = Vec::with_capacity(polygon.len() + 1);
        for (point, next) in polygon.iter().zip(polygon.iter().cycle().skip(1)) {
            let next = *next;
            if inside(point) {
                kept.push(*point);
            }
            if inside(point) != inside(&next) {
                let t = (bound - point[axis]) / (next[axis] - point[axis]);
                kept.push([point[0] + (next[0] - point[0]) * t, point[1] + (next[1] - point[1]) * t]);
            }
        }
        polygon = kept;
        if polygon.is_empty() {
            break;
        }
    }
    polygon
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::pos2;

    const QUAD: [Pos2; 4] = [pos2(0.0, 0.0), pos2(100.0, 0.0), pos2(100.0, 100.0), pos2(0.0, 100.0)];
    const STILL: Placement = Placement { tiling: [1.0, 1.0], rotation: 0.0, offset: [0.0, 0.0] };

    fn rounded(cells: &[(Vec<Pos2>, bool)]) -> Vec<(Vec<[i32; 2]>, bool)> {
        cells.iter().map(|(outline, light)| (outline.iter().map(|p| [p.x.round() as i32, p.y.round() as i32]).collect(), *light)).collect()
    }

    #[test]
    fn an_untouched_image_shows_four_by_four_cells_filling_the_frame() {
        let cells = warp_checker(QUAD, STILL);
        assert_eq!(cells.len(), 16);
        assert!(cells.iter().all(|(outline, _)| outline.len() == 4), "whole cells");
        assert_eq!(cells.iter().filter(|(_, light)| *light).count(), 8, "a checker");
        let area: f32 = cells.iter().map(|(outline, _)| (outline[2].x - outline[0].x).abs() * (outline[2].y - outline[0].y).abs()).sum();
        assert!((area - 100.0 * 100.0).abs() < 1.0, "they fill the frame: {area}");
    }

    #[test]
    fn rotation_turns_the_checker_inside_the_frame() {
        let still = rounded(&warp_checker(QUAD, STILL));
        let turned = warp_checker(QUAD, Placement { rotation: 30.0, ..STILL });
        assert_ne!(rounded(&turned), still, "a turned image turns the checker");
        assert!(turned.iter().any(|(outline, _)| outline.len() != 4), "cells cut by the frame's sides");
        assert!(
            turned.iter().flat_map(|(outline, _)| outline).all(|p| (-0.01..=100.01).contains(&p.x) && (-0.01..=100.01).contains(&p.y)),
            "kept inside the frame"
        );
        assert_eq!(rounded(&warp_checker(QUAD, Placement { rotation: 360.0, ..STILL })), still, "a full turn looks unturned");
        assert_ne!(rounded(&warp_checker(QUAD, Placement { tiling: [2.0, 2.0], ..STILL })), still, "tiling shows");
        assert_ne!(rounded(&warp_checker(QUAD, Placement { offset: [0.1, 0.0], ..STILL })), still, "offset shows");
    }

    #[test]
    fn hostile_placements_draw_a_bounded_checker() {
        for placement in [
            Placement { tiling: [f32::NAN, 1e9], ..STILL },
            Placement { rotation: f32::INFINITY, ..STILL },
            Placement { offset: [f32::NAN, 1e30], ..STILL },
            Placement { tiling: [0.0, -5.0], ..STILL },
        ] {
            let cells = warp_checker(QUAD, placement);
            assert!(cells.len() <= 40 * 40, "{placement:?}: {} cells", cells.len());
        }
    }
}
