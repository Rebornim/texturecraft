//! Export dialog › Output templates: create and edit export presets. Built-in presets are
//! read-only; the selected user preset is edited as a draft that is saved through
//! `export.preset.save` whenever it validates, so a half-typed file name never reaches disk.

use egui::{RichText, Ui, vec2};
use egui_phosphor::regular as ic;
use serde_json::{Value, json};
use texturecraft_engine::core::{BakedMap, Channel};
use texturecraft_engine::formats::export::{FILE_TOKENS, MAX_PRESET_NAME, sanitize_file_stem};
use texturecraft_engine::formats::{Dest, ExportPreset, FileFormat, MapSpec, SourceRef, VirtualMap};

mod channel_slot;
mod list_action;
mod map_action;
mod map_preview;
mod moved_output;
mod output_kind;
mod planned_output;
mod preview_pictures;
mod preview_text;

use channel_slot::ChannelSlot;
use list_action::ListAction;
use map_action::MapAction;
use map_preview::Preview;
use moved_output::MovedOutput;
use output_kind::OutputKind;
use planned_output::PlannedOutput;
use preview_text::PreviewText;

use crate::TextureCraftApp;
use crate::export_draft::ExportDraft;
use crate::i18n::{n_, tr, trf};
use crate::theme::{Tokens, caption};
use crate::widgets::{chip, icon_button, panel_header};

const ICON: f32 = 20.0;
/// Widths of the presets, input maps and mesh / converted maps columns; the outputs take the rest
const LIST_WIDTH: f32 = 200.0;
const INPUTS_WIDTH: f32 = 170.0;
const MESH_WIDTH: f32 = 160.0;
const OUTPUTS_MIN_WIDTH: f32 = 240.0;
/// Room a column separator takes, with the spacing on both sides
const COLUMN_GAP: f32 = 26.0;
/// An output card's inner margin, and the room kept for the outputs' scroll bar
const CARD_MARGIN: f32 = 6.0;
const SCROLL_BAR_ROOM: f32 = 12.0;
/// Height of a preset or input map row
const ROW_HEIGHT: f32 = 21.0;
const ROW_INDENT: f32 = 14.0;
/// Room under the outputs for the drag hint and the count
const FOOTER_HEIGHT: f32 = 52.0;
const RENAME_ID: &str = "exp_rename";
const RGB: [&str; 3] = ["R", "G", "B"];
/// A channel box's width for a one-letter channel, then for RGB and Gr
const BOX_WIDTH: f32 = 28.0;
const BOX_WIDE_WIDTH: f32 = 40.0;
const BOX_GAP: f32 = 4.0;
/// A channel box's colour bar: inset from its sides, its height and its gap above the box's bottom
const SLOT_BAR_INSET: f32 = 6.0;
const SLOT_BAR_HEIGHT: f32 = 2.0;
/// The outline a box or card shows while something can drop on it
const DROP_HIGHLIGHT_WIDTH: f32 = 1.5;
const DROP_HIGHLIGHT_GAP: f32 = 2.0;
const HANDLE_ICON_SIZE: f32 = 16.0;
/// The $ button inside the name field: its icon size and its inset from the field's edge
const DOLLAR_ICON_SIZE: f32 = 12.0;
const DOLLAR_INSET: f32 = 3.0;
/// A dragged map's chip sits below and right of the pointer, clear of the drop target under it
const DRAG_CHIP_OFFSET: egui::Vec2 = vec2(8.0, 8.0);
/// The Current preset and output count bars' inner margin
const BAR_MARGIN: egui::Margin = egui::Margin::symmetric(8, 4);
const BOX_RADIUS: f32 = 5.0;
/// Saturation and brightness of a map's colour (perceptual)
const BOX_SATURATION: f32 = 0.75;
const BOX_VALUE: f32 = 0.44;
/// A palette chip's height, text size and the room either side of its name
const CHIP_HEIGHT: f32 = 20.0;
const CHIP_TEXT_SIZE: f32 = 11.0;
const CHIP_PADDING: f32 = 7.0;
/// Space above each palette group, and between its chips
const PALETTE_GROUP_GAP: f32 = 6.0;
const PALETTE_CHIP_GAP: f32 = 4.0;
/// The greys a constant box spans
const BOX_CONSTANT_FLOOR: f32 = 40.0;
const BOX_CONSTANT_RANGE: f32 = 160.0;
/// Width of an output's file type and bit depth combos
const FILE_TYPE_WIDTH: f32 = 96.0;
const SIZE_WIDTH: f32 = 46.0;
/// Room at the name field's right end for the $ button
const NAME_DOLLAR_ROOM: f32 = 26.0;
/// Room between an output's file type combo and its icon button
const FILE_TYPE_STACK_GAP: f32 = 12.0;
const FILE_TYPE_STACK_WIDTH: f32 = FILE_TYPE_WIDTH + ICON + FILE_TYPE_STACK_GAP;
/// The file name field's inner margin: its left edge and its top and bottom (the right end is NAME_DOLLAR_ROOM)
const NAME_MARGIN_LEFT: i8 = 4;
const NAME_MARGIN_VERTICAL: i8 = 2;
/// Space under the Presets header, under the Current preset bar and below each output card
const PRESET_LIST_GAP: f32 = 2.0;
const OUTPUT_GAP: f32 = 4.0;
/// Space above the empty outputs hint
const EMPTY_HINT_GAP: f32 = 8.0;
/// Size of the small hint and error lines around the outputs
const HINT_TEXT_SIZE: f32 = 11.0;
/// A preset row's icon size and name size, the name's start and the room the icon and right edge take from its width
const PRESET_ICON_SIZE: f32 = 13.0;
const PRESET_NAME_SIZE: f32 = 12.5;
const PRESET_NAME_INDENT: f32 = 1.5 * ROW_INDENT;
const PRESET_NAME_ROOM: f32 = 2.0 * ROW_INDENT;

/// The Input maps column: the texture set channels by shading lobe
const INPUT_GROUPS: [(&str, &[Channel]); 9] = [
    (n_("Base"), &[Channel::BaseColor, Channel::Metallic]),
    (n_("Specular"), &[Channel::Roughness, Channel::SpecularLevel, Channel::Specular, Channel::AnisotropyLevel, Channel::AnisotropyAngle]),
    (n_("Transmission"), &[Channel::Transmission]),
    (n_("Subsurface"), &[Channel::ScatterWeight]),
    (n_("Coat"), &[Channel::CoatWeight, Channel::CoatRoughness, Channel::CoatNormal]),
    (n_("Fuzz/Sheen"), &[Channel::SheenWeight, Channel::SheenColor, Channel::SheenRoughness]),
    (n_("Emission"), &[Channel::Emissive]),
    (n_("Geometry"), &[Channel::Opacity, Channel::Height, Channel::Normal, Channel::AmbientOcclusion]),
    (n_("Miscellaneous"), &[Channel::Glossiness]),
];

