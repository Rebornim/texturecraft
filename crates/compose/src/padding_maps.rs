//! A texture set's UV padding maps, one per kind of channel

use texturecraft_core::{Channel, ChannelKind, UvPaddingMode};
use texturecraft_geom::UvSeam;
use texturecraft_texel::dilate::DilationMap;

use crate::seam_padding::{SEAM_REACH, cross_seams};

/// A texture set's padding maps: `all` for its channels, `flat` for tangent-space normals
#[derive(Clone, Debug, PartialEq)]
pub struct PaddingMaps {
    pub all: DilationMap,
    /// Padding from island edges (2D) always: a tangent-space normal turns with its island, so it can't continue across a seam
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

#[cfg(test)]
mod tests {
    use texturecraft_geom::UvSeamSide;
    use texturecraft_geom::glam::Vec2;

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
