//! Picking a layer on the mesh (R + click in the viewport, `layer.layersAt` and `layer.pickAt`): the layers laying something down where the mesh is clicked, and selecting one of them

use texturecraft_core::{Channel, LayerId, LayerKind};
use texturecraft_geom::glam::Vec2;

use crate::session::ViewMode;
use crate::{EngineError, Result, Session, StackTarget};

impl Session {
    /// The texture set and texel of the mesh under `screen`, None off the mesh
    pub fn texel_under(&self, screen: Vec2, viewport: Vec2) -> Option<(usize, (u32, u32))> {
        let hit = self.pick(screen, viewport)?;
        let ts = self.texture_set_of_material(hit.material).unwrap_or(self.ts());
        let (width, height) = self.caches.get(ts).map(|cache| (cache.size.0 as f32, cache.size.1 as f32))?;
        let texel = ((hit.uv.x * width).clamp(0.0, width - 1.0) as u32, (hit.uv.y * height).clamp(0.0, height - 1.0) as u32);
        Some((ts, texel))
    }

    /// The layers laying something down at `texel` of texture set `ts` in the shown channels (every channel outside a single-channel view), top first, each with the channels it changes there
    pub fn layers_at_texel(&self, ts: usize, texel: (u32, u32)) -> Vec<(LayerId, Vec<Channel>)> {
        let shown = match self.view {
            ViewMode::Channel(channel) => vec![channel],
            _ => self.doc.project.texture_set(ts).map(|set| set.channel_list()).unwrap_or_default(),
        };
        self.with_inputs(ts, |inputs| Some(texturecraft_compose::layers_at(inputs, texel, &shown))).unwrap_or_default()
    }

    /// The texture set under `screen` and the layers laying something down there (see [`Session::layers_at_texel`])
    pub fn layers_under(&self, screen: Vec2, viewport: Vec2) -> Result<(usize, Vec<(LayerId, Vec<Channel>)>)> {
        let (ts, texel) = self.texel_under(screen, viewport).ok_or_else(|| EngineError::Other("nothing under the cursor".into()))?;
        Ok((ts, self.layers_at_texel(ts, texel)))
    }

    /// Select `wanted` when it lays something down under `screen`, else the top layer there
    pub fn pick_layer(&mut self, screen: Vec2, viewport: Vec2, wanted: Option<LayerId>) -> Result<LayerId> {
        let (ts, layers) = self.layers_under(screen, viewport)?;
        let picked = match wanted {
            Some(wanted) => layers.iter().find(|(id, _)| *id == wanted).ok_or_else(|| EngineError::Other("that layer lays nothing down here".into()))?,
            None => layers.first().ok_or_else(|| EngineError::Other("no visible layer lays anything down here".into()))?,
        };
        let id = picked.0;

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