/// OpenPBR's names for the texture set channels (the Naming pattern's other choice)
const OPENPBR_NAMES: [(Channel, &str); 21] = [
    (Channel::BaseColor, n_("Base color")),
    (Channel::Metallic, n_("Base metalness")),
    (Channel::Roughness, n_("Specular roughness")),
    (Channel::SpecularLevel, n_("Specular weight")),
    (Channel::Specular, n_("Specular color")),
    (Channel::AnisotropyLevel, n_("Specular anisotropy")),
    (Channel::AnisotropyAngle, n_("Specular rotation")),
    (Channel::Transmission, n_("Transmission weight")),
    (Channel::ScatterWeight, n_("Subsurface weight")),
    (Channel::CoatWeight, n_("Coat weight")),
    (Channel::CoatRoughness, n_("Coat roughness")),
    (Channel::CoatNormal, n_("Coat normal")),
    (Channel::SheenWeight, n_("Fuzz weight")),
    (Channel::SheenColor, n_("Fuzz color")),
    (Channel::SheenRoughness, n_("Fuzz roughness")),
    (Channel::Emissive, n_("Emission color")),
    (Channel::Opacity, n_("Geometry opacity")),
    (Channel::Height, n_("Height")),
    (Channel::Normal, n_("Geometry normal")),
    (Channel::AmbientOcclusion, n_("Ambient occlusion")),
    (Channel::Glossiness, n_("Glossiness")),
];

/// A channel's name: TextureCraft's, or OpenPBR's when the Naming pattern asks for it
fn channel_name(channel: Channel, openpbr: bool) -> &'static str {
    let openpbr_name = openpbr.then(|| OPENPBR_NAMES.iter().find(|(c, _)| *c == channel).map(|(_, name)| *name)).flatten();
    openpbr_name.unwrap_or_else(|| channel.label())
}

fn source_label(source: &SourceRef, openpbr: bool) -> &'static str {
    match source {
        SourceRef::Channel { channel, .. } => channel_name(*channel, openpbr),
        SourceRef::Virtual { map, .. } => map.label(),
        SourceRef::Baked { map, .. } => crate::i18n::baked_map_label(*map),
        SourceRef::Constant(_) => n_("Constant"),
    }
}

/// `source` reading component `requested` (only 3-component sources have more than one)
fn with_component(source: &SourceRef, requested: u8) -> SourceRef {
    let component_index = if source.components() == 3 { requested.min(2) } else { 0 };
    let mut source = source.clone();
    if let SourceRef::Channel { component, .. } | SourceRef::Virtual { component, .. } | SourceRef::Baked { component, .. } = &mut source {
        *component = component_index;
    }
    source
}

/// The component `source` reads (0 for a constant)
fn component_of(source: &SourceRef) -> u8 {
    match source {
        SourceRef::Channel { component, .. } | SourceRef::Virtual { component, .. } | SourceRef::Baked { component, .. } => *component,
        SourceRef::Constant(_) => 0,
    }
}

/// How close two constants are to count as the same source
const SAME_CONSTANT_TOLERANCE: f32 = 1e-4;

/// Same input map (any component), or the same constant
fn same_source(first: &SourceRef, second: &SourceRef) -> bool {
    match (first, second) {
        (SourceRef::Constant(first_value), SourceRef::Constant(second_value)) => (first_value - second_value).abs() < SAME_CONSTANT_TOLERANCE,
        _ => with_component(first, 0) == with_component(second, 0),
    }
}

/// The component a channel reads by default: R, G, B their own, L and A the first
fn slot_component(dest: Dest) -> u8 {
    match dest {
        Dest::G => 1,
        Dest::B => 2,
        _ => 0,
    }
}

/// The boxes of `map`'s channels: grey, one colour box or three separate ones, then alpha
fn channel_slots(map: &MapSpec) -> Vec<ChannelSlot> {
    let has = |dest: Dest| map.channels.iter().any(|(d, _)| *d == dest);
    let source = |dest: Dest| map.channels.iter().find(|(d, _)| *d == dest).map(|(_, s)| s);
    let mut slots = Vec::new();
    if has(Dest::L) {
        slots.push(ChannelSlot { dests: vec![Dest::L], label: "Gr" });
    }
    if has(Dest::R) || has(Dest::G) || has(Dest::B) {
        let rgb = [Dest::R, Dest::G, Dest::B].map(source);
        let one_colour = match rgb {
            [Some(r), Some(g), Some(b)] if same_source(r, g) && same_source(r, b) => {
                r.components() == 3 && [r, g, b].iter().zip(0u8..).all(|(s, k)| component_of(s) == k) || matches!(r, SourceRef::Constant(_))
            }
            _ => false,
        };
        if one_colour && !map.options.separate_rgb {
            slots.push(ChannelSlot { dests: vec![Dest::R, Dest::G, Dest::B], label: "RGB" });
        } else {
            for (dest, label) in [(Dest::R, "R"), (Dest::G, "G"), (Dest::B, "B")] {
                if has(dest) {
                    slots.push(ChannelSlot { dests: vec![dest], label });
                }
            }
        }
    }
    if has(Dest::A) {
        slots.push(ChannelSlot { dests: vec![Dest::A], label: "A" });
    }

    slots
}

/// `input` dropped on a box: a colour box takes a colour input whole (a grey one in all three), a single box the matching component
fn drop_input(map: &mut MapSpec, slot: &ChannelSlot, input: &SourceRef) {
    for (dest, source) in &mut map.channels {
        if slot.dests.contains(dest) {
            *source = with_component(input, slot_component(*dest));
        }
    }
}

/// Empty a box: black, or white for alpha
fn clear_slot(map: &mut MapSpec, slot: &ChannelSlot) {
    for (dest, source) in &mut map.channels {
        if slot.dests.contains(dest) {
            *source = SourceRef::Constant(if *dest == Dest::A { 1.0 } else { 0.0 });
        }
    }
}

/// The saved (built-in or user) preset selected in the dialog.
fn selected(app: &TextureCraftApp) -> Option<ExportPreset> {
    app.session.export_preset(&app.ui.export.preset)
}

/// Keep the draft on the selected user preset; a valid draft unlike the saved one was changed elsewhere (an agent) and reloads, an invalid one is the user's edit
fn sync_draft(app: &mut TextureCraftApp) {
    let export = &mut app.ui.export;
    match app.session.export_presets.list.iter().find(|preset| preset.name == export.preset) {
        None => export.draft = None,
        Some(saved) => {
            if export.draft.as_ref().is_none_or(|draft| draft.name != saved.name || (draft != saved && draft.validate().is_ok())) {
                export.draft = Some(saved.clone());
            }
        }
    }
}

