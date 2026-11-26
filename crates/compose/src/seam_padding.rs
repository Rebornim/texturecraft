//! 3D Space Neighbor UV padding: texels past a UV seam continue the surface from the seam's other side

use texturecraft_core::{Channel, ChannelKind, UvPaddingMode};
use texturecraft_geom::{UvSeam, UvSeamSide, glam::Vec2};
use texturecraft_texel::dilate::{DilationMap, NONE};

/// Farthest a padding texel reaches across a seam, in texels; farther ones keep their nearest covered texel
pub const SEAM_REACH: u32 = 32;

/// A texture set's padding maps: `all` for its channels, `flat` for tangent-space normals
#[derive(Clone, Debug, PartialEq)]
pub struct PaddingMaps {
    pub all: DilationMap,
    /// 2D Space Neighbor always: a tangent-space normal turns with its island, so it can't continue across a seam
    pub flat: Option<DilationMap>,
}

impl PaddingMaps {
    /// The maps for `mode`: `covered` texels of a `width × height` set padded up to `max_dist` texels inside each `block`
    pub fn new(mode: UvPaddingMode, covered: &[bool], (width, height): (u32, u32), max_dist: Option<u32>, block: (u32, u32), seams: &[UvSeam]) -> PaddingMaps {
        let flat = DilationMap::in_rectangular_blocks(covered, width, height, max_dist, block);
        match mode {
            UvPaddingMode::Space2d => PaddingMaps { all: flat, flat: None },
            UvPaddingMode::Space3d => {
                let mut all = flat.clone();
                cross_seams(&mut all, covered, max_dist.map_or(SEAM_REACH, |d| d.min(SEAM_REACH)), block, seams);
                PaddingMaps { all, flat: Some(flat) }
            }
        }
    }

    /// The map padding tangent-space normals
    pub fn flat(&self) -> &DilationMap {
        self.flat.as_ref().unwrap_or(&self.all)
    }

    /// The map padding `channel`
    pub fn for_channel(&self, channel: Channel) -> &DilationMap {
        if channel.kind() == ChannelKind::NormalMap { self.flat() } else { &self.all }
    }
}

/// Point every padding texel just outside a seam at the texel as far inside the seam's other side
fn cross_seams(map: &mut DilationMap, covered: &[bool], reach: u32, block: (u32, u32), seams: &[UvSeam]) {
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

/// A seam side in texel space, with its unit normal into the island
struct TexelEdge {
    start: Vec2,
    end: Vec2,
    inward: Vec2,
}

impl TexelEdge {
    fn new(side: UvSeamSide, size: Vec2) -> Option<TexelEdge> {
        let (start, end, interior) = (side.start * size, side.end * size, side.interior * size);
        let normal = (end - start).perp().try_normalize()?;
        let inward = if normal.dot(interior - start) < 0.0 { -normal } else { normal };
        Some(TexelEdge { start, end, inward })
    }

    /// How far along the edge (0..1) and how far outside the island `point` is, when it is beside the edge on the outside
    fn outside(&self, point: Vec2) -> Option<(f32, f32)> {
        let direction = self.end - self.start;
        let along = (point - self.start).dot(direction) / direction.length_squared();
        let outside = -(point - self.start).dot(self.inward);
        ((0.0..=1.0).contains(&along) && outside > 0.0).then_some((along, outside))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two islands of one 3D edge: the left one's right side is the right one's left side
    fn two_islands() -> (Vec<bool>, Vec<UvSeam>) {
        let (width, height) = (32u32, 8u32);
        let covered = (0..width * height).map(|i| matches!(i % width, 4..=11 | 20..=27)).collect();
        let at = |x: f32, y: f32| Vec2::new(x / width as f32, y / height as f32);
        let left = UvSeamSide { start: at(12.0, 0.0), end: at(12.0, 8.0), interior: at(4.0, 0.0) };
        let right = UvSeamSide { start: at(20.0, 0.0), end: at(20.0, 8.0), interior: at(28.0, 0.0) };
        (covered, vec![UvSeam { sides: [left, right] }])
    }

    #[test]
    fn padding_past_a_seam_continues_from_its_other_side_and_normals_stay_flat() {
        let (covered, seams) = two_islands();
        let space_3d = PaddingMaps::new(UvPaddingMode::Space3d, &covered, (32, 8), Some(4), (0, 0), &seams);
        let space_2d = PaddingMaps::new(UvPaddingMode::Space2d, &covered, (32, 8), Some(4), (0, 0), &seams);
        let island = |map: &DilationMap, x: u32| map.source(x, 3).map(|(source_x, _)| if source_x < 16 { "left" } else { "right" });

        // Just right of the left island, then just left of the right island
        assert_eq!((island(&space_3d.all, 13), island(&space_3d.all, 18)), (Some("right"), Some("left")));
        assert_eq!((island(&space_2d.all, 13), island(&space_2d.all, 18)), (Some("left"), Some("right")));
        assert_eq!(space_3d.for_channel(Channel::Normal), &space_2d.all);
        assert_eq!(space_3d.for_channel(Channel::BaseColor), &space_3d.all);

        // Away from the seam, both modes pad from the near island
        assert_eq!(island(&space_3d.all, 2), Some("left"));
    }
}
