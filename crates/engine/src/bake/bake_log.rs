use texturecraft_core::{BakedMap, Project};

use super::{BakeHighState, BakeLogHigh, BakeLogMap, BakeMapState, BakeMesh};

/// The latest bake map by map for the Baking Log, kept from the bake's own state, never from Log lines
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BakeLog {
    pub maps: Vec<BakeLogMap>,
    /// Each high poly file of each texture set that bakes from the high polys, in bake order
    pub highs: Vec<BakeLogHigh>,
}

impl BakeLog {
    /// Start over for a bake of `sets`: every checked map of each set and the `high_polys` of each set that bakes from them, waiting, in bake order
    pub fn begin(&mut self, project: &Project, high_polys: &[BakeMesh], sets: &[usize]) {
        self.maps = sets
            .iter()
            .filter_map(|index| project.texture_sets.get(*index).map(|set| (*index, set)))
            .flat_map(|(index, set)| {
                set.bake.maps.enabled().map(move |map| BakeLogMap { texture_set: index, set_name: set.name.clone(), map, state: BakeMapState::Waiting })
            })
            .collect();
        self.highs = sets
            .iter()
            .filter_map(|index| project.texture_sets.get(*index).map(|set| (*index, set)))
            .filter(|(_, set)| !set.bake.high_poly.use_low_as_high)
            .flat_map(|(index, set)| {
                high_polys.iter().map(move |high| BakeLogHigh {
                    texture_set: index,
                    set_name: set.name.clone(),
                    file: high.name.clone(),
                    state: BakeHighState::Waiting,
                })
            })
            .collect();
    }

    /// `set` starts baking: its high polys load and its first map is the one in progress
    pub fn set_started(&mut self, set: usize) {
        for row in self.highs.iter_mut().filter(|row| row.texture_set == set && row.state == BakeHighState::Waiting) {
            row.state = BakeHighState::Loading;
        }
        self.start_next(set);
    }

    /// `set`'s bake has its high polys ready
    pub fn high_polys_loaded(&mut self, set: usize) {
        for row in self.highs.iter_mut().filter(|row| row.texture_set == set && row.state == BakeHighState::Loading) {
            row.state = BakeHighState::Loaded;
        }
    }

    /// `map` of `set` is baked; the set's next map starts
    pub fn map_baked(&mut self, set: usize, map: BakedMap) {
        if let Some(row) = self.maps.iter_mut().find(|row| row.texture_set == set && row.map == map) {
            row.state = BakeMapState::Baked;
        }
        self.start_next(set);
    }

    /// `set`'s bake was kept: a map it did not hand over is not baked
    pub fn set_finished(&mut self, set: usize) {
        for row in self.maps.iter_mut().filter(|row| row.texture_set == set && row.state.is_pending()) {
            row.state = BakeMapState::Canceled;
        }
    }

    /// Nothing of `set` was kept: every one of its maps failed for `reason`
    pub fn set_failed(&mut self, set: usize, reason: &str) {
        for row in self.maps.iter_mut().filter(|row| row.texture_set == set) {
            row.state = BakeMapState::Failed(reason.to_owned());
        }
        // A file that loaded is not what failed
        for row in self.highs.iter_mut().filter(|row| row.texture_set == set && row.state.is_pending()) {
            row.state = BakeHighState::Failed(reason.to_owned());
        }
    }

    /// The bake stopped: the maps of `current` (the set being baked, not kept) and every map still waiting are canceled
    pub fn cancel(&mut self, current: Option<usize>) {
        for row in &mut self.maps {
            if row.state.is_pending() || (Some(row.texture_set) == current && row.state == BakeMapState::Baked) {
                row.state = BakeMapState::Canceled;
            }
        }
        self.cancel_pending_highs();
    }

    /// The whole bake was rolled back: every map that did not fail is canceled
    pub fn discard(&mut self) {
        for row in self.maps.iter_mut().filter(|row| !matches!(row.state, BakeMapState::Failed(_))) {
            row.state = BakeMapState::Canceled;
        }
        self.cancel_pending_highs();
    }

    /// Whether a map is still waiting or baking
    pub fn is_running(&self) -> bool {
        self.maps.iter().any(|row| row.state.is_pending())
    }

    /// Whether a high poly file is still waiting or loading
    pub fn is_loading_high_polys(&self) -> bool {
        self.highs.iter().any(|row| row.state.is_pending())
    }

    fn cancel_pending_highs(&mut self) {
        for row in self.highs.iter_mut().filter(|row| row.state.is_pending()) {
            row.state = BakeHighState::Canceled;
        }
    }

    fn start_next(&mut self, set: usize) {
        if self.maps.iter().any(|row| row.texture_set == set && row.state == BakeMapState::Baking) {
            return;
        }

        if let Some(row) = self.maps.iter_mut().find(|row| row.texture_set == set && row.state == BakeMapState::Waiting) {
            row.state = BakeMapState::Baking;
        }
    }
}