/// The Output templates tab; `width` is the window's fixed width (widths measured from the ui would grow with any overflow)
pub fn tab(app: &mut TextureCraftApp, ui: &mut Ui, builtins: &[ExportPreset], width: f32) {
    sync_draft(app);
    let height = ui.available_height();
    let outputs_width = (width - LIST_WIDTH - INPUTS_WIDTH - MESH_WIDTH - 3.0 * COLUMN_GAP).max(OUTPUTS_MIN_WIDTH);
    let column = |ui: &mut Ui, width: f32, body: &mut dyn FnMut(&mut Ui)| {
        ui.vertical(|ui| {
            ui.set_width(width);
            ui.set_height(height);
            body(ui);
        });
    };

    let mut list_action = None;
    column(ui, LIST_WIDTH, &mut |ui| list_action = preset_list(app, ui, builtins));
    ui.separator();
    column(ui, outputs_width, &mut |ui| outputs(app, ui));
    ui.separator();
    let openpbr = app.ui.export.openpbr_names;
    // The maps the shown template reads, coloured like their channel boxes
    let shown = app.ui.export.draft.clone().or_else(|| selected(app));
    let used: Vec<SourceRef> =
        shown.iter().flat_map(|preset| preset.maps.iter()).flat_map(|map| map.channels.iter()).map(|(_, source)| with_component(source, 0)).collect();
    column(ui, INPUTS_WIDTH, &mut |ui| input_maps(ui, openpbr, &used));
    ui.separator();
    column(ui, MESH_WIDTH, &mut |ui| mesh_and_converted_maps(ui, &used));
    if let Some(list_action) = list_action {
        apply(app, ui.ctx(), list_action);
    }
}

fn preset_list(app: &mut TextureCraftApp, ui: &mut Ui, builtins: &[ExportPreset]) -> Option<ListAction> {
    let mut action = None;
    let selected = app.ui.export.preset.clone();
    let user = app.ui.export.draft.is_some();
    ui.horizontal(|ui| {
        ui.label(caption(tr("Presets")));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let tool =
                |ui: &mut Ui, enabled: bool, icon: &str, tip: &str| ui.add_enabled_ui(enabled, |ui| icon_button(ui, icon, tip, false, ICON)).inner.clicked();
            if tool(ui, true, ic::PLUS, tr("New template")) {
                action = Some(ListAction::New);
            }
            if tool(ui, user, ic::MINUS, tr("Delete the selected template")) {
                action = Some(ListAction::Remove);
            }
            if tool(ui, true, ic::COPY, tr("Duplicate the selected template")) {
                action = Some(ListAction::Duplicate);
            }
            if tool(ui, true, ic::UPLOAD_SIMPLE, tr("Export the selected template (.json)")) {
                action = Some(ListAction::Export);
            }
            if tool(ui, true, ic::DOWNLOAD_SIMPLE, tr("Import a template (.json)")) {
                action = Some(ListAction::Import);
            }
        });
    });
    ui.add_space(PRESET_LIST_GAP);
    let names: Vec<String> = app.session.export_presets.list.iter().map(|preset| preset.name.clone()).collect();
    egui::ScrollArea::vertical().id_salt("exp_presets").auto_shrink([false; 2]).show(ui, |ui| {
        for preset in builtins {
            let response = preset_row(ui, ic::FILE_LOCK, &preset.name, preset.name == selected).on_hover_text(tr("Built-in (read-only): duplicate it to edit"));

            if response.clicked() {
                action = Some(ListAction::Select(preset.name.clone()));
            }

            response.context_menu(|ui| {
                if ui.button(tr("Duplicate")).clicked() {
                    action = Some(ListAction::DuplicateNamed(preset.name.clone()));
                }
            });
        }
        for name in names {
            match app.ui.export.renaming.as_mut().filter(|_| name == selected) {
                Some(name_buffer) => {
                    let response = ui.add(egui::TextEdit::singleline(name_buffer).id(egui::Id::unique(RENAME_ID)).desired_width(f32::INFINITY));

                    if response.lost_focus() {
                        action = Some(if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                            ListAction::Select(name)
                        } else {
                            ListAction::Rename(name_buffer.clone())
                        });
                    }
                }
                None => {
                    let response = preset_row(ui, ic::FILE_TEXT, &name, name == selected).on_hover_text(tr("Double-click to rename"));

                    if response.double_clicked() {
                        action = Some(ListAction::StartRename(name.clone()));
                    } else if response.clicked() {
                        action = Some(ListAction::Select(name.clone()));
                    }

                    response.context_menu(|ui| {
                        if ui.button(tr("Rename")).clicked() {
                            action = Some(ListAction::StartRename(name.clone()));
                        }
                        if ui.button(tr("Duplicate")).clicked() {
                            action = Some(ListAction::DuplicateNamed(name.clone()));
                        }
                        if ui.button(tr("Remove")).clicked() {
                            action = Some(ListAction::RemoveNamed(name.clone()));
                        }
                    });
                }
            }
        }
    });

    action
}

fn apply(app: &mut TextureCraftApp, ctx: &egui::Context, list_action: ListAction) {
    app.ui.export.renaming = None;
    match list_action {
        ListAction::Select(name) => app.ui.export.preset = name,
        ListAction::New => {
            save_new(app, &ExportPreset { name: tr("New template").into(), maps: Vec::new() });
        }
        ListAction::DuplicateNamed(name) => {
            app.ui.export.preset = name;
            apply(app, ctx, ListAction::Duplicate);
        }
        ListAction::RemoveNamed(name) => {
            app.ui.export.preset = name;
            apply(app, ctx, ListAction::Remove);
        }
        ListAction::Duplicate => {
            if let Some(preset) = selected(app) {
                let name = trf("{name} copy", &[("name", &preset.name)]).chars().take(MAX_PRESET_NAME).collect();
                save_new(app, &ExportPreset { name, ..preset });
            }
        }
        ListAction::StartRename(name) => {
            app.ui.export.preset = name.clone();
            app.ui.export.renaming = Some(name);
            ctx.memory_mut(|m| m.request_focus(egui::Id::unique(RENAME_ID)));
        }
        ListAction::Rename(name) => rename(app, name.trim()),
        ListAction::Remove => {
            let name = app.ui.export.preset.clone();
            if app.run("export.preset.remove", json!({ "name": name })).is_some() {
                app.ui.export.preset = crate::state::ExportUi::default().preset;
                follow_rename(app, &name, &app.ui.export.preset.clone());
                app.toast(trf("Deleted template \"{name}\"", &[("name", &name)]), false);
            }
        }
        ListAction::Import => import_file(app, ctx),
        ListAction::Export => export_file(app),
    }
}

