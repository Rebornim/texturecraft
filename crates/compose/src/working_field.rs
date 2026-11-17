//! Whole-map fields some generators precompute at the working resolution (UV Border Distance, Dripping Rust), built once per layout, mesh map and settings

use std::sync::{Arc, Mutex, Weak};

use texturecraft_core::BlockScales;

use crate::eval::{Inputs, blur};

/// Most fields cached, and the bytes they may hold (the newest is kept whatever its size)
const CACHE_ENTRIES: usize = 8;
const CACHE_BYTES: usize = 256 << 20;

/// Texels between the samples a layout's fingerprint reads: dense enough to tell UV edits apart, sparse enough to read once per tile
const FINGERPRINT_STRIDE: usize = 31;

/// What a field was built from: the working layout (the g-buffer's address, size, coverage fingerprint, UV tile block and tile scales), the mesh, the mesh maps it reads and its settings' bits
#[derive(Clone)]
struct Key {
    layout: usize,
    size: (u32, u32),
    fingerprint: u64,
    block: (u32, u32),
    scales: BlockScales,
    mesh: (usize, usize),
    maps: Vec<Weak<Vec<f32>>>,
    settings: Vec<u32>,
}

impl PartialEq for Key {
    fn eq(&self, other: &Key) -> bool {
        let same_maps = self.maps.len() == other.maps.len() && self.maps.iter().zip(&other.maps).all(|(a, b)| a.strong_count() > 0 && a.ptr_eq(b));
        same_maps
            && self.layout == other.layout
            && self.size == other.size
            && self.fingerprint == other.fingerprint
            && self.block == other.block
            && self.scales == other.scales
            && self.mesh == other.mesh
            && self.settings == other.settings
    }
}

/// Built fields by what they were built from, most recently used last
pub(crate) struct FieldCache {
    entries: Mutex<Vec<(Key, Arc<Vec<f32>>)>>,
}

impl FieldCache {
    pub(crate) const fn new() -> FieldCache {
        FieldCache { entries: Mutex::new(Vec::new()) }
    }

    /// The field of `inp`'s working layout reading `maps` with `settings`, built by `build` (given the UV tile block) when missing, outside the lock: the build runs parallel work
    pub(crate) fn get_or_build(&self, inp: &Inputs, maps: &[&Arc<Vec<f32>>], settings: Vec<u32>, build: impl FnOnce((u32, u32)) -> Vec<f32>) -> Arc<Vec<f32>> {
        let gbuf = inp.gbuf;
        let block = working_block(inp);
        let key = Key {
            layout: gbuf.tri.as_ptr() as usize,
            size: (gbuf.width, gbuf.height),
            fingerprint: gbuf.tri.iter().step_by(FINGERPRINT_STRIDE).fold(gbuf.tri.len() as u64, |hash, tri| hash.rotate_left(5) ^ u64::from(*tri)),
            block,
            scales: inp.block_scales.clone(),
            mesh: (inp.mesh.positions.as_ptr() as usize, inp.mesh.positions.len()),
            maps: maps.iter().map(|map| Arc::downgrade(map)).collect(),
            settings,
        };

        let cached = {
            let mut entries = self.entries.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            entries.retain(|(key, _)| key.maps.iter().all(|map| map.strong_count() > 0));
            entries.iter().find(|(cached, _)| *cached == key).map(|(_, field)| field.clone())
        };
        if let Some(field) = cached {
            return field;
        }

        let field = Arc::new(build(block));
        let mut entries = self.entries.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        entries.retain(|(cached, _)| *cached != key);
        entries.push((key, field.clone()));
        let mut bytes = 0usize;
        let keep = entries
            .iter()
            .rev()
            .take_while(|(_, built)| {
                bytes = bytes.saturating_add(built.len().saturating_mul(size_of::<f32>()));
                bytes <= CACHE_BYTES
            })
            .count()
            .clamp(1, CACHE_ENTRIES);
        let excess = entries.len().saturating_sub(keep);
        entries.drain(..excess);
        field
    }
}

/// The size of one UV tile's block in `inp`'s working atlas
pub(crate) fn working_block(inp: &Inputs) -> (u32, u32) {
    let size = (inp.gbuf.width, inp.gbuf.height);
    let grid = inp.project.texture_set(inp.ts).ok().and_then(|set| set.tile_grid());
    let block = grid.map_or(size, |grid| (size.0 / grid.cols.max(1), size.1 / grid.rows.max(1)));
    (block.0.clamp(1, size.0.max(1)), block.1.clamp(1, size.1.max(1)))
}

/// `data` blurred one UV tile block at a time by the radius `radius_at` gives for the block's corner, reading black past each block's edges
pub(crate) fn blur_blocks(data: &mut [f32], (width, height): (u32, u32), block: (u32, u32), radius_at: impl Fn(u32, u32) -> usize) {
    for top in (0..height).step_by(block.1 as usize) {
        for left in (0..width).step_by(block.0 as usize) {
            let (columns, rows) = (block.0.min(width - left) as usize, block.1.min(height - top) as usize);
            let index = |column: usize, row: usize| (top as usize + row) * width as usize + left as usize + column;
            let mut values: Vec<f32> = (0..rows)
                .flat_map(|row| (0..columns).map(move |column| (column, row)))
                .map(|(column, row)| data.get(index(column, row)).copied().unwrap_or(0.0))
                .collect();
            let radius = radius_at(left, top);
            if radius == 0 {
                continue;
            }
            blur(&mut values, columns, rows, radius);
            for (offset, value) in values.into_iter().enumerate() {
                if let Some(target) = data.get_mut(index(offset % columns, offset / columns)) {
                    *target = value;
                }
            }
        }
    }
}
