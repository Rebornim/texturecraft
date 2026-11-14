//! A fill's Material section: the channel strip, the orb with the Material | Channels switch, then one slot card per channel that is on (Channels) or the fill's whole material (Material)

use std::collections::HashMap;
use std::sync::Arc;

use egui::{Color32, Painter, Rect};
use egui_phosphor::regular as ic;
use serde_json::{Value, json};
use texturecraft_engine::compose::{ImageRes, ImageStore};
use texturecraft_engine::core::{
    Channel, ChannelKind, CookResolution, Fill, LayerId, LinkParam, MaterialLink, NormalFormat, Procedural, ProceduralKind, Source,
};
use texturecraft_engine::fill_alpha::library_alpha_of;
use texturecraft_engine::fill_material_cmds::whole_materials;
use texturecraft_engine::materials::find;
use texturecraft_engine::paint::alphas;
use texturecraft_engine::texel::linear_to_srgb;

use crate::TextureCraftApp;
use crate::color_picker::{ColorPick, SwatchEdit};
use crate::dock::{anchors_below, with};
use crate::i18n::tr;
use crate::material_orb::material_orb;
use crate::orb_material::OrbMaterial;
use crate::picker_group::PickerGroup;
use crate::picker_tab::PickerTab;
use crate::property_sheet::PropertySheet;
use crate::resolution_band::{resolution_band, resolution_row};
use crate::resource_picker;
use crate::resource_picker_item::ResourcePickerItem;
use crate::slot_thumbnail::{paint_thumbnail, thumbnail};
use crate::source_slot::SourceSlot;
use crate::state::UiState;
use crate::theme::Tokens;
use crate::thumbs::Thumbs;
use crate::tile_picture::TilePicture;

/// The switch's two sides: the fill's whole material, or one source per channel
const MODES: [&str; 2] = ["Material", "Channels"];
const MATERIAL_MODE: usize = 0;
const CHANNELS_MODE: usize = 1;

/// Pattern scale and contrast a procedural's sliders run over (typed values stay inside them too)
const PATTERN_SCALE: std::ops::RangeInclusive<f32> = 0.1..=32.0;
const PATTERN_CONTRAST: std::ops::RangeInclusive<f32> = 0.0..=2.0;