/// Save `preset` as a new user preset (renamed when its name is taken) and select it
fn save_new(app: &mut TextureCraftApp, preset: &ExportPreset) -> Option<String> {
    let preset_json = serde_json::to_value(preset).ok()?;
    let reply = app.run("export.preset.save", json!({ "preset": preset_json, "unique": true }))?;
    let name = reply.get("name").and_then(Value::as_str)?.to_string();
    app.ui.export.preset = name.clone();
    Some(name)
}

fn rename(app: &mut TextureCraftApp, name: &str) {
    let Some(mut preset) = app.ui.export.draft.clone() else { return };
    if name.is_empty() || name == preset.name {
        return;
    }

    let old = std::mem::replace(&mut preset.name, name.to_string());
    let Ok(preset_json) = serde_json::to_value(&preset) else { return };
    if let Some(reply) = app.run("export.preset.save", json!({ "preset": preset_json, "rename": old }))
        && let Some(saved_name) = reply.get("name").and_then(Value::as_str)
    {
        app.ui.export.preset = saved_name.to_string();
        follow_rename(app, &old, saved_name);
    }
}

/// Export settings that chose a renamed or removed template follow it (`new` is the default after a removal)
fn follow_rename(app: &mut TextureCraftApp, old: &str, new: &str) {
    if let Some(draft) = app.ui.export.settings.as_mut() {
        rename_choices(draft, old, new);
    }

    // The saved settings too, so a later Cancel can't leave them naming a template that's gone
    let project = &app.session.doc.project;
    let saved_uses_it = project.export.as_ref().is_some_and(|export| export.parameters.preset == old)
        || project.texture_sets.iter().any(|set| set.export.overrides.preset.as_deref() == Some(old));
    if saved_uses_it {
        let mut saved = ExportDraft::from_project(project, "");
        rename_choices(&mut saved, old, new);
        app.run("export.settings.set", saved.params());
    }
}

/// Every template choice in `draft` that names `old` names `new`
fn rename_choices(draft: &mut ExportDraft, old: &str, new: &str) {
    let overrides = draft.sets.iter_mut().filter_map(|set| set.overrides.preset.as_mut());
    for preset in std::iter::once(&mut draft.settings.parameters.preset).chain(overrides) {
        if preset == old {
            *preset = new.to_string();
        }
    }
}

/// Add an imported preset (renamed when its name is taken) and select it.
pub fn import(app: &mut TextureCraftApp, preset: &ExportPreset) {
    if let Some(name) = save_new(app, preset) {
        app.toast(trf("Imported template \"{name}\"", &[("name", &name)]), false);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn import_file(app: &mut TextureCraftApp, _ctx: &egui::Context) {
    use crate::file_dialogs::{Bound, FileDialog};
    app.ask_file(FileDialog::open().filter(tr("Export template"), &["json"]), Bound::App, |app, paths| {
        let Some(path) = paths.first() else { return };
        match texturecraft_engine::formats::load_preset(path) {
            Ok(p) => import(app, &p),
            Err(e) => app.toast(trf("Couldn't import {name}: {error}", &[("name", &path.display()), ("error", &e)]), true),
        }
    });
}

#[cfg(target_arch = "wasm32")]
fn import_file(_app: &mut TextureCraftApp, ctx: &egui::Context) {
    crate::web_files::import_preset(ctx);
}

fn export_file(app: &mut TextureCraftApp) {
    let Some(p) = selected(app) else { return };
    let name = format!("{}.json", sanitize_file_stem(&p.name));
    let json = match texturecraft_engine::formats::preset_to_json(&p) {
        Ok(json) => json,
        Err(e) => {
            export_failed(app, &e);
            return;
        }
    };
    write_json(app, p.name, name, json);
}

fn export_failed(app: &mut TextureCraftApp, error: &dyn std::fmt::Display) {
    app.toast(trf("Couldn't export the template: {error}", &[("error", error)]), true);
}

fn exported(app: &mut TextureCraftApp, name: &str, to: &str) {
    app.toast(trf("Exported template \"{name}\" to {path}", &[("name", &name), ("path", &to)]), false);
}

/// Asks where to save `json` with the native dialog.
#[cfg(not(target_arch = "wasm32"))]
fn write_json(app: &mut TextureCraftApp, preset: String, name: String, json: String) {
    use crate::file_dialogs::{Bound, FileDialog};
    app.ask_file(FileDialog::save().filter(tr("Export template"), &["json"]).file_name(name), Bound::App, move |app, paths| {
        let Some(path) = paths.first() else { return };
        match std::fs::write(path, &json) {
            Ok(()) => exported(app, &preset, &path.display().to_string()),
            Err(e) => export_failed(app, &e),
        }
    });
}

#[cfg(target_arch = "wasm32")]
fn write_json(app: &mut TextureCraftApp, preset: String, name: String, json: String) {
    match crate::web_files::download(&name, json.as_bytes()) {
        Ok(()) => exported(app, &preset, &name),
        Err(e) => export_failed(app, &e),
    }
}

/// Read the user's templates: the library folder on the desktop, the app storage on the web.
pub fn load_library(app: &mut TextureCraftApp, storage: Option<&dyn eframe::Storage>) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = storage;
        let skipped = app.session.load_export_presets(crate::user_dir("export-presets"));
        app.session.log.extend(skipped.into_iter().map(|w| format!("Export template skipped: {w}")));
    }
    #[cfg(target_arch = "wasm32")]
    for p in storage.and_then(|s| eframe::get_value::<Vec<ExportPreset>>(s, STORAGE_KEY)).unwrap_or_default() {
        if let Ok(v) = serde_json::to_value(&p) {
            app.run("export.preset.save", json!({ "preset": v, "unique": true }));
        }
    }
}

#[cfg(target_arch = "wasm32")]
const STORAGE_KEY: &str = "export_presets";

/// Keep the user's templates in the app storage (the web has no library folder)
pub fn save_library(app: &TextureCraftApp, storage: &mut dyn eframe::Storage) {
    #[cfg(target_arch = "wasm32")]
    eframe::set_value(storage, STORAGE_KEY, &app.session.export_presets.list);
    #[cfg(not(target_arch = "wasm32"))]
    let _ = (app, storage);
}

