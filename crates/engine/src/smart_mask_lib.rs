//! User smart masks: a layer's mask saved with its painted pixels and images as a `.tcmask` file
//! (the `.tcraft` container holding a one-layer project that carries only the mask), and applied
//! back to any layer, replacing its mask.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde_json::Value;
use texturecraft_compose::PixelStore;
use texturecraft_core::{EffectKind, LayerId, LayerKind, Mask, Project, Source, TextureSet};

use crate::commands::{CommandSpec, has_layer};
use crate::session::{Document, StackTarget};
use crate::{EngineError, Result, Session, cmd};

/// Smart mask file extension.
pub const SMART_MASK_EXT: &str = "tcmask";

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!("mask.smart.save", "Create smart mask", [], None, "{layer?, path: .tcmask file, name?}", has_layer, save),
        cmd!("mask.smart.load", "Apply smart mask from file", [], None, "{path: .tcmask smart mask, layer?} — replaces the layer's mask", has_layer, load),
    ]
}

fn fe(e: texturecraft_formats::FormatError) -> EngineError {
    EngineError::Other(e.to_string())
}

/// Copy `mask` (painted pixels from `src`) with fresh effect ids from `dst`, its images resampled
/// to `size`.
fn copy_mask(mask: &Mask, src: &PixelStore, dst: &mut Project, dst_pixels: &mut PixelStore, size: (u32, u32)) -> Mask {
    let mut copy = mask.clone();
    for e in copy.effects.iter_mut() {
        let old = e.id;
        e.id = dst.alloc_id();
        if let Some(img) = src.masks.get(&old) {
            dst_pixels.masks.insert(e.id, crate::layer_ops::fit(img, size, &[0, 0]));
        }
    }
    copy
}

fn mask_images(mask: &Mask) -> Vec<String> {
    let mut used = Vec::new();
    for e in &mask.effects {
        if let EffectKind::Fill { source: Source::Image { name, .. }, .. } = &e.kind {
            used.push(name.clone());
        }
    }
    used
}

impl Session {
    /// Serialize layer `id`'s mask (with its painted pixels and images) as a smart mask file.
    pub fn smart_mask_bytes(&self, id: LayerId, name: &str) -> Result<Vec<u8>> {
        let (ts, _, _) = self.doc.project.locate(id).ok_or_else(|| EngineError::Other("layer not in a stack".into()))?;
        let layer = self.doc.project.layer(id)?;
        let mask = layer.mask.as_ref().ok_or_else(|| EngineError::Other("the layer has no mask".into()))?;
        // An anchor lives in the stack around the mask, which the file does not carry.
        if mask.effects.iter().any(|e| matches!(&e.kind, EffectKind::Fill { source, .. } if source.anchor().is_some())) {
            return Err(EngineError::Other("the mask reads an anchor point, which a smart mask cannot carry".into()));
        }
        let src_set = self.doc.project.texture_set(ts)?;
        let mut project = Project::new(name, "builtin:none");
        let mut set = TextureSet::new(name, 0, src_set.resolution);
        set.uv_tiles = src_set.uv_tiles.clone();
        set.tile_resolutions = src_set.tile_resolutions.clone();
        let size = set.size();
        project.texture_sets.push(set);
        let mut pixels = PixelStore::default();
        let copy = copy_mask(mask, &self.doc.pixels, &mut project, &mut pixels, size);
        let resources = mask_images(&copy).into_iter().filter_map(|n| self.doc.resources.get(&n).map(|b| (n, b.as_ref().clone()))).collect();
        let holder = project.alloc_id();
        let mut l = texturecraft_core::Layer::new(holder, name, LayerKind::Paint, &[]);
        l.mask = Some(copy);
        project.insert_layer(0, l, None, false)?;
        let pf = texturecraft_formats::ProjectFile {
            project,
            pixels,
            baked: BTreeMap::new(),
            mesh: None,
            high_poly: Vec::new(),
            cage: None,
            bake_paint: BTreeMap::new(),
            thumbnail: None,
            resources,
            warnings: Vec::new(),
        };
        texturecraft_formats::save_project_bytes(&pf).map_err(fe)
    }