/// A source the slot's picker offers
#[derive(Clone, Debug, PartialEq)]
enum SourceChoice {
    Uniform,
    Procedural(ProceduralKind),
    Image(String),
    Alpha(&'static str),
    Import,
    Anchor(LayerId),
}

/// The Material section of the fill at `base`; `on` holds the channels the fill writes
pub(crate) fn material_section(app: &mut TextureCraftApp, sheet: &mut PropertySheet<'_>, fill: &Fill, base: &Value, on: &[Channel], reader: LayerId) {
    sheet.section(tr("Material"), false);
    let channels = app.session.doc.project.texture_set(app.session.ts()).map(|texture_set| texture_set.channel_list()).unwrap_or_default();
    let names: Vec<&str> = channels.iter().map(|channel| tr(pill_name(*channel))).collect();
    let enabled: Vec<bool> = channels.iter().map(|channel| on.contains(channel)).collect();
    if let Some((index, solo)) = sheet.channel_pills(&names, &enabled)
        && let Some(channel) = channels.get(index)
    {
        let enable = solo || !enabled.get(index).copied().unwrap_or(false);
        app.run("layer.fill.set", with(base, json!({"channel": channel.id(), "enabled": enable, "solo": solo})));
    }

    let tokens = Tokens::get(sheet.ui.ctx());
    let look = OrbMaterial::of_fill(fill, on, &app.session.images);
    let shown = orb_look(&app.ui, fill, on, &app.session.images);
    let place = base.to_string();
    let orb = |painter: &Painter, rect: Rect| material_orb(painter, rect, &shown, &tokens, &format!("orb{place}"));
    let link = fill.link.as_deref().zip(base.get("layer").and_then(Value::as_u64)).map(|(link, layer)| (link.clone(), LayerId(layer)));
    let mode_id = sheet.ui.make_persistent_id(("material_mode", base.to_string()));
    // A fill opens on the side that made it: a material (linked or built-in) on Material, separate sources on Channels
    let made_whole = link.is_some() || fill.material.is_some();
    let mode = sheet.ui.data(|data| data.get_temp::<usize>(mode_id)).unwrap_or(if made_whole { MATERIAL_MODE } else { CHANNELS_MODE });
    let modes = MODES.map(tr);
    if let Some(picked) = sheet.orb_switch(&orb, &modes, mode) {
        sheet.ui.data_mut(|data| data.insert_temp(mode_id, picked));
    }

    match link {
        Some((link, layer)) if mode == MATERIAL_MODE => {
            let shown: Vec<Channel> = channels.into_iter().filter(|channel| on.contains(channel)).collect();
            material_card(app, sheet, &link, layer, &look, &place);
            mapping_band(app, sheet, fill, &shown, layer);
            let texture_set = app.session.doc.project.texture_set(app.session.ts()).map_or(link.size, |texture_set| texture_set.resolution);
            if let Some(picked) = resolution_band(sheet, link.resolution.unwrap_or(CookResolution::TEXTURE_SET), texture_set) {
                let size = match picked {
                    CookResolution::Relative(factor) => json!({"relative": factor}),
                    CookResolution::Fixed(size) => json!({"fixed": size}),
                };
                if app.run("material.link.resolution", with(&json!({"layer": layer.0, "cook": false}), size)).is_some() {
                    app.cook_link(layer);
                }
            }
            #[cfg(not(target_arch = "wasm32"))]
            if sheet.button(egui_phosphor::regular::ARROW_SQUARE_OUT, tr("Open in MaterialCraft")).clicked() {
                crate::dock::open_link_in_materialcraft(app, layer, &link);
            }
        }
        None if mode == MATERIAL_MODE => whole_material_card(app, sheet, fill, base, &look, &place),
        _ => {
            let anchors = anchors_below(app, reader);
            for channel in channels.into_iter().filter(|channel| on.contains(channel)) {
                source_card(app, sheet, fill.source(channel), base, SourceSlot::Channel(channel), &anchors, false);
            }
        }
    }
}

/// What the orb shows, following the Layers panel's channel list: the whole material, or the channel picked there alone
fn orb_look(ui: &UiState, fill: &Fill, on: &[Channel], images: &HashMap<String, Arc<ImageRes>>) -> OrbMaterial {
    if ui.layer_material_view { OrbMaterial::of_fill(fill, on, images) } else { OrbMaterial::of_channel(fill, ui.layer_channel, on, images) }
}

/// A channel's name on its pill: the reference strip's short Title Case words, the full name where it has none
pub(crate) fn pill_name(channel: Channel) -> &'static str {
    match channel {
        Channel::BaseColor => "Color",
        Channel::Metallic => "Metal",
        Channel::Roughness => "Rough",
        Channel::Normal => "Normal",
        Channel::Height => "Height",
        Channel::Opacity => "Opacity",
        other => other.label(),
    }
}

/// A linear colour as the sRGB colour the screen shows
pub(crate) fn shown_color(linear: [f32; 3]) -> Color32 {
    let [r, g, b] = linear.map(|c| (linear_to_srgb(c.clamp(0.0, 1.0)) * 255.0).round() as u8);
    Color32::from_rgb(r, g, b)
}

/// What the slot's source line says
pub(crate) fn source_line(app: &TextureCraftApp, source: &Source) -> String {
    match source {
        Source::Uniform(_) => tr("Uniform").to_owned(),
        Source::Procedural { pattern, .. } => format!("{} · {}", tr(pattern.kind.label()), tr("procedural")),
        Source::Image { name, .. } => match library_alpha_of(name) {
            Some(alpha) => format!("{} · {}", tr(&alpha.label), tr("alpha")),
            None => format!("{name} · {}", tr("image")),
        },
        Source::Anchor { anchor, .. } => {
            let name = app.session.doc.project.effect(*anchor).map_or_else(|| tr("missing").to_owned(), |(_, effect)| effect.name.clone());
            format!("{} · {name}", tr("Anchor"))
        }
    }
}