/// The selected template's output maps (editable for user templates)
fn outputs(app: &mut TextureCraftApp, ui: &mut Ui) {
    let tk = Tokens::get(ui.ctx());
    let (mut preset, editable) = match (&app.ui.export.draft, selected(app)) {
        (Some(draft), _) => (draft.clone(), true),
        (None, Some(builtin)) => (builtin, false),
        (None, None) => {
            ui.label(RichText::new(tr("Select an output template")).color(tk.text_dim));
            return;
        }
    };
    let params = plan_params(app, &preset);
    let preview = params.as_ref().map_err(String::clone).and_then(|params| planned_files(app, params));
    let mut action = None;
    egui::Frame::new().fill(tk.row).inner_margin(BAR_MARGIN).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(RichText::new(tr("Current preset:")).strong().color(tk.text_strong));
            ui.label(&preset.name);
            if !editable {
                ui.label(RichText::new(ic::LOCK_SIMPLE).color(tk.text_dim)).on_hover_text(tr("Built-in templates are read-only: duplicate one to edit it"));
            }
        });
    });
    ui.add_space(OUTPUT_GAP);
    ui.label(RichText::new(tr("Output maps")).strong().color(tk.text_strong));
    ui.horizontal(|ui| {
        ui.label(RichText::new(tr("Create:")).color(tk.text_dim));
        ui.add_enabled_ui(editable, |ui| {
            for kind in OutputKind::ALL {
                if ui.small_button(tr(kind.label())).clicked() {
                    action = Some(MapAction::Add(kind));
                }
            }
        });
    });
    if let Err(error) = &preview {
        ui.label(RichText::new(trf("Not saved: {error}", &[("error", error)])).color(tk.danger).size(HINT_TEXT_SIZE));
    }
    let mut changed = false;
    let map_count = preset.maps.len();
    let exported = preview.as_ref().map(|names| names.iter().flatten().count()).unwrap_or(0);
    let card_width = ui.available_width() - SCROLL_BAR_ROOM;
    let openpbr = app.ui.export.openpbr_names;
    let ctx = ui.ctx().clone();
    egui::ScrollArea::vertical().id_salt("exp_outputs").max_height(ui.available_height() - FOOTER_HEIGHT).auto_shrink([false; 2]).show(ui, |ui| {
        if map_count == 0 {
            ui.add_space(EMPTY_HINT_GAP);
            ui.label(RichText::new(tr("No outputs yet: create one above, then drag maps onto its channels.")).color(tk.text_dim));
        }
        ui.add_enabled_ui(editable, |ui| {
            for (output_index, map) in preset.maps.iter_mut().enumerate() {
                let planned = preview.as_ref().ok().and_then(|files| files.get(output_index).cloned().flatten());
                let mut pictures = || match &params {
                    Ok(params) => map_preview::preview(app, &ctx, params, output_index),
                    Err(_) => None,
                };
                changed |= output_card(ui, card_width, output_index, map, planned.as_ref(), openpbr, &mut pictures, &mut action);
                ui.add_space(OUTPUT_GAP);
            }
        });
    });
    ui.label(RichText::new(tr("Drag maps from the columns on the right onto an output's channels")).size(HINT_TEXT_SIZE).color(tk.text_dim));
    egui::Frame::new().fill(tk.row).inner_margin(BAR_MARGIN).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(trf("{n} output maps will be exported", &[("n", &exported)]));
    });
    if !editable {
        return;
    }
    match action {
        Some(MapAction::Add(kind)) => preset.maps.push(kind.new_map()),
        Some(MapAction::Duplicate(index)) if index < map_count => {
            let copy = preset.maps[index].clone();
            preset.maps.insert(index + 1, copy);
        }
        Some(MapAction::Move(from, to)) if from < map_count && to < map_count && from != to => {
            let moved = preset.maps.remove(from);
            preset.maps.insert(to, moved);
        }
        Some(MapAction::Remove(index)) if index < map_count => {
            preset.maps.remove(index);
        }
        _ if !changed => return,
        _ => {}
    }
    commit(app, preset);
}

/// Keep the edited draft; save it when valid
fn commit(app: &mut TextureCraftApp, preset: ExportPreset) {
    if preset.validate().is_ok()
        && let Ok(preset_json) = serde_json::to_value(&preset)
    {
        app.run("export.preset.save", json!({ "preset": preset_json }));
    }
    app.ui.export.draft = Some(preset);
}

/// The export.plan / export.preview params of `preset` for the current texture set with the dialog's settings
fn plan_params(app: &mut TextureCraftApp, preset: &ExportPreset) -> Result<Value, String> {
    let mut params = crate::export_dialog::global_params(app);
    params["preset"] = serde_json::to_value(preset).map_err(|error| error.to_string())?;
    params["ts"] = json!(app.session.ts());

    Ok(params)
}

/// The file each output writes (None: the set has none of the output's sources), via `export.plan`
fn planned_files(app: &mut TextureCraftApp, params: &Value) -> Result<Vec<Option<PlannedOutput>>, String> {
    let plan = texturecraft_engine::execute(&mut app.session, "export.plan", params).map_err(|error| error.to_string())?;
    let count = params.pointer("/preset/maps").and_then(Value::as_array).map_or(0, Vec::len);
    let mut files = vec![None; count];
    for entry in plan.as_array().into_iter().flatten() {
        let map = entry.get("map").and_then(Value::as_u64).and_then(|index| usize::try_from(index).ok());
        let size = |axis: usize| entry.get("size").and_then(|sizes| sizes.get(axis)).and_then(Value::as_u64).unwrap_or(0);
        if let (Some(slot), Some(file)) = (map.and_then(|index| files.get_mut(index)), entry.get("file").and_then(Value::as_str)) {
            *slot = Some(PlannedOutput { file: file.to_string(), size: [size(0), size(1)] });
        }
    }
    Ok(files)
}

