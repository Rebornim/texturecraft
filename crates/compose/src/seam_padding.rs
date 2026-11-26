//! UV padding across seams (3D): texels past a UV seam continue the surface from the seam's other side

use texturecraft_geom::{UvSeam, glam::Vec2};
use texturecraft_texel::dilate::{DilationMap, NONE};

use crate::texel_edge::TexelEdge;

/// Farthest a padding texel reaches across a seam, in texels; farther ones keep their nearest covered texel
pub const SEAM_REACH: u32 = 32;

/// Point every padding texel just outside a seam at the texel as far inside the seam's other side
pub(crate) fn cross_seams(map: &mut DilationMap, covered: &[bool], reach: u32, block: (u32, u32), seams: &[UvSeam]) {
    let (width, height) = (map.width, map.height);
    let size = Vec2::new(width as f32, height as f32);
    let block_of = |x: u32, y: u32| (x.checked_div(block.0).unwrap_or(0), y.checked_div(block.1).unwrap_or(0));
    let is_covered = |x: u32, y: u32| covered.get((y * width + x) as usize).copied().unwrap_or(false);
    let mut crossed_at = vec![f32::INFINITY; map.src.len()];
    for seam in seams {
        for (near, far) in [(seam.sides[0], seam.sides[1]), (seam.sides[1], seam.sides[0])] {
            let (Some(near_edge), Some(far_edge)) = (TexelEdge::new(near, size), TexelEdge::new(far, size)) else {
                continue;
            };

            let reach = reach as f32;
            let low = near_edge.start.min(near_edge.end) - Vec2::splat(reach);
            let high = near_edge.start.max(near_edge.end) + Vec2::splat(reach);
            let (x0, y0) = (low.x.floor().max(0.0) as u32, low.y.floor().max(0.0) as u32);
            let (x1, y1) = (high.x.ceil().min(size.x) as u32, high.y.ceil().min(size.y) as u32);
            for y in y0..y1 {
                for x in x0..x1 {
                    let index = (y * width + x) as usize;
                    let Some(&source) = map.src.get(index) else { continue };
                    if source == NONE || source == index as u32 {
                        continue;
                    }

                    let point = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
                    let Some((along, outside)) = near_edge.outside(point) else { continue };
                    if outside > reach || outside >= crossed_at.get(index).copied().unwrap_or(0.0) {
                        continue;
                    }

                    // Another island's edge nearer than the seam keeps its texel
                    let (source_x, source_y) = (source % width, source / width);
                    if (source_x.abs_diff(x).max(source_y.abs_diff(y)) as f32) + 1.0 < outside {
                        continue;
                    }

                    // Padding stays inside its UV tile, like the 2D padding
                    let inside = near_edge.start.lerp(near_edge.end, along) + near_edge.inward * 0.5;
                    if block_of(inside.x.max(0.0) as u32, inside.y.max(0.0) as u32) != block_of(x, y) {
                        continue;
                    }

                    // As far inside the other side as the texel is outside this one, at least to that side's first texel centres
                    let target = far_edge.start.lerp(far_edge.end, along) + far_edge.inward * outside.max(0.5);
                    if target.x < 0.0 || target.y < 0.0 || target.x >= size.x || target.y >= size.y {
                        continue;
                    }

                    let (target_x, target_y) = (target.x as u32, target.y as u32);
                    if is_covered(target_x, target_y)
                        && let (Some(destination), Some(at)) = (map.src.get_mut(index), crossed_at.get_mut(index))
                    {
                        *destination = target_y * width + target_x;
                        *at = outside;
                    }
                }
            }
        }
    }
}
