use serde::Serialize;
use texturecraft_core::{BakedMap, Project};

use super::{BakeLogMap, BakeMapState};

/// The latest bake map by map for the Baking Log, kept from the bake's own state, never from Log lines
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct BakeLog {
    pub maps: Vec<BakeLogMap>,
}

impl BakeLog {
    /// Start over for a bake of `sets`: every checked map of each set, waiting, in bake order
    pub fn begin(&mut self, project: &Project, sets: &[usize]) {
        self.maps = sets
            .iter()
            .filter_map(|index| project.texture_sets.get(*index).map(|set| (*index, set)))
            .flat_map(|(index, set)| {
                set.bake.maps.enabled().map(move |map| BakeLogMap { texture_set: index, set_name: set.name.clone(), map, state: BakeMapState::Waiting })
            })
            .collect();
    }

    /// `set` starts baking: its first map is the one in progress
    pub fn set_started(&mut self, set: usize) {
        self.start_next(set);
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
    }

    /// The bake stopped: the maps of `current` (the set being baked, not kept) and every map still waiting are canceled
    pub fn cancel(&mut self, current: Option<usize>) {
        for row in &mut self.maps {
            if row.state.is_pending() || (Some(row.texture_set) == current && row.state == BakeMapState::Baked) {
                row.state = BakeMapState::Canceled;
            }
        }
    }

    /// The whole bake was rolled back: every map that did not fail is canceled
    pub fn discard(&mut self) {
        for row in self.maps.iter_mut().filter(|row| !matches!(row.state, BakeMapState::Failed(_))) {
            row.state = BakeMapState::Canceled;
        }
    }

    /// Whether a map is still waiting or baking
    pub fn is_running(&self) -> bool {
        self.maps.iter().any(|row| row.state.is_pending())
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
