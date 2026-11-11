//! The source slot of a fill: a mask fill's grayscale or one channel of a fill. It shows what the fill reads and opens the source picker: the patterns, the project's images and a uniform value, or the anchor points below the layer

use std::sync::Arc;

use egui::{Sense, Ui, vec2};
use serde_json::json;
use texturecraft_engine::compose::{ImageRes, ThumbTarget};
use texturecraft_engine::core::{Channel, LayerId, ProceduralKind, Source};

use crate::TextureCraftApp;
use crate::dock::with;
use crate::fill_source_target::FillSourceTarget;
use crate::filter_slot::{SLOT_HEIGHT, paint};
use crate::i18n::tr;
use crate::resource_picker;
use crate::resource_picker_anchor::ResourcePickerAnchor;
use crate::resource_picker_choice::ResourcePickerChoice;
use crate::resource_picker_tile::ResourcePickerTile;
use crate::shelf_preview::ShelfPreview;

/// The key of the uniform value tile, and the prefixes of the pattern and image tiles' keys
const UNIFORM_KEY: &str = "uniform";
const PATTERN_PREFIX: &str = "pattern:";
const IMAGE_PREFIX: &str = "image:";
/// The gray the uniform value tile shows
const UNIFORM_TILE_GRAY: f32 = 0.8;

/// The slot of `fill`, which reads `source`; `anchors` are the anchor points below its layer and `caption` names what it fills
pub(crate) fn show(app: &mut TextureCraftApp, ui: &mut Ui, fill: &FillSourceTarget, source: &Source, (anchors, caption): (&[(LayerId, String)], &str)) {
    let ctx = ui.ctx().clone();
    let channel = match fill {
        FillSourceTarget::Mask { .. } => source_channel(source),
        FillSourceTarget::Channel { channel, .. } => *channel,
    };
    let (name, picture) = shown(app, &ctx, source);

    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), SLOT_HEIGHT), Sense::click());
    // Read as a dropdown named for what it fills, its value the source
    response.widget_info(|| egui::WidgetInfo { current_text_value: Some(name.clone()), ..egui::WidgetInfo::labeled(egui::Role::ComboBox, true, caption) });
    let popup = ui.make_persistent_id(("fill_source_picker", fill_key(fill)));
    if response.clicked() {
        resource_picker::toggle(&ctx, popup);
    }
    let highlighted = response.hovered() || resource_picker::is_open(&ctx, popup);
    paint(ui, rect, (highlighted, response.has_focus()), picture, (caption, &name));
    if !resource_picker::is_open(&ctx, popup) {
        return;
    }

    let images = matches!(fill, FillSourceTarget::Channel { .. });
    let tiles = tiles(app, &ctx, images);
    let anchors: Vec<ResourcePickerAnchor> = anchors
        .iter()
        .map(|(anchor, name)| ResourcePickerAnchor {
            anchor: *anchor,
            name: name.clone(),
            layer: layer_name(app, *anchor),
            channel: channel.label(),
            picture: app.layer_panel.texture(&ctx, &app.session, ThumbTarget::Stage { effect: *anchor, channel }),
        })
        .collect();
    // The pictures render on the layer stack's service, which runs with the Layers panel: it may be hidden
    app.layer_panel.poll(&ctx, &app.session);
    let current = current_key(source);
    let picked = resource_picker::show_sources(&response, popup, &mut app.thumbs, (current.as_deref(), source.anchor()), tiles, anchors);
    match picked {
        Some(ResourcePickerChoice::Resource(key)) if Some(&key) != current.as_ref() => pick_resource(app, fill, &key),
        Some(ResourcePickerChoice::Anchor(anchor)) if source.anchor() != Some(anchor) => pick_anchor(app, fill, anchor, channel),
        _ => {}
    }
}

/// The name and picture of what `source` reads
fn shown(app: &mut TextureCraftApp, ctx: &egui::Context, source: &Source) -> (String, Option<egui::TextureId>) {
    match source {
        Source::Uniform(_) => (tr("Uniform value").to_string(), None),
        Source::Procedural { pattern, .. } => (tr(pattern.kind.label()).to_string(), app.thumbs.procedural(ctx, pattern.kind).map(|texture| texture.id())),
        Source::Image { name, .. } => {
            let picture = app.session.images.get(name).cloned().map(|image| app.thumbs.image(ctx, name, &image).id());
            (name.clone(), picture)
        }
        Source::Anchor { anchor, channel } => {
            let name = format!("{} · {}", layer_name(app, *anchor), tr(channel.label()));
            (name, app.layer_panel.texture(ctx, &app.session, ThumbTarget::Stage { effect: *anchor, channel: *channel }))
        }
    }
}

/// The name of the layer anchor point `anchor` sits on
fn layer_name(app: &TextureCraftApp, anchor: LayerId) -> String {
    let project = &app.session.doc.project;
    let layer = project.effect(anchor).and_then(|(owner, _)| project.layers.get(&owner));
    layer.map_or_else(|| tr("missing").to_string(), |layer| layer.name.clone())
}

/// The channel a mask fill's anchor is read in (height until one is picked)
fn source_channel(source: &Source) -> Channel {
    match source {
        Source::Anchor { channel, .. } => *channel,
        _ => Channel::Height,
    }
}