/// One output card, `width` wide like every card (one sized from what the card above left would creep wider)
#[allow(clippy::too_many_arguments)]
fn output_card(
    ui: &mut Ui,
    width: f32,
    output_index: usize,
    map: &mut MapSpec,
    planned: Option<&PlannedOutput>,
    openpbr: bool,
    pictures: &mut dyn FnMut() -> Preview,
    action: &mut Option<MapAction>,
) -> bool {
    let tk = Tokens::get(ui.ctx());
    let mut changed = false;
    let file = planned.map(|planned| planned.file.as_str());
    let text = preview_text(map, planned, openpbr);
    let row_height = ui.spacing().interact_size.y;
    let card = egui::Frame::new().fill(tk.row).corner_radius(tk.radius).inner_margin(CARD_MARGIN).show(ui, |ui| {
        let inner = width - 2.0 * CARD_MARGIN;
        ui.set_width(inner);
        ui.horizontal(|ui| {
            let handle = ui.add(egui::Label::new(RichText::new(ic::DOTS_SIX_VERTICAL).size(HANDLE_ICON_SIZE).color(tk.text_dim)).sense(egui::Sense::drag()));
            handle.dnd_set_drag_payload(MovedOutput(output_index));
            let handle = handle.on_hover_cursor(egui::CursorIcon::Grab);
            if handle.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            }

            // The name and the channels take what the file type stack leaves, so every card's columns line up
            let spacing = ui.spacing().item_spacing.x;
            let main_width = inner - handle.rect.width() - FILE_TYPE_STACK_WIDTH - 2.0 * spacing;
            ui.allocate_ui_with_layout(vec2(main_width, 2.0 * row_height + ui.spacing().item_spacing.y), egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.set_width(main_width);
                changed |= name_field(ui, map, file, main_width);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = BOX_GAP;
                    for slot in channel_slots(map).iter() {
                        changed |= channel_box(ui, map, slot, openpbr, &mut *pictures, &text);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| changed |= size_combo(ui, output_index, map));
                });
            });

            ui.allocate_ui_with_layout(vec2(FILE_TYPE_STACK_WIDTH, 2.0 * row_height), egui::Layout::top_down(egui::Align::Max), |ui| {
                ui.allocate_ui_with_layout(vec2(FILE_TYPE_STACK_WIDTH, row_height), egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if icon_button(ui, ic::COPY, tr("Duplicate this output"), false, ICON).clicked() {
                        *action = Some(MapAction::Duplicate(output_index));
                    }
                    changed |= format_combo(ui, output_index, map);
                });
                ui.allocate_ui_with_layout(vec2(FILE_TYPE_STACK_WIDTH, row_height), egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if icon_button(ui, ic::X, tr("Remove this output"), false, ICON).clicked() {
                        *action = Some(MapAction::Remove(output_index));
                    }
                    changed |= bit_depth_combo(ui, output_index, map);
                });
            });
        });
    });

    // An output's handle dropped on this card moves it here
    if let Some(moved) = card.response.dnd_release_payload::<MovedOutput>() {
        *action = Some(MapAction::Move(moved.0, output_index));
    }
    if card.response.dnd_hover_payload::<MovedOutput>().is_some() {
        ui.painter().rect_stroke(card.response.rect, tk.radius, egui::Stroke::new(DROP_HIGHLIGHT_WIDTH, tk.accent), egui::StrokeKind::Outside);
    }
    changed
}

/// The file name template across the card, with the variable menu's $ inside its right end
fn name_field(ui: &mut Ui, map: &mut MapSpec, file: Option<&str>, width: f32) -> bool {
    let mut changed = false;
    let writes = match file {
        Some(file) => trf("Writes {file}", &[("file", &file)]),
        None => tr("Not exported: no source in this texture set").to_string(),
    };
    let margin = egui::Margin { left: NAME_MARGIN_LEFT, right: NAME_DOLLAR_ROOM as i8, top: NAME_MARGIN_VERTICAL, bottom: NAME_MARGIN_VERTICAL };
    let field =
        ui.add(egui::TextEdit::singleline(&mut map.file).font(egui::TextStyle::Monospace).margin(margin).align(egui::Align2::LEFT_CENTER).desired_width(width));
    changed |= field.changed();
    let field = field.on_hover_text(writes);

    let dollar_rect = egui::Rect::from_min_max(
        egui::pos2(field.rect.right() - NAME_DOLLAR_ROOM + DOLLAR_INSET, field.rect.top() + DOLLAR_INSET),
        egui::pos2(field.rect.right() - DOLLAR_INSET, field.rect.bottom() - DOLLAR_INSET),
    );
    let dollar =
        ui.put(dollar_rect, egui::Button::new(RichText::new(ic::CURRENCY_DOLLAR).size(DOLLAR_ICON_SIZE)).small()).on_hover_text(tr("Insert a variable"));
    egui::Popup::menu(&dollar).show(|ui| {
        for (token, what) in FILE_TOKENS {
            if ui.button(RichText::new(token).monospace()).on_hover_text(what).clicked() {
                map.file.push_str(token);
                changed = true;
                ui.close();
            }
        }
    });
    changed
}

/// The map's own output size, or Global: the export's size from Settings
fn size_combo(ui: &mut Ui, output_index: usize, map: &mut MapSpec) -> bool {
    let tk = Tokens::get(ui.ctx());
    let mut changed = false;
    let shown = match map.options.size {
        Some(size) => RichText::new(size.to_string()),
        None => RichText::new(tr("Global")).color(tk.text_dim),
    };
    egui::ComboBox::from_id_salt(("exp_map_size", output_index)).width(SIZE_WIDTH).selected_text(shown).show_ui(ui, |ui| {
        changed |= ui.selectable_value(&mut map.options.size, None, tr("Global")).changed();
        for size in crate::export_controls::SIZES {
            changed |= ui.selectable_value(&mut map.options.size, Some(size), size.to_string()).changed();
        }
    });
    changed
}

fn format_combo(ui: &mut Ui, output_index: usize, map: &mut MapSpec) -> bool {
    let mut changed = false;
    egui::ComboBox::from_id_salt(("exp_map_fmt", output_index)).width(FILE_TYPE_WIDTH).selected_text(map.format.extension()).show_ui(ui, |ui| {
        for format in FileFormat::ALL {
            if ui.selectable_label(map.format == format, format.extension()).clicked() {
                map.format = format;
                map.bit_depth = format.depth(map.bit_depth);
                map.options.dithering &= map.bit_depth == 8;
                changed = true;
            }
        }
    });
    changed
}

/// The bit depths the map's format holds, 8 bits also dithered
fn bit_depth_combo(ui: &mut Ui, output_index: usize, map: &mut MapSpec) -> bool {
    let mut changed = false;
    let shown = crate::export_controls::bit_depth_text(map.bit_depth, map.options.dithering);
    egui::ComboBox::from_id_salt(("exp_map_bits", output_index)).width(FILE_TYPE_WIDTH).selected_text(shown).show_ui(ui, |ui| {
        for &depth in map.format.bit_depths() {
            for dithering in if depth == 8 { &[false, true][..] } else { &[false][..] } {
                let selected = map.bit_depth == depth && map.options.dithering == *dithering;
                if ui.selectable_label(selected, crate::export_controls::bit_depth_text(depth, *dithering)).clicked() {
                    map.bit_depth = depth;
                    map.options.dithering = *dithering;
                    changed = true;
                }
            }
        }
    });
    changed
}

/// What hovering one of the output's boxes says besides the pictures
fn preview_text(map: &MapSpec, planned: Option<&PlannedOutput>, openpbr: bool) -> PreviewText {
    let details = planned.map_or_else(String::new, |planned| {
        let bits = crate::export_controls::bit_depth_text(map.bit_depth, map.options.dithering);
        format!("{} x {} · {} {bits}", planned.size[0], planned.size[1], map.format.extension())
    });
    let sources = map.channels.iter().map(|(dest, source)| (*dest, source_name(source, openpbr, true))).collect();
    PreviewText { file: planned.map(|planned| planned.file.clone()), details, sources }
}

