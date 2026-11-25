//! Picking a layer on the mesh (R + click in the viewport, `layer.pickAt`): select the layer laying something down where the mesh is clicked

use texturecraft_core::{LayerId, LayerKind};
use texturecraft_geom::glam::Vec2;

use crate::session::ViewMode;
use crate::{EngineError, Result, Session, StackTarget};

/// How far a click may land from the last pick, in viewport pixels, and still pick on the same spot
const SAME_SPOT_DISTANCE: f32 = 4.0;

impl Session {
    /// Select the top layer laying something down under `screen` in the shown channels (every channel outside a single-channel view); a click on the same spot again selects the next layer down, wrapping to the top
    pub fn pick_layer(&mut self, screen: Vec2, viewport: Vec2) -> Result<LayerId> {
        let hit = self.pick(screen, viewport).ok_or_else(|| EngineError::Other("nothing under the cursor".into()))?;
        let ts = self.texture_set_of_material(hit.material).unwrap_or(self.ts());
        let (width, height) =
            self.caches.get(ts).map(|cache| (cache.size.0 as f32, cache.size.1 as f32)).ok_or_else(|| EngineError::Other("texture set not ready".into()))?;
        let texel = ((hit.uv.x * width).clamp(0.0, width - 1.0) as u32, (hit.uv.y * height).clamp(0.0, height - 1.0) as u32);
        let shown = match self.view {
            ViewMode::Channel(channel) => vec![channel],
            _ => self.doc.project.texture_set(ts).map(|set| set.channel_list()).unwrap_or_default(),
        };
        let layers = self.with_inputs(ts, |inputs| Some(texturecraft_compose::layers_at(inputs, texel, &shown))).unwrap_or_default();

        let same_spot = self.last_layer_pick.is_some_and(|last| last.distance(screen) <= SAME_SPOT_DISTANCE);
        let selected = (ts == self.ts()).then(|| self.selected_layer()).flatten();
        let below_selected = selected.and_then(|selected| layers.iter().position(|id| *id == selected)).and_then(|at| layers.get(at + 1));
        let picked = match below_selected {
            Some(next) if same_spot => Some(next),
            _ => layers.first(),
        };
        let Some(&id) = picked else {
            self.last_layer_pick = None;
            return Err(EngineError::Other("no visible layer lays anything down here".into()));
        };

        // Paths paint into the active set, and the clone source is a point of the set it was picked in
        if ts != self.ts() {
            self.finish_path()?;
            self.tool.clone_source = None;
        }
        self.active_set = ts;
        self.select_one(ts, id);
        self.open_folders_holding(id);
        self.target = StackTarget::Content;
        self.revision = self.revision.wrapping_add(1);
        self.last_layer_pick = Some(screen);
        Ok(id)
    }

    /// Expand every collapsed folder holding `id`, so the Layers panel shows its row
    fn open_folders_holding(&mut self, id: LayerId) {
        let mut child = id;
        while let Some((_, Some(folder), _)) = self.doc.project.locate(child) {
            if let Some(LayerKind::Group { collapsed, .. }) = self.doc.project.layers.get_mut(&folder).map(|layer| &mut layer.kind) {
                *collapsed = false;
            }
            child = folder;
        }
    }
}
