use rayon::prelude::*;
use texturecraft_core::TextureSet;
use texturecraft_texel::{ColorSpace, DilationMap};

use super::rendered_map::RenderedMap;
use super::{At, Dest, ExportInput, ExportOptions, FileFormat, Padding, Sampler, diffusion, is_color, layout, resize};
use crate::{FormatError, MAX_IMAGE_SIZE, Result};

/// Renders a texture set's output maps: the set's sampler and padding, shared by every map
pub(super) struct MapRenderer<'a> {
    opts: &'a ExportOptions,
    set: &'a TextureSet,
    sampler: Sampler<'a>,
    dilation_map: Option<DilationMap>,
    /// Padding beyond the dilation takes each channel's default
    beyond_default: bool,
    /// The composite's (width, height), and the side of one UV tile's block
    resolution: u32,
    height: u32,
    tile_res: u32,
}

impl<'a> MapRenderer<'a> {
    pub fn new(input: &ExportInput<'a>, opts: &'a ExportOptions) -> Result<MapRenderer<'a>> {
        let set = input.project.texture_set(input.ts)?;
        let (resolution, height) = (input.composite.resolution, input.composite.height);
        let grid = set.tile_grid();
        let tile_res = resolution / grid.map_or(1, |tile_grid| tile_grid.cols.max(1));
        if tile_res == 0 || tile_res > MAX_IMAGE_SIZE || height != tile_res * grid.map_or(1, |tile_grid| tile_grid.rows) {
            return Err(FormatError::Invalid(format!("composite size {resolution}×{height} does not fit texture set \"{}\"", set.name)));
        }
        if input.coverage.len() != resolution as usize * height as usize {
            return Err(FormatError::Invalid(format!("coverage has {} texels, expected {resolution}×{height}", input.coverage.len())));
        }

        // Padding stays inside each UV tile
        let dilation_map = match opts.padding {
            Padding::None => None,
            Padding::Dilation(distance) | Padding::DilationDefault(distance) | Padding::DilationDiffusion(distance) => {
                Some(DilationMap::in_blocks(input.coverage, resolution, height, Some(distance), tile_res))
            }
            Padding::Infinite => Some(DilationMap::in_blocks(input.coverage, resolution, height, None, tile_res)),
        };
        let beyond_default = matches!(opts.padding, Padding::DilationDefault(_) | Padding::Infinite);
        let sampler = Sampler::new(input, opts, set, tile_res);
        Ok(MapRenderer { opts, set, sampler, dilation_map, beyond_default, resolution, height, tile_res })
    }

    /// Output map `map_index` for UV tile `tile` (None: the whole set), as file type `format` will hold it; `name` is the file's, for warnings
    pub fn render(&self, map_index: usize, tile: Option<u32>, format: FileFormat, name: &str, warnings: &mut Vec<String>) -> Result<RenderedMap> {
        let (opts, set, sampler) = (self.opts, self.set, &self.sampler);
        let map = opts.preset.maps.get(map_index).ok_or_else(|| FormatError::Invalid("planned map out of range".into()))?;
        let channel_layout = layout(set, map);
        let component_count = channel_layout.len();

        // The texels written: one tile, or the whole set
        let tile_res = self.tile_res;
        let (origin_x, origin_y, written_width, written_height) = match (tile, set.tile_grid()) {
            (Some(tile), Some(tile_grid)) => {
                let (block_column, block_row) =
                    tile_grid.block(tile).ok_or_else(|| FormatError::Invalid(format!("UV tile {tile} is not in the set's grid")))?;

                // A tile smaller than its block fills the block's top-left corner
                let side = ((tile_res as f32 * set.tile_scale(tile)).round() as u32).clamp(1, tile_res);
                (block_column * tile_res, block_row * tile_res, side, side)
            }
            _ => (0, 0, self.resolution, self.height),
        };

        let mut data = vec![0.0f32; written_width as usize * written_height as usize * component_count];
        data.par_chunks_mut(written_width as usize * component_count).enumerate().for_each(|(row_index, row)| {
            let y = origin_y + row_index as u32;
            for (column, x) in (origin_x..origin_x + written_width).enumerate() {
                let at = if sampler.covered(x, y) {
                    Some(At::Texel(x, y))
                } else {
                    match self.dilation_map.as_ref().and_then(|dilation| dilation.source(x, y)) {
                        Some((source_x, source_y)) => Some(At::Texel(source_x, source_y)),
                        None => self.beyond_default.then_some(At::Default),
                    }
                };
                let Some(at) = at else { continue };

                let mut cache = None;
                for (component, (_, source)) in channel_layout.iter().enumerate() {
                    if let Some(value) = row.get_mut(column * component_count + component) {
                        *value = sampler.sample(source, at, &mut cache);
                    }
                }
            }
        });

        if matches!(opts.padding, Padding::DilationDiffusion(_)) {
            let valid: Vec<bool> = (origin_y..origin_y + written_height)
                .flat_map(|y| (origin_x..origin_x + written_width).map(move |x| (x, y)))
                .map(|(x, y)| sampler.covered(x, y) || self.dilation_map.as_ref().is_some_and(|dilation| dilation.source(x, y).is_some()))
                .collect();
            diffusion::diffuse(&mut data, &valid, (written_width, written_height), component_count);
        }

        // `size` is per tile: an atlas scales with it
        let scale = |length: u32| (u64::from(length) * u64::from(map.options.size.or(opts.size).unwrap_or(tile_res)) / u64::from(tile_res)) as u32;
        let size = (scale(written_width), scale(written_height));
        let mut data = resize(data, (written_width, written_height), component_count, size);

        // Colour maps leave linear Rec.709 here, except sRGB in integer files, which encoding applies
        let space = opts.space_for(format);
        let is_rgb = channel_layout.len() >= 3 && channel_layout.iter().take(3).map(|(dest, _)| *dest).eq([Dest::R, Dest::G, Dest::B]);
        let convert = is_color(map) && (space != ColorSpace::Srgb || format.is_float()) && space != ColorSpace::LinearRec709;
        if convert && is_rgb {
            data.par_chunks_mut(component_count).for_each(|texel| {
                if let [red, green, blue, ..] = texel {
                    [*red, *green, *blue] = space.from_linear_rec709([*red, *green, *blue]);
                }
            });
        } else if convert {
            warnings.push(format!("\"{name}\" packs colour into one channel (L), so it is written in linear Rec.709, not {}", space.name()));
        }

        let srgb = channel_layout.iter().map(|(dest, _)| *dest != Dest::A && is_color(map) && !format.is_float() && space == ColorSpace::Srgb).collect();

        // A colour space the user chose is written into the colour maps (data maps have none)
        let embed = (opts.color_space.is_some() && is_color(map) && is_rgb).then_some(space);
        let dithered = map.options.dithering || opts.dithered_maps.contains(&map_index);
        Ok(RenderedMap { data, component_count, size, srgb, embed, dithered })
    }
}