/// One source's card (a channel's, a mask fill's grayscale or a filter's image input): its slot (the row opens the source picker), then the source's own settings; `in_card`: only its slot row, inside the card open now
pub(crate) fn source_card(
    app: &mut TextureCraftApp,
    sheet: &mut PropertySheet<'_>,
    source: Source,
    base: &Value,
    slot_of: SourceSlot,
    anchors: &[(LayerId, String)],
    in_card: bool,
) {
    let set = |extra: Value| with(base, with(&slot_of.fields(), extra));
    let tokens = Tokens::get(sheet.ui.ctx());
    let picture = thumbnail(app, &sheet.ui.ctx().clone(), &source);
    let line = source_line(app, &source);
    let uniform = matches!(source, Source::Uniform(_));
    let procedural = matches!(source, Source::Procedural { .. });
    let thumb = |painter: &egui::Painter, rect: egui::Rect| paint_thumbnail(painter, rect, &picture, &tokens);
    let slot = if in_card {
        sheet.slot_in_card(&thumb, slot_of.name(), &line, !uniform)
    } else {
        sheet.slot(&thumb, slot_of.name(), &line, None, !uniform, procedural)
    };

    let popup = sheet.ui.make_persistent_id(("source_picker", base.to_string(), slot_of.key()));
    if slot.pick.clicked() {
        resource_picker::toggle(sheet.ui.ctx(), popup);
    }
    let current = source_choice(&source);
    // From Uniform the picker opens at the top, not at its Uniform tile at the end
    let reveal = current != SourceChoice::Uniform;
    let uniform = match source {
        Source::Uniform(value) => value,
        _ => slot_of.default_value(),
    };
    let ctx = sheet.ui.ctx().clone();
    let tabs = |thumbs: &mut Thumbs| source_tabs(thumbs, &ctx, &app.session.images, anchors, shown_color(uniform));
    let picked = resource_picker::show(&slot.pick, popup, &mut app.thumbs, Some(&current), reveal, tabs);
    if let Some(choice) = picked.filter(|choice| *choice != current) {
        pick_source(app, sheet.ui.ctx(), slot_of, choice, &set);
    }
    if slot.clear {
        app.run("layer.fill.set", set(json!({"value": slot_of.default_value()})));
    }
    if let Some(name) = crate::dock::image_drop(sheet.ui, &slot.pick) {
        app.run("layer.fill.set", set(json!({"image": name})));
    }

    // A slot in a mode's card shows a pattern's Scale and Contrast like every slot; its other rows and Resolution band would split the card
    if in_card {
        if let Source::Procedural { pattern, .. } = source {
            let mut edited = pattern.clone();
            if let Some((key, value, merge)) = scale_and_contrast(sheet, &mut edited) {
                let mut settings = json!({"procedural": pattern.kind.label(), "merge": merge});
                settings[key] = value;
                app.run("layer.fill.set", set(settings));
            }
        }
        return;
    }
    match source {
        Source::Uniform(value) => uniform_rows(app, sheet, slot_of, value, &set),
        Source::Procedural { pattern, low, high } => procedural_rows(app, sheet, slot_of, &pattern, [low, high], slot.reset, &set),
        Source::Image { normal_format, .. } if slot_of.kind() == ChannelKind::NormalMap => {
            if sheet.fold(tr("Attributes"), None, false) {
                let formats = [tr("OpenGL"), tr("DirectX")];
                let chosen = usize::from(normal_format == NormalFormat::DirectX);
                if let Some(picked) = sheet.switch(tr("Normal format"), &formats, chosen) {
                    app.run("layer.fill.set", set(json!({"normalFormat": if picked == 1 { "directX" } else { "openGl" }})));
                }
            }
        }
        Source::Image { .. } | Source::Anchor { .. } => {}
    }
    sheet.close_card();
}

fn pick_source(app: &mut TextureCraftApp, ctx: &egui::Context, slot_of: SourceSlot, choice: SourceChoice, set: &impl Fn(Value) -> Value) {
    match choice {
        SourceChoice::Uniform => {
            app.run("layer.fill.set", set(json!({"value": slot_of.default_value()})));
        }
        SourceChoice::Procedural(kind) => {
            let (low, high) = slot_of.procedural_ends();
            app.run("layer.fill.set", set(json!({"procedural": kind.label(), "low": low, "high": high})));
        }
        SourceChoice::Image(name) => {
            app.run("layer.fill.set", set(json!({"image": name})));
        }
        SourceChoice::Alpha(id) => {
            app.run("layer.fill.set", set(json!({"alpha": id})));
        }
        SourceChoice::Import => crate::menus::invoke(app, ctx, "resource.import"),
        SourceChoice::Anchor(anchor) => {
            app.run("layer.fill.set", set(json!({"anchor": anchor.0})));
        }
    }
}