/// A source's name: a constant with its value, a colour map with the component read when `one_component`
fn source_name(source: &SourceRef, openpbr: bool, one_component: bool) -> String {
    match source {
        SourceRef::Constant(v) => format!("{} {v:.2}", tr(source_label(source, openpbr))),
        other if one_component && other.components() == 3 => {
            format!("{} ({})", tr(source_label(other, openpbr)), RGB.get(component_of(other) as usize).copied().unwrap_or("R"))
        }
        other => tr(source_label(other, openpbr)).to_string(),
    }
}

/// A channel box: coloured by the map it reads, a map dropped on it fills it, a click opens its menu, hovering shows the file
fn channel_box(ui: &mut Ui, map: &mut MapSpec, slot: &ChannelSlot, openpbr: bool, pictures: &mut dyn FnMut() -> Preview, text: &PreviewText) -> bool {
    let tk = Tokens::get(ui.ctx());
    let source = map.channels.iter().find(|(dest, _)| slot.dests.contains(dest)).map(|(_, s)| s.clone()).unwrap_or(SourceRef::Constant(0.0));
    let name = source_name(&source, openpbr, slot.dests.len() == 1);

    let box_width = if slot.label.len() > 1 { BOX_WIDE_WIDTH } else { BOX_WIDTH };
    let (rect, response) = ui.allocate_exact_size(vec2(box_width, ui.spacing().interact_size.y), egui::Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, format!("{}: {name}", slot.label)));
    let fill = box_color(&source);
    ui.painter().rect_filled(rect, BOX_RADIUS, fill);
    // The label sits a little high, leaving the bottom to its channel's colour bar
    ui.painter().text(
        rect.center() - vec2(0.0, SLOT_BAR_HEIGHT),
        egui::Align2::CENTER_CENTER,
        slot.label,
        egui::FontId::proportional(CHIP_TEXT_SIZE),
        egui::Color32::WHITE,
    );
    let bar_bottom = rect.bottom() - SLOT_BAR_HEIGHT;
    let bar = egui::Rect::from_x_y_ranges(rect.left() + SLOT_BAR_INSET..=rect.right() - SLOT_BAR_INSET, bar_bottom - SLOT_BAR_HEIGHT..=bar_bottom);
    ui.painter().rect_filled(bar, 0.0, slot_bar_color(slot));
    if response.dnd_hover_payload::<SourceRef>().is_some() {
        ui.painter().rect_stroke(rect.expand(DROP_HIGHLIGHT_GAP), BOX_RADIUS, egui::Stroke::new(DROP_HIGHLIGHT_WIDTH, tk.accent), egui::StrokeKind::Outside);
    }

    let mut changed = false;
    if let Some(input) = response.dnd_release_payload::<SourceRef>() {
        drop_input(map, slot, &input);
        changed = true;
    }
    // A built-in template's boxes are disabled, and its files are worth a look all the same
    let response = response.on_hover_ui(|ui| map_preview::show(ui, &pictures(), text)).on_disabled_hover_ui(|ui| map_preview::show(ui, &pictures(), text));
    egui::Popup::menu(&response).show(|ui| {
        ui.label(RichText::new(&name).strong());
        // A single box reading a colour map can read any of its components
        if slot.dests.len() == 1 && source.components() == 3 {
            for (k, label) in (0u8..).zip(RGB) {
                if ui.selectable_label(component_of(&source) == k, label).clicked() {
                    drop_input(map, slot, &with_component(&source, k));
                    changed = true;
                }
            }
            ui.separator();
        }
        if let SourceRef::Constant(mut value) = source
            && ui.add(egui::Slider::new(&mut value, 0.0..=1.0).text(tr("Value"))).changed()
        {
            drop_input(map, slot, &SourceRef::Constant(value));
            changed = true;
        }
        if ui.button(tr("Clear channel")).clicked() {
            clear_slot(map, slot);
            changed = true;
            ui.close();
        }
    });
    changed
}

/// A box's colour: the same map is the same colour everywhere; a constant is its own grey
fn box_color(source: &SourceRef) -> egui::Color32 {
    if let SourceRef::Constant(value) = source {
        let grey = (value.clamp(0.0, 1.0) * BOX_CONSTANT_RANGE + BOX_CONSTANT_FLOOR) as u8;
        return egui::Color32::from_gray(grey);
    }

    // A hue from the map's name, deep and dark in perceptual terms so it isn't pastel and the white label reads on it
    let hash = source_label(&with_component(source, 0), false).bytes().fold(0u32, |h, b| h.wrapping_mul(31).wrapping_add(u32::from(b)));
    egui::ecolor::HsvaGamma { h: (hash % 360) as f32 / 360.0, s: BOX_SATURATION, v: BOX_VALUE, a: 1.0 }.into()
}

/// The strip under a box's label: white for colour, the channel's own colour, grey, or black and white for alpha
fn slot_bar_color(slot: &ChannelSlot) -> egui::Color32 {
    match slot.label {
        "R" => egui::Color32::from_rgb(230, 70, 70),
        "G" => egui::Color32::from_rgb(80, 200, 90),
        "B" => egui::Color32::from_rgb(80, 130, 240),
        "Gr" => egui::Color32::from_gray(150),
        "A" => egui::Color32::from_gray(220),
        _ => egui::Color32::WHITE,
    }
}

/// A preset in the list: its icon and name, highlighted when selected
fn preset_row(ui: &mut Ui, icon: &str, name: &str, selected: bool) -> egui::Response {
    let tk = Tokens::get(ui.ctx());
    let (rect, row) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_HEIGHT), egui::Sense::click());
    row.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, selected, name));
    if row.hovered() {
        ui.painter().rect_filled(rect, 0.0, tk.row_hover);
    }
    let color = if selected { tk.accent } else { tk.text };
    let icon_at = egui::pos2(rect.left() + ROW_INDENT / 2.0, rect.center().y);
    ui.painter().text(icon_at, egui::Align2::LEFT_CENTER, icon, egui::FontId::proportional(PRESET_ICON_SIZE), color);
    let galley = crate::widgets::elided(ui.painter(), name, PRESET_NAME_SIZE, color, rect.width() - PRESET_NAME_ROOM);
    ui.painter().galley(egui::pos2(rect.left() + PRESET_NAME_INDENT, rect.center().y - galley.size().y / 2.0), galley, color);

    row
}

