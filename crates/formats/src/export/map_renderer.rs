use rayon::prelude::*;
use texturecraft_core::{TextureSet, TileGrid};
use texturecraft_texel::{ColorSpace, DilationMap};

use super::rendered_map::RenderedMap;
use super::{At, Dest, ExportInput, ExportOptions, FileFormat, Padding, Sampler, diffuse_blocks, layout, resize, scale_export_side, tile_side};
use crate::{FormatError, MAX_IMAGE_SIZE, Result};

/// Renders the output maps of one texture set's export: the padding and sampling every map shares
pub(super) struct MapRenderer<'a> {
    options: &'a ExportOptions,
    set: &'a TextureSet,
    sampler: Sampler<'a>,
    dilation_map: Option<DilationMap>,
    grid: Option<TileGrid>,
    /// The composite's size
    width: u32,
    height: u32,
    /// Texels per atlas block (the width for a set without UV tiles)
    tile_res: u32,
}

impl<'a> MapRenderer<'a> {
    pub(super) fn new(input: &ExportInput<'a>, options: &'a ExportOptions) -> Result<MapRenderer<'a>> {
        let set = input.project.texture_set(input.ts)?;
        let (width, height) = (input.composite.resolution, input.composite.height);
        let grid = set.tile_grid();
        let tile_res = width / grid.map_or(1, |g| g.cols.max(1));
        if tile_res == 0 || tile_res > MAX_IMAGE_SIZE || height != tile_res * grid.map_or(1, |g| g.rows) {
            return Err(FormatError::Invalid(format!("composite size {width}×{height} does not fit texture set \"{}\"", set.name)));
        }

        if input.coverage.len() as u64 != u64::from(width) * u64::from(height) {
            return Err(FormatError::Invalid(format!("coverage has {} texels, expected {width}×{height}", input.coverage.len())));
        }

        // Padding stays inside each UV tile
        let dilation_map = match options.padding {
            Padding::None => None,
            Padding::Dilation(d) | Padding::DilationDefault(d) | Padding::DilationDiffusion(d) => {
                Some(DilationMap::in_blocks(input.coverage, width, height, Some(d), tile_res))
            }
            Padding::Infinite => Some(DilationMap::in_blocks(input.coverage, width, height, None, tile_res)),
        };
        let sampler = Sampler::new(input, options, set, tile_res, false);
        Ok(MapRenderer { options, set, sampler, dilation_map, grid, width, height, tile_res })
    }

    /// The export size of map `map_index`'s file of `tile` (None: the whole set)
    pub(super) fn size(&self, map_index: usize, tile: Option<u32>) -> Result<(u32, u32)> {
        let (_, _, w, h) = self.region(tile)?;

        // An atlas scales by the requested size per tile
        let scale = |value| scale_export_side(value, self.tile_res, self.options.map_size(map_index));
        Ok((scale(w), scale(h)))
    }

    /// The composite texels a file of `tile` covers: one tile, or the whole set
    fn region(&self, tile: Option<u32>) -> Result<(u32, u32, u32, u32)> {
        match (tile, self.grid) {
            (Some(t), Some(g)) => {
                let (bx, by) = g.block(t).ok_or_else(|| FormatError::Invalid(format!("UV tile {t} is not in the set's grid")))?;

                // A tile smaller than its block fills the block's top-left corner
                let side = tile_side(self.set, t, self.tile_res);
                Ok((bx * self.tile_res, by * self.tile_res, side, side))
            }
            _ => Ok((0, 0, self.width, self.height)),
        }
    }

    /// Map `map_index` of the preset for `tile`, as `format` at `bit_depth` holds it
    pub(super) fn render(&self, map_index: usize, tile: Option<u32>, format: FileFormat, bit_depth: u8) -> Result<RenderedMap> {
        let options = self.options;
        let map = options.preset.maps.get(map_index).ok_or_else(|| FormatError::Invalid("planned map out of range".into()))?;
        let channel_layout = layout(self.set, map);
        let component_count = channel_layout.len();
        let inverted: Vec<bool> = channel_layout.iter().map(|(d, _)| map.inverted.contains(d)).collect();
        let (x0, y0, w, h) = self.region(tile)?;
        let size = self.size(map_index, tile)?;
        let sampler = &self.sampler;
        let dilation_map = self.dilation_map.as_ref();
        let beyond_default = matches!(options.padding, Padding::DilationDefault(_) | Padding::Infinite);

        let mut data = vec![0.0f32; w as usize * h as usize * component_count];
        data.par_chunks_mut(w as usize * component_count).enumerate().for_each(|(row_y, row)| {
            let y = y0 + row_y as u32;
            for (column, x) in (x0..x0 + w).enumerate() {
                let at = if sampler.covered(x, y) {
                    Some(At::Texel(x, y))
                } else {
                    match dilation_map.and_then(|d| d.source(x, y)) {
                        Some((sx, sy)) => Some(At::Texel(sx, sy)),
                        None => beyond_default.then_some(At::Default),
                    }
                };
                let Some(at) = at else { continue };
                let mut cache = None;
                for (component, (_, source)) in channel_layout.iter().enumerate() {
                    if let Some(value) = row.get_mut(column * component_count + component) {
                        let sample = sampler.sample(source, at, &mut cache);
                        *value = if inverted.get(component).copied().unwrap_or(false) { 1.0 - sample } else { sample };
                    }
                }
            }
        });

        // Diffuse into the texels the dilation did not reach
        if let (Padding::DilationDiffusion(_), Some(dilation)) = (options.padding, dilation_map) {
            let block = if tile.is_none() && self.grid.is_some() { self.tile_res } else { w.max(h) };
            diffuse_blocks(&mut data, (w as usize, h as usize), component_count, dilation, (x0 as usize, y0 as usize), block as usize);
        }

        let mut data = resize(data, (w, h), component_count, size);

        // Integer sRGB converts in `encode`; other color spaces convert here
        let space = options.space_for_map(map, format, bit_depth);
        let float = format.holds_float(bit_depth);
        let convert = (space != ColorSpace::Srgb || float) && space != ColorSpace::LinearRec709;
        if convert {
            data.par_chunks_mut(component_count).for_each(|px| {
                if let [r, g, b, ..] = px {
                    [*r, *g, *b] = space.from_linear_rec709([*r, *g, *b]);
                }
            });
        }
        let srgb = channel_layout.iter().map(|(d, _)| *d != Dest::A && !float && space == ColorSpace::Srgb).collect();

        // A colour space the user chose is written into the colour maps (data maps have none)
        let embed = (options.color_space.is_some() && map.is_color() && component_count >= 3).then_some(space);
        Ok(RenderedMap { data, size, component_count, srgb, embed })
    }
}
