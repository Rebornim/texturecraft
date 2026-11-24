use std::sync::{Arc, Mutex};

use crate::eval::Inputs;
use crate::field_cache_key::FieldCacheKey;
use crate::working_field::working_block;

/// Most fields cached, and the bytes they may hold (the newest is kept whatever its size)
const CACHE_ENTRIES: usize = 8;
const CACHE_BYTES: usize = 256 << 20;

/// Texels between the samples a layout's fingerprint reads: dense enough to tell UV edits apart, sparse enough to read once per tile
const FINGERPRINT_STRIDE: usize = 31;

/// Built fields by what they were built from, most recently used last
pub(crate) struct FieldCache {
    entries: Mutex<Vec<(FieldCacheKey, Arc<Vec<f32>>)>>,
}

impl FieldCache {
    pub(crate) const fn new() -> FieldCache {
        FieldCache { entries: Mutex::new(Vec::new()) }
    }

    /// The field of `inp`'s working layout reading `maps` with `settings`, built by `build` (given the UV tile block) when missing, outside the lock: the build runs parallel work
    pub(crate) fn get_or_build(&self, inp: &Inputs, maps: &[&Arc<Vec<f32>>], settings: Vec<u32>, build: impl FnOnce((u32, u32)) -> Vec<f32>) -> Arc<Vec<f32>> {
        let gbuf = inp.gbuf;
        let block = working_block(inp);
        let key = FieldCacheKey {
            layout: gbuf.tri.as_ptr() as usize,
            size: (gbuf.width, gbuf.height),
            fingerprint: gbuf.tri.iter().step_by(FINGERPRINT_STRIDE).fold(gbuf.tri.len() as u64, |hash, tri| hash.rotate_left(5) ^ u64::from(*tri)),
            block,
            scales: inp.block_scales.clone(),
            mesh: (inp.mesh.positions.as_ptr() as usize, inp.mesh.positions.len(), inp.mesh.uvs.as_ptr() as usize, inp.mesh.uvs.len()),
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
