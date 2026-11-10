//! One asset on the shelf: its name, the tab it belongs to, and how it previews, drags and applies

use serde_json::json;
use texturecraft_engine::core::ProceduralKind;
use texturecraft_engine::filter_lib::{FILTERS, FilterDef};
use texturecraft_engine::materials::{MATERIALS, SmartMaterial};
use texturecraft_engine::paint::alphas::{self, LibraryAlpha};
use texturecraft_engine::paint::presets::{BrushPreset, PRESETS};
use texturecraft_engine::smart_masks::{SMART_MASKS, SmartMask};

use crate::TextureCraftApp;
use crate::i18n::tr;
use crate::shelf::{Asset, Tab, apply};
use crate::shelf_preview::ShelfPreview;

/// A shelf tile's asset
#[derive(Clone)]
pub(crate) enum ShelfEntry {
    Material(&'static SmartMaterial),
    SmartMask(&'static SmartMask),
    Filter(&'static FilterDef),
    Brush(&'static BrushPreset),
    Alpha(&'static LibraryAlpha),
    Procedural(ProceduralKind),
    /// An image resource of the project, by name
    Image(String),
}

impl ShelfEntry {
    /// Every built-in asset and the project's images
    pub(crate) fn all(app: &TextureCraftApp) -> Vec<ShelfEntry> {
        let built_in = MATERIALS
            .iter()
            .map(ShelfEntry::Material)
            .chain(SMART_MASKS.iter().map(ShelfEntry::SmartMask))
            .chain(FILTERS.iter().map(ShelfEntry::Filter))
            .chain(PRESETS.iter().map(ShelfEntry::Brush))
            .chain(alphas::library().iter().map(ShelfEntry::Alpha))
            .chain(ProceduralKind::ALL.into_iter().map(ShelfEntry::Procedural));
        built_in.chain(app.session.doc.resources.keys().cloned().map(ShelfEntry::Image)).collect()
    }

    /// The tab that lists the asset
    pub(crate) fn tab(&self) -> Tab {
        match self {
            ShelfEntry::Material(material) if material.smart => Tab::Smart,
            ShelfEntry::Material(_) => Tab::Materials,
            ShelfEntry::SmartMask(_) => Tab::Masks,
            ShelfEntry::Filter(_) => Tab::Filters,
            ShelfEntry::Brush(_) => Tab::Brushes,
            ShelfEntry::Alpha(_) => Tab::Alphas,
            ShelfEntry::Procedural(_) => Tab::Procedurals,
            ShelfEntry::Image(_) => Tab::Textures,
        }
    }

    /// The name under the tile, translated where the asset's name is
    pub(crate) fn label(&self) -> &str {
        match self {
            ShelfEntry::Material(material) => material.label,
            ShelfEntry::SmartMask(mask) => mask.label,
            ShelfEntry::Filter(filter) => tr(filter.label),
            ShelfEntry::Brush(preset) => preset.label,
            ShelfEntry::Alpha(alpha) => &alpha.label,
            ShelfEntry::Procedural(kind) => tr(kind.label()),
            ShelfEntry::Image(name) => name,
        }
    }

    /// Whether `filter` (lowercase) matches the asset's name, id or category, in English or translated
    pub(crate) fn matches(&self, filter: &str) -> bool {
        let texts: &[&str] = match self {
            ShelfEntry::Material(material) => &[material.label, material.category, material.id],
            ShelfEntry::SmartMask(mask) => &[mask.label, mask.id],
            ShelfEntry::Filter(filter) => &[filter.label, filter.group, filter.id, tr(filter.label), tr(filter.group)],
            ShelfEntry::Brush(preset) => &[preset.label, preset.category, preset.id],
            ShelfEntry::Alpha(alpha) => &[&alpha.label, alpha.category, &alpha.id],
            ShelfEntry::Procedural(kind) => &[kind.label(), tr(kind.label())],
            ShelfEntry::Image(name) => &[name],
        };
        crate::shelf::matches(filter, texts)
    }

    /// The thumbnail, or None while it renders
    pub(crate) fn thumbnail(&self, app: &mut TextureCraftApp, ctx: &egui::Context) -> Option<egui::TextureHandle> {
        match self {
            ShelfEntry::Material(material) => app.thumbs.material(ctx, material.id),
            ShelfEntry::SmartMask(mask) => app.thumbs.smart_mask(ctx, mask.id),
            ShelfEntry::Filter(filter) => app.thumbs.filter(ctx, filter.id),
            ShelfEntry::Brush(preset) => app.thumbs.brush_preset(ctx, preset),
            ShelfEntry::Alpha(alpha) => app.thumbs.alpha(ctx, alpha),
            ShelfEntry::Procedural(kind) => app.thumbs.procedural(ctx, *kind),
            ShelfEntry::Image(name) => {
                let image = app.session.images.get(name).cloned()?;
                Some(app.thumbs.image(ctx, name, &image))
            }
        }
    }

    /// What the hover popup renders; None for an image that is gone
    pub(crate) fn preview(&self, app: &TextureCraftApp) -> Option<ShelfPreview> {
        Some(match self {
            ShelfEntry::Material(material) => ShelfPreview::material(material.id),
            ShelfEntry::SmartMask(mask) => ShelfPreview::SmartMask(mask.id),
            ShelfEntry::Filter(filter) => ShelfPreview::Filter(filter.id),
            ShelfEntry::Brush(preset) => ShelfPreview::Brush(preset),
            ShelfEntry::Alpha(alpha) => ShelfPreview::Alpha(alpha),
            ShelfEntry::Procedural(kind) => ShelfPreview::Procedural(*kind),
            ShelfEntry::Image(name) => ShelfPreview::Image(name.clone(), app.session.images.get(name).cloned()?),
        })
    }

    /// The drag payload's asset; brushes and procedurals don't drag
    pub(crate) fn asset(&self) -> Option<Asset> {
        match self {
            ShelfEntry::Material(material) => Some(Asset::Material(material.id)),
            ShelfEntry::SmartMask(mask) => Some(Asset::SmartMask(mask.id)),
            ShelfEntry::Filter(filter) => Some(Asset::Filter(filter.id)),
            ShelfEntry::Alpha(alpha) => Some(Asset::Alpha(&alpha.id)),
            ShelfEntry::Image(name) => Some(Asset::Texture(name.clone())),
            ShelfEntry::Brush(_) | ShelfEntry::Procedural(_) => None,
        }
    }

    /// Whether the tile is outlined as in use: the brush's library alpha
    pub(crate) fn in_use(&self, app: &TextureCraftApp) -> bool {
        let ShelfEntry::Alpha(alpha) = self else { return false };

        let brush = &app.session.tool.brush;
        brush.alpha_image.is_none() && brush.alpha_library.as_deref() == Some(alpha.id.as_str())
    }

    /// What a click on the tile does
    pub(crate) fn activate(&self, app: &mut TextureCraftApp) {
        match self {
            ShelfEntry::Brush(preset) => {
                app.run("brush.preset.apply", json!({"preset": preset.id}));
            }
            ShelfEntry::Procedural(kind) => {
                app.run("layer.add.fill", json!({"name": kind.label()}));
                app.run("layer.fill.set", json!({"channel": "baseColor", "procedural": kind.label(), "low": [0.1, 0.1, 0.1], "high": [0.9, 0.9, 0.9]}));
            }
            entry => {
                if let Some(asset) = entry.asset() {
                    apply(app, &asset);
                }
            }
        }
    }
}