/// The picker's choice `source` is: a library alpha's image is its alpha
fn source_choice(source: &Source) -> SourceChoice {
    match source {
        Source::Uniform(_) => SourceChoice::Uniform,
        Source::Procedural { pattern, .. } => SourceChoice::Procedural(pattern.kind),
        Source::Image { name, .. } => library_alpha_of(name).map_or_else(|| SourceChoice::Image(name.clone()), |alpha| SourceChoice::Alpha(alpha.id.as_str())),
        Source::Anchor { anchor, .. } => SourceChoice::Anchor(*anchor),
    }
}

/// The source picker's tabs: Resources (Procedurals, Textures with Import... first, Alphas in their shelf groups, Uniform) and the stack's Anchor points
fn source_tabs(thumbs: &mut Thumbs, ctx: &egui::Context, images: &ImageStore, anchors: &[(LayerId, String)], uniform: Color32) -> Vec<PickerTab<SourceChoice>> {
    let procedurals = ProceduralKind::ALL
        .into_iter()
        .map(|kind| tile(SourceChoice::Procedural(kind), tr(kind.label()), None, TilePicture::Texture(thumbs.procedural(ctx, kind))))
        .collect();
    let mut textures = vec![tile(SourceChoice::Import, tr("Import..."), None, TilePicture::Import)];
    let mut names: Vec<&String> = images.keys().filter(|name| library_alpha_of(name).is_none()).collect();
    names.sort();
    textures.extend(names.into_iter().filter_map(|name| images.get(name).map(|image| (name, image))).map(|(name, image)| {
        let picture = TilePicture::Texture(Some(thumbs.image(ctx, name, image)));
        tile(SourceChoice::Image(name.clone()), name, None, picture)
    }));
    let alphas = alphas::library()
        .iter()
        .map(|alpha| {
            let picture = TilePicture::Texture(thumbs.alpha(ctx, alpha));
            tile(SourceChoice::Alpha(alpha.id.as_str()), tr(&alpha.label), Some(tr(alpha.category)), picture)
        })
        .collect();
    let uniform = vec![tile(SourceChoice::Uniform, tr("Uniform value"), None, TilePicture::Swatch(uniform))];
    let resources = [("Procedurals", procedurals), ("Textures", textures), ("Alphas", alphas), ("Uniform", uniform)]
        .into_iter()
        .map(|(header, items)| PickerGroup { header: Some(tr(header)), items })
        .collect();
    let anchors = anchors.iter().map(|(anchor, label)| tile(SourceChoice::Anchor(*anchor), label, None, TilePicture::Glyph(ic::ANCHOR))).collect();
    vec![
        PickerTab { icon: ic::SQUARES_FOUR, name: tr("Resources"), groups: resources },
        PickerTab { icon: ic::ANCHOR, name: tr("Anchor points"), groups: vec![PickerGroup { header: None, items: anchors }] },
    ]
}

/// One tile of the source picker
fn tile(id: SourceChoice, label: &str, caption: Option<&'static str>, picture: TilePicture) -> ResourcePickerItem<SourceChoice> {
    ResourcePickerItem { id, label: label.to_owned(), caption, picture }
}

/// A uniform source: the colour bar (a click opens the picker) or the gray ramp with its invert glyph
fn uniform_rows(app: &mut TextureCraftApp, sheet: &mut PropertySheet<'_>, slot_of: SourceSlot, value: [f32; 3], set: &impl Fn(Value) -> Value) {
    let label = slot_of.name();
    if slot_of.kind() == ChannelKind::Color {
        let pick = sheet.color_bar(label, value, app.picking_color, &app.session.doc.project.swatches);
        if let Some(color) = take_color_pick(app, pick) {
            app.run("layer.fill.set", set(json!({"color": color, "merge": pick.merge})));
        }
        return;
    }
    let mut gray = value[0];
    let edit = sheet.ramp(label, &mut gray, slot_of.range());
    if edit.invert {
        let inverted = if slot_of.kind() == ChannelKind::Signed { -gray } else { 1.0 - gray };
        app.run("layer.fill.set", set(json!({"value": inverted})));
    } else if let Some(merge) = edit.changed {
        app.run("layer.fill.set", set(json!({"value": gray, "merge": merge})));
    }
}

