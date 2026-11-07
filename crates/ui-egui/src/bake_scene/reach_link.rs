use std::sync::mpsc::{Receiver, TryRecvError};

use rayon::prelude::*;
use texturecraft_engine::Session;
use texturecraft_engine::bake::HighPolyReach;

use super::cage_shell_key::CageShellKey;
use crate::gpu::SceneSync;

/// The red on the high poly: each texture set's ray reach, worked out on a worker thread, and the distances the GPU's colors show
#[derive(Default)]
pub(super) struct ReachLink {
    /// The inputs and high poly the reach was started for
    keys: Vec<Option<CageShellKey>>,
    high: usize,
    job: Option<Receiver<Vec<Option<HighPolyReach>>>>,
    reach: Vec<Option<HighPolyReach>>,
    /// Each set's (max frontal, max rear) and the selected-set filter the GPU's colors are for
    shown: Option<(Vec<(f32, f32)>, Option<usize>)>,
}

impl ReachLink {
    /// Whether a reach is being worked out (the view must keep drawing to pick it up)
    pub(super) fn busy(&self) -> bool {
        self.job.is_some()
    }

    /// Start a new reach when the high poly `high` or a set's inputs changed, and color the high poly when the reach or the distances changed
    pub(super) fn sync(&mut self, session: &Session, high: usize, only_set: Option<usize>, out: &mut SceneSync) {
        let set_count = session.doc.project.texture_sets.len();
        let keys: Vec<Option<CageShellKey>> = (0..set_count).map(|ts| CageShellKey::of(session, ts)).collect();
        if keys != self.keys || high != self.high {
            let inputs: Vec<_> = (0..set_count).map(|ts| session.bake_cage_inputs(ts).ok().flatten()).collect();
            self.job = Some(crate::spawn_worker("texturecraft-bake-reach", move || {
                inputs.iter().map(|inputs| inputs.as_ref().and_then(|inputs| inputs.high_poly_reach().ok())).collect()
            }));
            (self.keys, self.high) = (keys, high);
        }

        if let Some(job) = &self.job {
            match job.try_recv() {
                Ok(reach) => {
                    self.reach = reach;
                    self.shown = None;
                    self.job = None;
                }
                Err(TryRecvError::Disconnected) => self.job = None,
                Err(TryRecvError::Empty) => {}
            }
        }

        let distances: Vec<(f32, f32)> = session.doc.project.texture_sets.iter().map(|set| (set.bake.cage.max_frontal, set.bake.cage.max_rear)).collect();
        let shown = (distances, only_set);
        if self.shown.as_ref() == Some(&shown) {
            return;
        }

        let Some(vertex_count) = self.reach.iter().flatten().map(|reach| reach.vertices.len()).max() else { return };

        let sets: Vec<(&HighPolyReach, f32, f32)> = self
            .reach
            .iter()
            .zip(&shown.0)
            .enumerate()
            .filter(|(ts, _)| only_set.is_none_or(|only| only == *ts))
            .filter_map(|(_, (reach, &(max_frontal, max_rear)))| Some((reach.as_ref()?, max_frontal, max_rear)))
            .collect();
        // Each vertex belongs to the set whose low poly is nearest, as the bake's rays of that set are the ones that reach it
        let missed: Vec<f32> = (0..vertex_count)
            .into_par_iter()
            .map(|vertex| {
                let nearest = sets
                    .iter()
                    .filter_map(|(reach, max_frontal, max_rear)| reach.vertex_missed(vertex, *max_frontal, *max_rear))
                    .min_by(|a, b| a.0.total_cmp(&b.0));
                nearest.map_or(0.0, |(_, missed)| f32::from(u8::from(missed)))
            })
            .collect();
        out.bake_high_missed = Some((self.high as u64, missed));
        self.shown = Some(shown);
    }
}