/// Keeps the pickers of two slots shown at once apart
fn fill_key(fill: &FillSourceTarget) -> String {
    match fill {
        FillSourceTarget::Mask { effect } => format!("mask_{effect}"),
        FillSourceTarget::Channel { base, channel } => format!("{base}_{}", channel.id()),
    }
}

/// The key of the tile `source` is, outlined in the picker
fn current_key(source: &Source) -> Option<String> {
    match source {
        Source::Uniform(_) => Some(UNIFORM_KEY.to_string()),
        Source::Procedural { pattern, .. } => Some(format!("{PATTERN_PREFIX}{}", pattern.kind.label())),
        Source::Image { name, .. } => Some(format!("{IMAGE_PREFIX}{name}")),
        Source::Anchor { .. } => None,
    }
}

/// The Resources tab: the patterns, the project's images (`images`) and a uniform value
fn tiles(app: &mut TextureCraftApp, ctx: &egui::Context, images: bool) -> Vec<ResourcePickerTile<'static>> {
    let mut tiles: Vec<ResourcePickerTile<'static>> = ProceduralKind::ALL
        .into_iter()
        .map(|kind| ResourcePickerTile {
            key: format!("{PATTERN_PREFIX}{}", kind.label()),
            label: tr(kind.label()).to_string(),
            group: tr("Patterns"),
            thumbnail: app.thumbs.procedural(ctx, kind),
            preview: Box::new(move || ShelfPreview::Procedural(kind)),
        })
        .collect();
    if images {
        let mut names: Vec<(String, Arc<ImageRes>)> = app.session.images.iter().map(|(name, image)| (name.clone(), image.clone())).collect();
        names.sort_by(|a, b| a.0.cmp(&b.0));
        for (name, image) in names {
            let thumbnail = Some(app.thumbs.image(ctx, &name, &image));
            let preview_name = name.clone();
            tiles.push(ResourcePickerTile {
                key: format!("{IMAGE_PREFIX}{name}"),
                label: name,
                group: tr("Images"),
                thumbnail,
                preview: Box::new(move || ShelfPreview::Image(preview_name.clone(), image.clone())),
            });
        }
    }
    let gray = Arc::new(ImageRes { width: 1, height: 1, rgba: vec![[UNIFORM_TILE_GRAY, UNIFORM_TILE_GRAY, UNIFORM_TILE_GRAY, 1.0]], srgb: false });
    tiles.push(ResourcePickerTile {
        key: UNIFORM_KEY.to_string(),
        label: tr("Uniform value").to_string(),
        group: tr("Uniform"),
        thumbnail: Some(app.thumbs.image(ctx, UNIFORM_KEY, &gray)),
        preview: Box::new(move || ShelfPreview::Image(UNIFORM_KEY.to_string(), gray.clone())),
    });
    tiles
}

/// Sets the fill to the resource whose tile key is `key`
fn pick_resource(app: &mut TextureCraftApp, fill: &FillSourceTarget, key: &str) {
    let pattern = key.strip_prefix(PATTERN_PREFIX).and_then(ProceduralKind::parse);
    match (fill, pattern) {
        (FillSourceTarget::Mask { effect }, Some(kind)) => {
            app.run("effect.set", json!({"effect": effect, "procedural": kind.label()}));
        }
        (FillSourceTarget::Mask { effect }, None) => {
            app.run("effect.set", json!({"effect": effect, "uniform": 1.0}));
        }
        (FillSourceTarget::Channel { base, channel }, Some(kind)) => {
            let (low, high) = pattern_range(*channel);
            app.run("layer.fill.set", with(base, json!({"channel": channel.id(), "procedural": kind.label(), "low": low, "high": high})));
        }
        (FillSourceTarget::Channel { base, channel }, None) => match key.strip_prefix(IMAGE_PREFIX) {
            Some(name) => {
                app.run("layer.fill.set", with(base, json!({"channel": channel.id(), "image": name})));
            }
            None => {
                app.run("layer.fill.set", with(base, json!({"channel": channel.id(), "value": channel.default_value()})));
            }
        },
    }
}

/// Sets the fill to read anchor point `anchor` in `channel`
fn pick_anchor(app: &mut TextureCraftApp, fill: &FillSourceTarget, anchor: LayerId, channel: Channel) {
    let (command, params) = match fill {
        FillSourceTarget::Mask { effect } => ("effect.set", json!({"effect": effect, "anchor": anchor.0, "anchorChannel": channel.id()})),
        FillSourceTarget::Channel { base, channel } => ("layer.fill.set", with(base, json!({"channel": channel.id(), "anchor": anchor.0}))),
    };
    app.run(command, params);
}

/// The values a new pattern maps to in `channel`: grays for the base color, a small relief for height, most of the range otherwise
fn pattern_range(channel: Channel) -> ([f32; 3], [f32; 3]) {
    match channel {
        Channel::BaseColor => ([0.15; 3], [0.85; 3]),
        Channel::Height => ([-0.05; 3], [0.05; 3]),
        _ => ([0.2; 3], [0.8; 3]),
    }
}
