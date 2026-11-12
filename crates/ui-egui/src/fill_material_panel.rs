//! A fill's Material section: the channel strip, the orb with the Material | Channels switch, then one slot card per channel that is on (Channels) or the fill's whole material (Material)

use egui::{Color32, Painter, Rect};
use serde_json::{Value, json};
use texturecraft_engine::core::{
    Channel, ChannelKind, CookResolution, Fill, LayerId, LinkParam, MaterialLink, NormalFormat, Procedural, ProceduralKind, Source,
};
use texturecraft_engine::fill_material_cmds::whole_materials;
use texturecraft_engine::materials::{SmartMaterial, find};
use texturecraft_engine::texel::linear_to_srgb;

use crate::TextureCraftApp;
use crate::color_picker::{ColorPick, SwatchEdit};
use crate::dock::{anchors_below, with};
use crate::i18n::{tr, trf};
use crate::material_orb::material_orb;
use crate::orb_material::OrbMaterial;
use crate::property_sheet::PropertySheet;
use crate::resolution_band::resolution_band;
use crate::slot_thumbnail::{paint_thumbnail, thumbnail};
use crate::theme::Tokens;

/// The switch's two sides: the fill's whole material, or one source per channel
const MODES: [&str; 2] = ["Material", "Channels"];
const MATERIAL_MODE: usize = 0;
const CHANNELS_MODE: usize = 1;

/// Pattern scale and contrast a procedural's sliders run over (typed values stay inside them too)
const PATTERN_SCALE: std::ops::RangeInclusive<f32> = 0.1..=32.0;
const PATTERN_CONTRAST: std::ops::RangeInclusive<f32> = 0.0..=2.0;

/// A source the slot's list offers
#[derive(Clone, Copy, Debug, PartialEq)]
enum SourceChoice {
    Uniform,
    Procedural(ProceduralKind),
    Image(usize),
    Import,
    Anchor(usize),
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
    let place = base.to_string();
    let orb = |painter: &Painter, rect: Rect| material_orb(painter, rect, &look, &tokens, &format!("orb{place}"));
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
                channel_card(app, sheet, fill, base, channel, &anchors);
            }
        }
    }
}

/// A channel's name on its pill: the reference strip's short Title Case words, the full name where it has none
fn pill_name(channel: Channel) -> &'static str {
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
fn source_line(app: &TextureCraftApp, source: &Source) -> String {
    match source {
        Source::Uniform(_) => tr("Uniform").to_owned(),
        Source::Procedural { pattern, .. } => format!("{} · {}", tr(pattern.kind.label()), tr("procedural")),
        Source::Image { name, .. } => format!("{name} · {}", tr("image")),
        Source::Anchor { anchor, .. } => {
            let name = app.session.doc.project.effect(*anchor).map_or_else(|| tr("missing").to_owned(), |(_, effect)| effect.name.clone());
            format!("{} · {name}", tr("Anchor"))
        }
    }
}

