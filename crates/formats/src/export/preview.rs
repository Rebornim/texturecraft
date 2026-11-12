//! A small picture of one output map for the template editor, sampled from the texture set the way the export samples it

use super::map_preview::MapPreview;
use super::{At, Dest, ExportInput, ExportOptions, Sampler, is_color, layout, list_outputs};
use crate::{FormatError, Result};
use rayon::prelude::*;
use texturecraft_texel::linear_to_srgb;

/// The largest preview side: a preview is a glance, never a second export
pub const MAX_PREVIEW_SIDE: u32 = 512;

/// Output map `map`, `side` texels wide in the set's aspect; None when the set writes no file for it
pub fn preview_map(input: &ExportInput, opts: &ExportOptions, map: usize, side: u32) -> Result<Option<MapPreview>> {
    let set = input.project.texture_set(input.ts)?;
    let spec = opts.preset.maps.get(map).ok_or_else(|| FormatError::Invalid(format!("the template has no output map {map}")))?;
    if !list_outputs(input.project, input.ts, input.mesh_name, opts)?.iter().any(|planned| planned.maps().any(|planned_map| planned_map == map)) {
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
    let channel_layout = layout(set, spec);
    let component_count = channel_layout.len();
    let color = is_color(spec);
    // The texel at the centre of each preview pixel's footprint
    let texel = |index: u32, preview_length: u32, source_length: u32| {
        ((u64::from(index) * 2 + 1) * u64::from(source_length) / (2 * u64::from(preview_length))).min(u64::from(source_length) - 1) as u32
    };

    let mut pixels = vec![0u8; width as usize * rows as usize * component_count];
    pixels.par_chunks_mut(width as usize * component_count).enumerate().for_each(|(row, values)| {
        let y = texel(row as u32, rows, height);
        for column in 0..width {
            let x = texel(column, width, res);
            // Off the UV islands shows the channel's default, so the islands read at a glance
            let at = if sampler.covered(x, y) { At::Texel(x, y) } else { At::Default };
            let mut cache = None;
            for (component, (dest, source)) in channel_layout.iter().enumerate() {
                let value = sampler.sample(source, at, &mut cache);
                let stored = if color && *dest != Dest::A { linear_to_srgb(value) } else { value.clamp(0.0, 1.0) };
                if let Some(slot) = values.get_mut(column as usize * component_count + component) {
                    *slot = (stored * 255.0).round() as u8;
                }
            }
        }
    });

    Ok(Some(MapPreview { width, height: rows, channels: channel_layout.iter().map(|(dest, _)| *dest).collect(), pixels }))
}
