use std::sync::Arc;

use texturecraft_engine::Session;
use texturecraft_render::BakeVisualization;

use super::CageUpload;
use super::cage_link::CageLink;
use super::cage_shell_key::CageShellKey;
use super::reach_link::ReachLink;
use crate::gpu::SceneSync;

/// What the GPU has of bake mode's visualization: the high poly, each set's cage and the UV seams
#[derive(Default)]
pub struct BakeSceneLink {
    /// Identity of the uploaded high poly, 0 for none
    high: usize,
    cages: Vec<Option<CageLink>>,
    /// The low-poly mesh whose seams are uploaded
    seams_mesh_id: Option<u64>,
    next_topology: u64,
    reach: ReachLink,
}

impl BakeSceneLink {
    /// Send everything again (the GPU lost what it was sent); the cage shells and the reach worked out on the CPU stay
    pub fn resend(&mut self) {
        self.high = 0;
        self.seams_mesh_id = None;
        for link in self.cages.iter_mut().flatten() {
            link.max_frontal = f32::NAN;
        }
        self.reach.resend();
    }

    /// Whether something is still being worked out off the UI thread
    pub fn busy(&self) -> bool {
        self.reach.busy()
    }

    /// Add to `out` what the GPU needs to draw `visualization` this frame
    pub fn sync(&mut self, session: &Session, visualization: Option<BakeVisualization>, out: &mut SceneSync) {
        let Some(visualization) = visualization else { return };

        if visualization.high_poly {
            let high = session.bake_meshes.merged_high().map(|high| high.mesh.clone());
            let identity = high.as_ref().map_or(0, |mesh| Arc::as_ptr(mesh) as usize);
            if identity != self.high {
                self.high = identity;
                out.bake_high = Some((high, identity as u64));
            }
            self.reach.sync(session, identity, visualization.only_set, out);
        }

        if visualization.uv_seams && self.seams_mesh_id != Some(session.mesh_id) {
            self.seams_mesh_id = Some(session.mesh_id);
            out.uv_seams = Some((session.mesh_id, session.mesh.uv_seam_edges()));
        }

        if visualization.cage {
            self.sync_cages(session, out);
        }
    }

    /// Rebuild the cages whose inputs changed and move the corners of those whose distance changed
    fn sync_cages(&mut self, session: &Session, out: &mut SceneSync) {
        let set_count = session.doc.project.texture_sets.len();
        if self.cages.len() > set_count {
            self.cages.truncate(set_count);
            out.bake_cage_count = Some(set_count);
        }
        self.cages.resize_with(set_count, || None);

        for (ts, link) in self.cages.iter_mut().enumerate() {
            let (Some(key), Some(set)) = (CageShellKey::of(session, ts), session.doc.project.texture_sets.get(ts)) else { continue };

            let max_frontal = set.bake.cage.max_frontal;
            match link {
                Some(link) if link.key == key => {
                    if link.max_frontal == max_frontal {
                        continue;
                    }
                    link.max_frontal = max_frontal;
                }
                _ => {
                    self.next_topology += 1;
                    let shell = session.bake_cage_inputs(ts).ok().flatten().and_then(|inputs| inputs.cage_shell().ok()).map(Arc::new);
                    *link = Some(CageLink { key, shell, topology: self.next_topology, max_frontal });
                }
            }

            let Some(link) = link.as_ref() else { continue };

            let upload = link.shell.as_ref().map(|shell| CageUpload { corners: shell.corners(max_frontal), shell: shell.clone() });
            out.bake_cages.push((ts, upload, link.topology));
        }
    }
}