/// One channel's card: its slot (the row opens the source list), then the source's own settings
fn channel_card(app: &mut TextureCraftApp, sheet: &mut PropertySheet<'_>, fill: &Fill, base: &Value, channel: Channel, anchors: &[(LayerId, String)]) {
    let set = |extra: Value| with(base, with(&json!({"channel": channel.id()}), extra));
    let source = fill.source(channel);
    let tokens = Tokens::get(sheet.ui.ctx());
    let picture = thumbnail(app, &sheet.ui.ctx().clone(), &source);
    let line = source_line(app, &source);
    let uniform = matches!(source, Source::Uniform(_));
    let procedural = matches!(source, Source::Procedural { .. });
    let slot = sheet.slot(&|painter, rect| paint_thumbnail(painter, rect, &picture, &tokens), tr(channel.label()), &line, None, !uniform, procedural);

    let images: Vec<String> = app.session.doc.resources.keys().cloned().collect();
    let mut options = vec![SourceChoice::Uniform];
    options.extend(ProceduralKind::ALL.into_iter().map(SourceChoice::Procedural));
    options.extend((0..images.len()).map(SourceChoice::Image));
    options.push(SourceChoice::Import);
    options.extend((0..anchors.len()).map(SourceChoice::Anchor));
    let current = match &source {
        Source::Uniform(_) => SourceChoice::Uniform,
        Source::Procedural { pattern, .. } => SourceChoice::Procedural(pattern.kind),
        Source::Image { name, .. } => images.iter().position(|image| image == name).map_or(SourceChoice::Import, SourceChoice::Image),
        Source::Anchor { anchor, .. } => anchors.iter().position(|(id, _)| id == anchor).map_or(SourceChoice::Uniform, SourceChoice::Anchor),
    };
    let name = |choice: SourceChoice| match choice {
        SourceChoice::Uniform => tr("Uniform").to_owned(),
        SourceChoice::Procedural(kind) => tr(kind.label()).to_owned(),
        SourceChoice::Image(index) => images.get(index).cloned().unwrap_or_default(),
        SourceChoice::Import => tr("Import...").to_owned(),
        SourceChoice::Anchor(index) => anchors.get(index).map(|(_, label)| trf("Anchor: {name}", &[("name", label)])).unwrap_or_default(),
    };
    let (picked, _) = sheet.hanging_list(&slot.pick, current, &options, &name, None);
    if let Some(choice) = picked.filter(|choice| *choice != current) {
        pick_source(app, sheet.ui.ctx(), channel, choice, &images, anchors, &set);
    }
    if slot.clear {
        app.run("layer.fill.set", set(json!({"value": channel.default_value()})));
    }
    if let Some(name) = crate::dock::image_drop(sheet.ui, &slot.pick) {
        app.run("layer.fill.set", set(json!({"image": name})));
    }

    match source {
        Source::Uniform(value) => uniform_rows(app, sheet, channel, value, &set),
        Source::Procedural { pattern, low, high } => procedural_rows(app, sheet, channel, &pattern, [low, high], slot.reset, &set),
        Source::Image { normal_format, .. } if channel.kind() == ChannelKind::NormalMap => {
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

fn pick_source(
    app: &mut TextureCraftApp,
    ctx: &egui::Context,
    channel: Channel,
    choice: SourceChoice,
    images: &[String],
    anchors: &[(LayerId, String)],
    set: &impl Fn(Value) -> Value,
) {
    match choice {
        SourceChoice::Uniform => {
            app.run("layer.fill.set", set(json!({"value": channel.default_value()})));
        }
        SourceChoice::Procedural(kind) => {
            let (low, high) = match channel {
                Channel::BaseColor => ([0.15; 3], [0.85; 3]),
                Channel::Height => ([-0.05; 3], [0.05; 3]),
                _ => ([0.2; 3], [0.8; 3]),
            };
            app.run("layer.fill.set", set(json!({"procedural": kind.label(), "low": low, "high": high})));
        }
        SourceChoice::Image(index) => {
            if let Some(name) = images.get(index) {
                app.run("layer.fill.set", set(json!({"image": name})));
            }
        }
        SourceChoice::Import => crate::menus::invoke(app, ctx, "resource.import"),
        SourceChoice::Anchor(index) => {
            if let Some((anchor, _)) = anchors.get(index) {
                app.run("layer.fill.set", set(json!({"anchor": anchor.0})));
            }
        }
    }
}

/// A uniform source: the colour bar (a click opens the picker) or the gray ramp with its invert glyph
fn uniform_rows(app: &mut TextureCraftApp, sheet: &mut PropertySheet<'_>, channel: Channel, value: [f32; 3], set: &impl Fn(Value) -> Value) {
    let label = tr(channel.label());
    if channel.kind() == ChannelKind::Color {
        let pick = sheet.color_bar(label, value, app.picking_color, &app.session.doc.project.swatches);
        if let Some(color) = take_color_pick(app, pick) {
            app.run("layer.fill.set", set(json!({"color": color, "merge": pick.merge})));
        }
        return;
    }
    let range = if channel.kind() == ChannelKind::Signed { -1.0..=1.0 } else { 0.0..=1.0 };
    let mut gray = value[0];
    let edit = sheet.ramp(label, &mut gray, range);
    if edit.invert {
        let inverted = if channel.kind() == ChannelKind::Signed { -gray } else { 1.0 - gray };
        app.run("layer.fill.set", set(json!({"value": inverted})));
    } else if let Some(merge) = edit.changed {
        app.run("layer.fill.set", set(json!({"value": gray, "merge": merge})));
    }
}

/// The colour `pick` picked, after starting or ending an eyedropper pick its eyedropper asked for and making the change its Swatches row asked for
fn take_color_pick(app: &mut TextureCraftApp, pick: ColorPick) -> Option<[f32; 3]> {
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

/// A procedural source: Scale and Contrast, then its Attributes (seed, balance, invert and its two values)
fn procedural_rows(
    app: &mut TextureCraftApp,
    sheet: &mut PropertySheet<'_>,
    channel: Channel,
    pattern: &Procedural,
    [low, high]: [[f32; 3]; 2],
    reset: bool,
    set: &impl Fn(Value) -> Value,
) {
    let send = |app: &mut TextureCraftApp, pattern: &Procedural, low: [f32; 3], high: [f32; 3], merge: bool| {
        let settings = json!({
            "procedural": pattern.kind.label(), "scale": pattern.scale, "contrast": pattern.contrast, "seed": pattern.seed,
            "balance": pattern.balance, "invert": pattern.invert, "low": low, "high": high, "merge": merge,
        });
        app.run("layer.fill.set", set(settings));
    };
    if reset {
        send(app, &Procedural::new(pattern.kind), low, high, false);
        return;
    }

    let mut edited = pattern.clone();
    let mut merge = None;
    merge = merge.or(sheet.slider(tr("Scale"), &mut edited.scale, PATTERN_SCALE, 2));
    merge = merge.or(sheet.slider(tr("Contrast"), &mut edited.contrast, PATTERN_CONTRAST, 2));
    let (mut low, mut high) = (low, high);
    if sheet.fold(tr("Attributes"), None, false) {
        if sheet.seed(tr("Seed"), &mut edited.seed) {
            merge = merge.or(Some(false));
        }
        merge = merge.or(sheet.signed_slider(tr("Balance"), &mut edited.balance, -1.0..=1.0, 2));
        if sheet.toggle(tr("Invert"), &mut edited.invert) {
            merge = merge.or(Some(false));
        }
        if channel.kind() == ChannelKind::Color {
            for (label, value) in [(tr("Low"), &mut low), (tr("High"), &mut high)] {
                let pick = sheet.color_bar_led(label, *value, app.picking_color, &app.session.doc.project.swatches);
                if let Some(picked) = take_color_pick(app, pick) {
                    *value = picked;
                    merge = merge.or(Some(pick.merge));
                }
            }
        } else {
            let range = if channel.kind() == ChannelKind::Signed { -1.0..=1.0 } else { 0.0..=1.0 };
            for (label, value) in [(tr("Low"), &mut low), (tr("High"), &mut high)] {
                let mut gray = value[0];
                if let Some(step) = sheet.slider(label, &mut gray, range.clone(), 2) {
                    *value = [gray; 3];
                    merge = merge.or(Some(step));
                }
            }
        }
    }
    if let Some(merge) = merge {
        send(app, &edited, low, high, merge);
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
    let materials: Vec<&'static SmartMaterial> = whole_materials().collect();
    let options: Vec<usize> = (0..materials.len()).collect();
    let current = held.and_then(|material| materials.iter().position(|listed| listed.id == material.id)).unwrap_or(usize::MAX);
    let label = |index: usize| materials.get(index).map(|material| tr(material.label).to_owned()).unwrap_or_default();
    let picture = |painter: &Painter, rect: Rect, index: usize| {
        if let Some(material) = materials.get(index) {
            let look = OrbMaterial::uniform(material.base, material.metallic, material.roughness);
            material_orb(painter, rect, &look, &tokens, material.id);
        }
    };
    if let (Some(index), _) = sheet.hanging_list(&slot.pick, current, &options, &label, Some(&picture))
        && index != current
        && let Some(material) = materials.get(index)
    {
        app.run("layer.fill.material", with(base, json!({"material": material.id})));
    }
    if slot.clear {
        app.run("layer.fill.material", with(base, json!({"material": null})));
    }
    sheet.close_card();
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