/// An input map to drag onto an output channel: a chip, filled with its boxes' colour when the template reads it
fn palette_item(ui: &mut Ui, label: &str, source: &SourceRef, used: bool) {
    let tk = Tokens::get(ui.ctx());
    let galley = ui.painter().layout_no_wrap(label.to_string(), egui::FontId::proportional(CHIP_TEXT_SIZE), tk.text);
    let (rect, item) = ui.allocate_exact_size(vec2(galley.size().x + 2.0 * CHIP_PADDING, CHIP_HEIGHT), egui::Sense::drag());
    item.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
    let (fill, text) = match used {
        true => (box_color(source), egui::Color32::WHITE),
        false if item.hovered() => (tk.row_hover, tk.text),
        false => (tk.row, tk.text_dim),
    };
    ui.painter().rect_filled(rect, BOX_RADIUS, fill);
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, label, egui::FontId::proportional(CHIP_TEXT_SIZE), text);

    let item = item.on_hover_cursor(egui::CursorIcon::Grab);
    item.dnd_set_drag_payload(source.clone());
    if item.dragged()
        && let Some(at) = item.interact_pointer_pos()
    {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        egui::Area::new(item.id.with("drag")).order(egui::Order::Tooltip).fixed_pos(at + DRAG_CHIP_OFFSET).show(ui.ctx(), |ui| chip(ui, label, true));
    }
}

/// A palette group: its name, then its maps as chips that wrap
fn palette_group(ui: &mut Ui, name: &str, items: &[(&str, SourceRef)], used: &[SourceRef]) {
    let tk = Tokens::get(ui.ctx());
    ui.add_space(PALETTE_GROUP_GAP);
    if !name.is_empty() {
        ui.label(RichText::new(name).size(CHIP_TEXT_SIZE).color(tk.text_dim));
    }
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(PALETTE_CHIP_GAP, PALETTE_CHIP_GAP);
        for (label, source) in items {
            palette_item(ui, label, source, used.iter().any(|u| same_source(u, source)));
        }
    });
}

/// The Input maps column: texture set channels by lobe
fn input_maps(ui: &mut Ui, openpbr: bool, used: &[SourceRef]) {
    panel_header(ui, tr("Input maps"), |_| {});
    egui::ScrollArea::vertical().id_salt("exp_inputs").auto_shrink([false; 2]).show(ui, |ui| {
        for (group, channels) in INPUT_GROUPS {
            let items: Vec<(&str, SourceRef)> =
                channels.iter().map(|&channel| (tr(channel_name(channel, openpbr)), SourceRef::Channel { channel, component: 0 })).collect();
            palette_group(ui, tr(group), &items, used);
        }
    });
}

/// The Mesh maps column above the Converted maps (with the constants)
fn mesh_and_converted_maps(ui: &mut Ui, used: &[SourceRef]) {
    let half = ui.available_height() / 2.0;
    panel_header(ui, tr("Mesh maps"), |_| {});
    egui::ScrollArea::vertical().id_salt("exp_mesh_maps").max_height(half - ROW_HEIGHT).auto_shrink([false; 2]).show(ui, |ui| {
        let items: Vec<(&str, SourceRef)> =
            BakedMap::ALL.iter().map(|&map| (crate::i18n::baked_map_label(map), SourceRef::Baked { map, component: 0 })).collect();
        palette_group(ui, "", &items, used);
    });
    ui.separator();
    panel_header(ui, tr("Converted maps"), |_| {});
    egui::ScrollArea::vertical().id_salt("exp_converted_maps").auto_shrink([false; 2]).show(ui, |ui| {
        let items: Vec<(&str, SourceRef)> = VirtualMap::ALL.iter().map(|&map| (tr(map.label()), SourceRef::Virtual { map, component: 0 })).collect();
        palette_group(ui, "", &items, used);
        let constants =
            [(tr(n_("Black")), SourceRef::Constant(0.0)), (tr(n_("Gray 50%")), SourceRef::Constant(0.5)), (tr(n_("White")), SourceRef::Constant(1.0))];
        palette_group(ui, tr("Constants"), &constants, used);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ch(channel: Channel, component: u8) -> SourceRef {
        SourceRef::Channel { channel, component }
    }

    fn labels(map: &MapSpec) -> Vec<&'static str> {
        channel_slots(map).into_iter().map(|slot| slot.label).collect()
    }

    #[test]
    fn boxes_follow_the_channels_and_drops_fill_them() {
        let mut colour = OutputKind::Rgb.new_map();
        assert_eq!(labels(&colour), ["RGB"]);
        let rgb = channel_slots(&colour).remove(0);
        drop_input(&mut colour, &rgb, &ch(Channel::BaseColor, 0));
        assert_eq!(colour.channels, [(Dest::R, ch(Channel::BaseColor, 0)), (Dest::G, ch(Channel::BaseColor, 1)), (Dest::B, ch(Channel::BaseColor, 2))]);
        assert_eq!(labels(&colour), ["RGB"], "a colour map fills the one box");

        let mut separate = OutputKind::SeparateRgbAlpha.new_map();
        assert_eq!(labels(&separate), ["R", "G", "B", "A"]);
        let green = channel_slots(&separate).remove(1);
        drop_input(&mut separate, &green, &ch(Channel::Metallic, 0));
        assert_eq!(separate.channels[1], (Dest::G, ch(Channel::Metallic, 0)));
        clear_slot(&mut separate, &green);
        assert_eq!(separate.channels[1], (Dest::G, SourceRef::Constant(0.0)));
        let alpha = channel_slots(&separate).remove(3);
        clear_slot(&mut separate, &alpha);
        assert_eq!(separate.channels[3], (Dest::A, SourceRef::Constant(1.0)), "an empty alpha is white");

        assert_eq!(labels(&OutputKind::Gray.new_map()), ["Gr"]);
        let unreal = texturecraft_engine::formats::builtin_preset("Unreal Engine (Packed)").unwrap();
        let packed = unreal.maps.iter().find(|map| map.file.ends_with("OcclusionRoughnessMetallic")).unwrap();
        assert_eq!(labels(packed), ["R", "G", "B"], "a packed map shows its channels apart");
        for kind in OutputKind::ALL {
            ExportPreset { name: "t".into(), maps: vec![kind.new_map()] }.validate().unwrap();
        }
    }

    #[test]
    fn a_box_colour_belongs_to_its_map() {
        assert_eq!(box_color(&ch(Channel::Roughness, 0)), box_color(&ch(Channel::Roughness, 0)));
        assert_ne!(box_color(&ch(Channel::Roughness, 0)), box_color(&ch(Channel::Metallic, 0)));
        assert_eq!(box_color(&ch(Channel::BaseColor, 0)), box_color(&ch(Channel::BaseColor, 2)), "every component of a map shares its colour");
    }
}