/// The colour `pick` picked, after starting or ending an eyedropper pick its eyedropper asked for and making the change its Swatches row asked for
pub(crate) fn take_color_pick(app: &mut TextureCraftApp, pick: ColorPick) -> Option<[f32; 3]> {
    if pick.eyedropper {
        app.picking_color = if app.picking_color == Some(pick.id) { None } else { Some(pick.id) };
    }
    match pick.swatch {
        Some(SwatchEdit::Add(color)) => {
            app.run("swatch.add", json!({"color": color}));
        }
        Some(SwatchEdit::Remove(index)) => {
            app.run("swatch.remove", json!({"index": index}));
        }
        None => {}
    }
    pick.picked
}

/// A pattern's Scale and Contrast rows; returns the edit (key, value, whether it continues a drag)
fn scale_and_contrast(sheet: &mut PropertySheet<'_>, edited: &mut Procedural) -> Option<(&'static str, Value, bool)> {
    let mut edit = None;
    if let Some(merge) = sheet.slider(tr("Scale"), &mut edited.scale, PATTERN_SCALE, 2) {
        edit = Some(("scale", json!(edited.scale), merge));
    }
    if let Some(merge) = sheet.slider(tr("Contrast"), &mut edited.contrast, PATTERN_CONTRAST, 2) {
        edit = edit.or(Some(("contrast", json!(edited.contrast), merge)));
    }
    edit
}

/// A pattern's Balance and Seed rows; returns the edit like [`scale_and_contrast`]
fn balance_and_seed(sheet: &mut PropertySheet<'_>, edited: &mut Procedural) -> Option<(&'static str, Value, bool)> {
    let mut edit = None;
    if let Some(merge) = sheet.signed_slider(tr("Balance"), &mut edited.balance, -1.0..=1.0, 2) {
        edit = Some(("balance", json!(edited.balance), merge));
    }
    if sheet.seed(tr("Seed"), &mut edited.seed) {
        edit = edit.or(Some(("seed", json!(edited.seed), false)));
    }
    edit
}

/// A procedural source: Scale and Contrast, then its Attributes (seed, balance, invert and its two values), each sending only its own parameter
fn procedural_rows(
    app: &mut TextureCraftApp,
    sheet: &mut PropertySheet<'_>,
    slot_of: SourceSlot,
    pattern: &Procedural,
    [low, high]: [[f32; 3]; 2],
    reset: bool,
    set: &impl Fn(Value) -> Value,
) {
    let kind = pattern.kind.label();
    if reset {
        let fresh = Procedural::new(pattern.kind);
        let settings = json!({
            "procedural": kind, "scale": fresh.scale, "contrast": fresh.contrast, "seed": fresh.seed,
            "balance": fresh.balance, "invert": fresh.invert, "low": low, "high": high, "merge": false,
            "resolution": {"relative": 0},
        });
        app.run("layer.fill.set", set(settings));
        return;
    }

    let mut edited = pattern.clone();
    let mut edit = scale_and_contrast(sheet, &mut edited);
    // A grayscale's card (a mask fill's, a filter input's) shows its Balance and Seed with Scale and Contrast; a material channel's keeps them in Attributes
    let gray = matches!(slot_of, SourceSlot::Grayscale | SourceSlot::Input(_));
    if gray {
        edit = edit.or(balance_and_seed(sheet, &mut edited));
    }
    if sheet.fold(tr("Attributes"), None, false) {
        if !gray {
            edit = edit.or(balance_and_seed(sheet, &mut edited));
        }
        if sheet.toggle(tr("Invert"), &mut edited.invert) {
            edit = edit.or(Some(("invert", json!(edited.invert), false)));
        }
        let ends = [("low", tr("Low"), low), ("high", tr("High"), high)];
        if slot_of.kind() == ChannelKind::Color {
            for (key, label, value) in ends {
                let pick = sheet.color_bar_led(label, value, app.picking_color, &app.session.doc.project.swatches);
                if let Some(picked) = take_color_pick(app, pick) {
                    edit = edit.or(Some((key, json!(picked), pick.merge)));
                }
            }
        } else {
            for (key, label, value) in ends {
                let mut gray = value[0];
                if let Some(merge) = sheet.slider(label, &mut gray, slot_of.range(), 2) {
                    edit = edit.or(Some((key, json!([gray, gray, gray]), merge)));
                }
            }
        }
    }
    // The size the pattern is made at, as a cooked texture would be (the texture set's own by default)
    let texture_set = app.session.doc.project.texture_set(app.session.ts()).map_or(2048, |texture_set| texture_set.resolution);
    if let Some(picked) = resolution_row(sheet, pattern.resolution.unwrap_or(CookResolution::TEXTURE_SET), texture_set) {
        let size = match picked {
            CookResolution::Relative(factor) => json!({"relative": factor}),
            CookResolution::Fixed(size) => json!({"fixed": size}),
        };
        edit = edit.or(Some(("resolution", size, false)));
    }
    if let Some((key, value, merge)) = edit {
        let mut settings = json!({"procedural": kind, "merge": merge});
        settings[key] = value;
        app.run("layer.fill.set", set(settings));
    }
}