    /// Replace layer `layer`'s mask with the smart mask file `bytes` (one undo step).
    pub fn apply_smart_mask_bytes(&mut self, layer: LayerId, bytes: &[u8]) -> Result<()> {
        let pf = texturecraft_formats::load_project_bytes(bytes).map_err(fe)?;
        let (name, mask) = pf
            .project
            .texture_sets
            .first()
            .and_then(|t| t.stack.last())
            .and_then(|id| pf.project.layers.get(id))
            .and_then(|l| l.mask.clone().map(|m| (l.name.clone(), m)))
            .ok_or_else(|| EngineError::Other("the file holds no smart mask".into()))?;
        let (ts, _, _) = self.doc.project.locate(layer).ok_or_else(|| EngineError::Other("layer not in a stack".into()))?;
        let resources: BTreeMap<String, Arc<Vec<u8>>> = pf.resources.into_iter().map(|(k, v)| (k, Arc::new(v))).collect();
        self.edit(&format!("Apply smart mask {name}"), |s| {
            for (k, v) in &resources {
                s.doc.resources.entry(k.clone()).or_insert_with(|| v.clone());
            }
            let size = s.doc.project.texture_set(ts)?.size();
            let m = copy_mask(&mask, &pf.pixels, &mut s.doc.project, &mut s.doc.pixels, size);
            let old = s.doc.project.layer_mut(layer)?.mask.replace(m);
            for e in old.map(|m| m.effects).unwrap_or_default() {
                s.doc.pixels.masks.remove(&e.id);
            }
            s.target = StackTarget::Mask;
            Ok(())
        })?;
        self.sync_images();
        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
use crate::commands::{bad, layer_param, strp};
#[cfg(not(target_arch = "wasm32"))]
use serde_json::json;

#[cfg(not(target_arch = "wasm32"))]
fn save(s: &mut Session, p: &Value) -> Result<Value> {
    let id = layer_param(s, "mask.smart.save", p)?;
    let path = strp(p, "path").ok_or_else(|| bad("mask.smart.save", "`path` is required"))?;
    let name = strp(p, "name").map(str::to_string).unwrap_or_else(|| s.doc.project.layer(id).map(|l| l.name.clone()).unwrap_or_default());
    let bytes = s.smart_mask_bytes(id, &name)?;
    let path = if path.ends_with(&format!(".{SMART_MASK_EXT}")) { path.to_string() } else { format!("{path}.{SMART_MASK_EXT}") };
    if let Some(dir) = std::path::Path::new(&path).parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| bad("mask.smart.save", format!("cannot create {}: {e}", dir.display())))?;
    }
    let tmp = format!("{path}.tmp");
    std::fs::write(&tmp, bytes).and_then(|_| std::fs::rename(&tmp, &path)).map_err(|e| bad("mask.smart.save", format!("cannot write {path}: {e}")))?;
    Ok(json!({ "path": path }))
}

#[cfg(not(target_arch = "wasm32"))]
fn load(s: &mut Session, p: &Value) -> Result<Value> {
    let path = strp(p, "path").ok_or_else(|| bad("mask.smart.load", "`path` is required"))?;
    let layer = layer_param(s, "mask.smart.load", p)?;
    let bytes = std::fs::read(path).map_err(|e| bad("mask.smart.load", format!("cannot read {path}: {e}")))?;
    s.apply_smart_mask_bytes(layer, &bytes)?;
    Ok(json!({ "layer": layer.0 }))
}

#[cfg(target_arch = "wasm32")]
fn save(_: &mut Session, _: &Value) -> Result<Value> {
    Err(EngineError::Other("file paths are not available in the browser".into()))
}

#[cfg(target_arch = "wasm32")]
fn load(_: &mut Session, _: &Value) -> Result<Value> {
    Err(EngineError::Other("file paths are not available in the browser".into()))
}
