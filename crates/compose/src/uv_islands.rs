//! The UV island generators: a value per UV island at every working texel, built once per layout and settings

use std::sync::Arc;

use rayon::prelude::*;
use texturecraft_core::{UvRandomColorParameters, gradient};
use texturecraft_geom::TrianglePartition;
use texturecraft_texel::noise::cell_hash;

use crate::eval::Inputs;
use crate::working_field::FieldCache;

static CACHE: FieldCache = FieldCache::new();

/// UV Random Color at every working texel of `inp`: one value per UV island (0 outside the islands)
pub(crate) fn random_color_field(inp: &Inputs, random: &UvRandomColorParameters, seed: u32) -> Arc<Vec<f32>> {
    let mut settings = vec![seed, u32::from(random.custom_gradient)];
    settings.extend(random.gradient.iter().flat_map(|color| color.map(f32::to_bits)));
    CACHE.get_or_build(inp, &[], settings, |_| {
        let island_values = island_values(inp, |island| random_gray(random, seed, island));
        let gbuf = inp.gbuf;
        gbuf.tri.par_iter().map(|triangle| island_values.get(triangle).copied().unwrap_or(0.0)).collect()
    })
}

/// The value `value_of` gives each UV island, by triangle
fn island_values(inp: &Inputs, value_of: impl Fn(u32) -> f32) -> std::collections::HashMap<u32, f32> {
    let material = inp.project.texture_set(inp.ts).map_or(0, |set| set.material);
    let islands = TrianglePartition::uv_islands(inp.mesh, material);
    islands
        .groups
        .iter()
        .enumerate()
        .flat_map(|(island, triangles)| triangles.iter().map(move |triangle| (*triangle, island as u32)))
        .map(|(triangle, island)| (triangle, value_of(island)))
        .collect()
}

/// An island's gray: the mean of a random color's channels, or of the gradient's color at a random point
fn random_gray(random: &UvRandomColorParameters, seed: u32, island: u32) -> f32 {
    let island = island as i32;
    let color = if random.custom_gradient {
        gradient(&random.gradient, cell_hash(island, 0, 0, seed)).unwrap_or([0.0; 3])
    } else {
        std::array::from_fn(|channel| cell_hash(island, channel as i32 + 1, 0, seed))
    };
    color.iter().sum::<f32>() / 3.0
}
