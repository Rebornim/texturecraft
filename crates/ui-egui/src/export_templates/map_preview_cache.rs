use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use egui::util::id_type_map::RawKey;
use serde_json::Value;

use super::map_preview_pictures::MapPreviewPictures;
use super::{PlannedOutput, planned_outputs};
use crate::TextureCraftApp;

/// Pixels across an output map's hover preview: enough to read the packing, cheap to sample
const PREVIEW_SIDE_PIXELS: u32 = 150;

/// The current template plan and its hover pictures, invalidated together on edits.
#[derive(Clone)]
pub(super) struct MapPreviewCache {
    key: u64,
    plan: Result<Arc<[Option<PlannedOutput>]>, String>,
    pictures: HashMap<usize, Option<Arc<MapPreviewPictures>>>,
}

impl MapPreviewCache {
    fn id() -> egui::Id {
        egui::Id::unique("export_map_preview_cache")
    }

    fn key(app: &TextureCraftApp, params: &Value) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        params.hash(&mut hasher);
        (app.session.project_id, app.session.revision).hash(&mut hasher);
        hasher.finish()
    }

    /// Output map `index`'s pictures once made; None until then
    pub(super) fn get(ctx: &egui::Context, index: usize) -> Option<Option<Arc<MapPreviewPictures>>> {
        ctx.data(|data| {
            let cache = data.get_temp_raw(RawKey::new::<Self>(Self::id()))?.downcast_ref::<Self>()?;
            cache.pictures.get(&index).cloned()
        })
    }

    /// Release the plan and its pictures when their tab is dismissed.
    pub(super) fn clear(ctx: &egui::Context) {
        ctx.data_mut(|data| data.remove::<Self>(Self::id()));
    }

    /// Keep one descriptor per map until the settings or document change.
    pub(super) fn plan(app: &mut TextureCraftApp, ctx: &egui::Context, params: &Value) -> Result<Arc<[Option<PlannedOutput>]>, String> {
        let key = Self::key(app, params);
        let cached = ctx.data_mut(|data| {
            let cache = data.get_temp_raw(RawKey::new::<Self>(Self::id()))?.downcast_ref::<Self>()?;
            if cache.key == key {
                return Some(cache.plan.clone());
            }

            data.remove::<Self>(Self::id());
            None
        });
        if let Some(plan) = cached {
            return plan;
        }

        let plan = planned_outputs(app, params).map(Arc::from);
        ctx.data_mut(|data| data.insert_temp(Self::id(), Self { key, plan: plan.clone(), pictures: HashMap::new() }));
        plan
    }

    /// Make output map `index`'s pictures for `params` unless they are made, and draw again to show them
    pub(super) fn fill(app: &mut TextureCraftApp, ctx: &egui::Context, params: &Value, index: usize) {
        if Self::get(ctx, index).is_some() {
            return;
        }

        let preview = app.session.export_preview(params, index, PREVIEW_SIDE_PIXELS).ok().flatten();
        let pictures = preview.map(|preview| Arc::new(MapPreviewPictures::new(ctx, &preview, index)));
        ctx.data_mut(|data| {
            if let Some(cache) = data.get_temp_raw_mut(RawKey::new::<Self>(Self::id())).and_then(|entry| entry.downcast_mut::<Self>()) {
                cache.pictures.insert(index, pictures);
            }
        });
        ctx.request_repaint();
    }
}
