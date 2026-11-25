//! What each baked map was baked from, and which maps are out of date since

use std::collections::BTreeMap;

use texturecraft_core::{BakeChange, BakeSource, BakedMap, CageType, fingerprint_words};

use crate::{Result, Session};

impl Session {
    /// What a bake of `map` on texture set `texture_set` would read now
    pub fn bake_source(&self, texture_set: usize, map: BakedMap) -> Result<BakeSource> {
        let set = self.doc.project.texture_set(texture_set)?;
        let from_high = !set.bake.high_poly.use_low_as_high && !self.doc.bake_meshes.high.is_empty();
        let high_poly = match from_high {
            true => fingerprint_words(self.doc.bake_meshes.high.iter().map(|high| high.fingerprint)),
            false => 0,
        };
        let reads_cage = from_high && set.bake.cage.kind == CageType::CustomFile;
        let cage = self.doc.bake_meshes.cage.as_ref().filter(|_| reads_cage).map_or(0, |cage| cage.fingerprint);

        Ok(BakeSource {
            low_poly: self.low_poly_fingerprint(),
            high_poly,
            cage,
            settings: set.bake_settings_fingerprint(),
            map_settings: set.map_settings_fingerprint(map),
        })
    }

    /// What changed since `map` of texture set `texture_set` was baked; empty when up to date, not baked or without a record
    pub fn bake_changes(&self, texture_set: usize, map: BakedMap) -> Vec<BakeChange> {
        let Ok(set) = self.doc.project.texture_set(texture_set) else { return Vec::new() };
        let Some(record) = set.bake_sources.get(&map).filter(|_| set.baked.contains(&map)) else { return Vec::new() };

        self.bake_source(texture_set, map).map(|current| record.changes(&current)).unwrap_or_default()
    }

    /// The out-of-date maps of texture set `texture_set`, each with what changed
    pub fn out_of_date_maps(&self, texture_set: usize) -> BTreeMap<BakedMap, Vec<BakeChange>> {
        BakedMap::ALL.into_iter().map(|map| (map, self.bake_changes(texture_set, map))).filter(|(_, changes)| !changes.is_empty()).collect()
    }

    /// Fingerprint of the low poly's own geometry (computed once per loaded mesh)
    fn low_poly_fingerprint(&self) -> u64 {
        *self.low_poly_fingerprint.get_or_init(|| {
            let source = &self.source;
            let floats =
                source.positions.iter().chain(&source.normals).flat_map(|vector| vector.to_array()).chain(source.uvs.iter().flat_map(|uv| uv.to_array()));
            let words = floats.map(|float| u64::from(float.to_bits())).chain(source.indices.iter().map(|index| u64::from(*index)));
            fingerprint_words(words)
        })
    }

    /// Keep what the `stored` maps of `texture_set` were baked from and the newest stroke their bake saw; without it they lose their record
    pub(super) fn record_bake_sources(
        &mut self,
        texture_set: usize,
        recorded: Option<(&BTreeMap<BakedMap, BakeSource>, u64)>,
        stored: &[BakedMap],
    ) -> Result<()> {
        let set = self.doc.project.texture_set_mut(texture_set)?;
        match recorded {
            Some((sources, _)) => set.bake_sources.extend(sources),
            None => set.bake_sources.retain(|map, _| !stored.contains(map)),
        }

        let baked = &set.baked;
        set.bake_sources.retain(|map, _| baked.contains(map));
        if let Some((_, painted_stroke)) = recorded {
            self.forget_painted_region(texture_set, stored, painted_stroke);
        }
        Ok(())
    }
}