/// The Material side: the fill's whole material in one card (the row lists its presets), its inputs, then opening it in MaterialCraft
fn material_card(app: &mut TextureCraftApp, sheet: &mut PropertySheet<'_>, link: &MaterialLink, layer: LayerId, look: &OrbMaterial, place: &str) {
    let tokens = Tokens::get(sheet.ui.ctx());
    let base = json!({"layer": layer.0, "cook": false});
    let slot = sheet.slot(&|painter, rect| material_orb(painter, rect, look, &tokens, &format!("card{place}")), &link.name, tr("Material"), None, false, false);
    let presets: Vec<usize> = (0..link.presets.len()).collect();
    let preset_name = |index: usize| link.presets.get(index).map(|preset| preset.name.clone()).unwrap_or_default();
    let editable = cfg!(not(target_arch = "wasm32"));
    if editable
        && let (Some(index), _) = sheet.hanging_list(&slot.pick, usize::MAX, &presets, &preset_name, None)
        && app.run("material.link.preset", with(&base, json!({"name": preset_name(index)}))).is_some()
    {
        app.cook_link(layer);
    }

    let pending = sheet.ui.make_persistent_id(("material_cook_pending", layer.0));
    for param in &link.params {
        if let Some(value) = link_input(sheet, layer, param, editable)
            && app.run("material.link.set", with(&base, json!({"id": param.id, "value": value}))).is_some()
        {
            sheet.ui.data_mut(|data| data.insert_temp(pending, true));
        }
    }
    // A drag stores every step; the maps re-cook once the pointer lets go
    let waiting = sheet.ui.data(|data| data.get_temp::<bool>(pending)).unwrap_or(false);
    if waiting && !sheet.ui.input(|input| input.pointer.any_down()) {
        sheet.ui.data_mut(|data| data.remove::<bool>(pending));
        app.cook_link(layer);
    }
    sheet.close_card();
}

/// The Material side of a fill without a linked material: one card holding the built-in material its sources came from (the row lists every plain material, × returns the channels to their defaults)
fn whole_material_card(app: &mut TextureCraftApp, sheet: &mut PropertySheet<'_>, fill: &Fill, base: &Value, look: &OrbMaterial, place: &str) {
    let tokens = Tokens::get(sheet.ui.ctx());
    let held = fill.material.as_deref().and_then(find);
    let name = held.map_or_else(|| tr("None"), |material| tr(material.label));
    let slot =
        sheet.slot(&|painter, rect| material_orb(painter, rect, look, &tokens, &format!("card{place}")), name, tr("Material"), None, held.is_some(), false);
    let popup = sheet.ui.make_persistent_id(("material_picker", base.to_string()));
    if slot.pick.clicked() {
        resource_picker::toggle(sheet.ui.ctx(), popup);
    }
    let current = held.map(|material| material.id);
    let ctx = sheet.ui.ctx().clone();
    let picked = resource_picker::show(&slot.pick, popup, &mut app.thumbs, current.as_ref(), current.is_some(), |thumbs| material_tabs(&ctx, thumbs));
    if let Some(id) = picked.filter(|id| Some(*id) != current) {
        app.run("layer.fill.material", with(base, json!({"material": id})));
    }
    if slot.clear {
        app.run("layer.fill.material", with(base, json!({"material": null})));
    }
    sheet.close_card();
}

