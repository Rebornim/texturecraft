//! A small picture of one output map for the template editor, sampled from the texture set the way the export samples it

use super::{At, Dest, ExportInput, ExportOptions, Sampler, is_color, layout, list_outputs};
use crate::{FormatError, Result};
use rayon::prelude::*;
use texturecraft_texel::linear_to_srgb;

/// The largest preview side: a preview is a glance, never a second export
pub const MAX_PREVIEW_SIDE: u32 = 512;

/// One output map, small: its channels in file order and 8-bit values as the file stores them (colour sRGB-encoded)
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapPreview {
    pub width: u32,
    pub height: u32,
    pub channels: Vec<Dest>,
    /// `channels.len()` values per pixel, row by row
    pub pixels: Vec<u8>,
}

/// Map `map` of `opts`'s preset for `input`'s texture set, `side` pixels wide (the set's aspect kept); None when the set
/// writes no file for it. Texels off the UV islands show the channel's default, so the islands read at a glance.
pub fn preview_map(input: &ExportInput, opts: &ExportOptions, map: usize, side: u32) -> Result<Option<MapPreview>> {
    let set = input.project.texture_set(input.ts)?;
    let spec = opts.preset.maps.get(map).ok_or_else(|| FormatError::Invalid(format!("the template has no output map {map}")))?;
    if !list_outputs(input.project, input.ts, input.mesh_name, opts)?.iter().any(|planned| planned.map == map) {
        return Ok(None);
    }
    let (res, height) = (input.composite.resolution, input.composite.height);
    if res == 0 || height == 0 || input.coverage.len() != res as usize * height as usize {
        return Err(FormatError::Invalid(format!("composite size {res}×{height} does not match its coverage")));
    }

    let width = side.clamp(1, MAX_PREVIEW_SIDE).min(res);
    let rows = ((u64::from(width) * u64::from(height) / u64::from(res)) as u32).max(1);
    let tile_res = res / set.tile_grid().map_or(1, |grid| grid.cols.max(1));
    let sampler = Sampler::new(input, opts, set, tile_res.max(1));
    let lay = layout(set, spec);
    let n = lay.len();
    let color = is_color(spec);
    // The texel at the centre of each preview pixel's footprint
    let texel = |i: u32, out: u32, of: u32| ((u64::from(i) * 2 + 1) * u64::from(of) / (2 * u64::from(out))).min(u64::from(of) - 1) as u32;

    let mut pixels = vec![0u8; width as usize * rows as usize * n];
    pixels.par_chunks_mut(width as usize * n).enumerate().for_each(|(row, values)| {
        let y = texel(row as u32, rows, height);
        for column in 0..width {
            let x = texel(column, width, res);
            let at = if sampler.covered(x, y) { At::Texel(x, y) } else { At::Default };
            let mut cache = None;
            for (k, (dest, source)) in lay.iter().enumerate() {
                let value = sampler.sample(source, at, &mut cache);
                let stored = if color && *dest != Dest::A { linear_to_srgb(value) } else { value.clamp(0.0, 1.0) };
                if let Some(slot) = values.get_mut(column as usize * n + k) {
                    *slot = (stored * 255.0).round() as u8;
                }
            }
        }
    });

    Ok(Some(MapPreview { width, height: rows, channels: lay.iter().map(|(dest, _)| *dest).collect(), pixels }))
}