/// The material picker's one tab: every plain material's preview ball, grouped under its shelf category in library order
fn material_tabs(ctx: &egui::Context, thumbs: &mut Thumbs) -> Vec<PickerTab<&'static str>> {
    let mut groups: Vec<PickerGroup<&'static str>> = Vec::new();
    for material in whole_materials() {
        let item = ResourcePickerItem {
            id: material.id,
            label: tr(material.label).to_owned(),
            caption: None,
            picture: TilePicture::Texture(thumbs.material(ctx, material.id)),
        };
        let header = Some(tr(material.category));
        match groups.iter_mut().find(|group| group.header == header) {
            Some(group) => group.items.push(item),
            None => groups.push(PickerGroup { header, items: vec![item] }),
        }
    }
    vec![PickerTab { icon: ic::SPHERE, name: tr("Materials"), groups }]
}

/// Channels mapping: which of the material's outputs each channel that is on reads
fn mapping_band(app: &mut TextureCraftApp, sheet: &mut PropertySheet<'_>, fill: &Fill, shown: &[Channel], layer: LayerId) {
    let Some(link) = &fill.link else { return };
    if !sheet.band(tr("Channels mapping"), true, None, None).open {
        return;
    }
    let outputs: Vec<String> = fill.link_outputs().into_keys().collect();
    let indices: Vec<usize> = (0..outputs.len()).collect();
    for channel in shown {
        let reads = link.output_of(*channel);
        let current = outputs.iter().position(|output| output == reads).unwrap_or(usize::MAX);
        let name = |index: usize| outputs.get(index).map_or(reads, String::as_str).to_owned();
        if let (Some(index), _) = sheet.dropdown(tr(channel.label()), current, &indices, name, None)
            && let Some(output) = outputs.get(index)
        {
            app.run("material.link.map", json!({"layer": layer.0, "channel": channel.id(), "output": output}));
        }
    }
}

/// One input of the material: a slider for numbers, a toggle, a dropdown for choices; its new typed value when it changed
fn link_input(sheet: &mut PropertySheet<'_>, layer: LayerId, param: &LinkParam, editable: bool) -> Option<Value> {
    let kind = param.default.get("t").and_then(Value::as_str).unwrap_or("");
    let value = param.value.get("v").unwrap_or(&Value::Null);
    let number = value.as_f64().unwrap_or(0.0) as f32;
    let (low, high) = if param.max > param.min { (param.min, param.max) } else { (0.0, 1.0) };
    let changed = match kind {
        "bool" => {
            let mut on = value.as_bool().unwrap_or(number != 0.0);
            sheet.toggle(&param.label, &mut on).then(|| json!(on))
        }
        "enum" if !param.options.is_empty() => {
            let options: Vec<usize> = (0..param.options.len()).collect();
            let name = |index: usize| param.options.get(index).cloned().unwrap_or_default();
            sheet.dropdown(&param.label, number.max(0.0) as usize, &options, name, None).0.map(|index| json!(index))
        }
        "float" | "int" | "enum" => {
            let mut edited = number;
            let decimals = if kind == "float" { 2 } else { 0 };
            sheet.slider(&param.label, &mut edited, low.min(number)..=high.max(number), decimals).map(|_| json!(edited))
        }
        // Colours and vectors keep their egui editors until the sheet draws them
        _ => sheet.embed(|ui| crate::dock::link_param_editor(ui, layer, param)),
    };
    changed.filter(|_| editable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_orb_shows_the_channel_the_layers_panel_shows_and_the_whole_material_on_material() {
        let mut ui = UiState::default();
        let on = [Channel::BaseColor, Channel::Roughness];

        let whole = orb_look(&ui, &Fill::default(), &on, &HashMap::new());
        ui.layer_material_view = false;
        ui.layer_channel = Channel::Roughness;
        let roughness = orb_look(&ui, &Fill::default(), &on, &HashMap::new());

        assert_eq!(whole.alone(), None, "the list starts on Material: the whole material");
        assert_eq!(roughness.alone(), Some(Channel::Roughness));
        assert_ne!(whole.key(), roughness.key(), "a new pick draws a new orb");
    }
}
