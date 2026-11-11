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
mod moved_output;
mod output_kind;

use channel_slot::ChannelSlot;
use moved_output::MovedOutput;
use output_kind::OutputKind;

use crate::TextureCraftApp;
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
/// A channel box's side, and the greys a constant box spans
const BOX_SIZE: f32 = 34.0;
const BOX_RADIUS: f32 = 5.0;
/// Space above each palette group
const PALETTE_GROUP_GAP: f32 = 6.0;
const BOX_CONSTANT_FLOOR: f32 = 40.0;
const BOX_CONSTANT_RANGE: f32 = 160.0;
/// Width of an output's file type and bit depth combos
const FILE_TYPE_WIDTH: f32 = 96.0;
const FILE_TYPE_STACK_WIDTH: f32 = FILE_TYPE_WIDTH + ICON + 12.0;

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

fn source_label(s: &SourceRef, openpbr: bool) -> &'static str {
    match s {
        SourceRef::Channel { channel, .. } => channel_name(*channel, openpbr),
        SourceRef::Virtual { map, .. } => map.label(),
        SourceRef::Baked { map, .. } => crate::i18n::baked_map_label(*map),
        SourceRef::Constant(_) => n_("Constant"),
    }
}

/// `s` reading component `k` (only 3-component sources have more than one).
fn with_component(s: &SourceRef, k: u8) -> SourceRef {
    let k = if s.components() == 3 { k.min(2) } else { 0 };
    let mut s = s.clone();
    if let SourceRef::Channel { component, .. } | SourceRef::Virtual { component, .. } | SourceRef::Baked { component, .. } = &mut s {
        *component = k;
    }
    s
}

/// The component `s` reads (0 for a constant)
fn component_of(s: &SourceRef) -> u8 {
    match s {
        SourceRef::Channel { component, .. } | SourceRef::Virtual { component, .. } | SourceRef::Baked { component, .. } => *component,
        SourceRef::Constant(_) => 0,
    }
}

/// Same input map (any component), or the same constant.
fn same_source(a: &SourceRef, b: &SourceRef) -> bool {
    match (a, b) {
        (SourceRef::Constant(x), SourceRef::Constant(y)) => (x - y).abs() < 1e-4,
        _ => with_component(a, 0) == with_component(b, 0),
    }
}

/// The component a channel reads by default: R, G, B their own, L and A the first.
fn slot_component(d: Dest) -> u8 {
    match d {
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

/// Keep the draft on the selected user preset; a valid draft that differs from the saved preset
/// was changed elsewhere (an agent) and reloads, an invalid one keeps the user's edit.
fn sync_draft(app: &mut TextureCraftApp) {
    let e = &mut app.ui.export;
    match app.session.export_presets.list.iter().find(|p| p.name == e.preset) {
        None => e.draft = None,
        Some(saved) => {
            if e.draft.as_ref().is_none_or(|d| d.name != saved.name || (d != saved && d.validate().is_ok())) {
                e.draft = Some(saved.clone());
            }
        }
    }
}

enum ListAction {
    Select(String),
    New,
    Duplicate,
    /// From a preset's right-click menu
    DuplicateNamed(String),
    StartRename(String),
    Rename(String),
    Remove,
    RemoveNamed(String),
    Import,
    Export,
}

enum MapAction {
    Add(OutputKind),
    Duplicate(usize),
    /// From, to
    Move(usize, usize),
    Remove(usize),
}

/// The Output templates tab: presets, the selected preset's outputs, input maps, then mesh and converted maps
/// `width` comes from the window's fixed size, not the ui: an overflow grows the window, and widths measured from it would grow too
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
    column(ui, INPUTS_WIDTH, &mut |ui| input_maps(ui, openpbr));
    ui.separator();
    column(ui, MESH_WIDTH, &mut mesh_and_converted_maps);
    if let Some(a) = list_action {
        apply(app, ui.ctx(), a);
    }
}

fn preset_list(app: &mut TextureCraftApp, ui: &mut Ui, builtins: &[ExportPreset]) -> Option<ListAction> {
    let mut act = None;
    let sel = app.ui.export.preset.clone();
    let user = app.ui.export.draft.is_some();
    ui.horizontal(|ui| {
        ui.label(caption(tr("Presets")));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let tool =
                |ui: &mut Ui, enabled: bool, icon: &str, tip: &str| ui.add_enabled_ui(enabled, |ui| icon_button(ui, icon, tip, false, ICON)).inner.clicked();
            if tool(ui, true, ic::PLUS, tr("New template")) {
                act = Some(ListAction::New);
            }
            if tool(ui, user, ic::MINUS, tr("Delete the selected template")) {
                act = Some(ListAction::Remove);
            }
            if tool(ui, true, ic::COPY, tr("Duplicate the selected template")) {
                act = Some(ListAction::Duplicate);
            }
            if tool(ui, true, ic::UPLOAD_SIMPLE, tr("Export the selected template (.json)")) {
                act = Some(ListAction::Export);
            }
            if tool(ui, true, ic::DOWNLOAD_SIMPLE, tr("Import a template (.json)")) {
                act = Some(ListAction::Import);
            }
        });
    });
    ui.add_space(2.0);
    let names: Vec<String> = app.session.export_presets.list.iter().map(|p| p.name.clone()).collect();
    egui::ScrollArea::vertical().id_salt("exp_presets").auto_shrink([false; 2]).show(ui, |ui| {
        for p in builtins {
            let r = preset_row(ui, ic::FILE_LOCK, &p.name, p.name == sel).on_hover_text(tr("Built-in (read-only): duplicate it to edit"));
            if r.clicked() {
                act = Some(ListAction::Select(p.name.clone()));
            }
            r.context_menu(|ui| {
                if ui.button(tr("Duplicate")).clicked() {
                    act = Some(ListAction::DuplicateNamed(p.name.clone()));
                }
            });
        }
        for name in names {
            match app.ui.export.renaming.as_mut().filter(|_| name == sel) {
                Some(buf) => {
                    let r = ui.add(egui::TextEdit::singleline(buf).id(egui::Id::unique(RENAME_ID)).desired_width(f32::INFINITY));
                    if r.lost_focus() {
                        act = Some(if ui.input(|i| i.key_pressed(egui::Key::Escape)) { ListAction::Select(name) } else { ListAction::Rename(buf.clone()) });
                    }
                }
                None => {
                    let r = preset_row(ui, ic::FILE_TEXT, &name, name == sel).on_hover_text(tr("Double-click to rename"));
                    if r.double_clicked() {
                        act = Some(ListAction::StartRename(name.clone()));
                    } else if r.clicked() {
                        act = Some(ListAction::Select(name.clone()));
                    }
                    r.context_menu(|ui| {
                        if ui.button(tr("Rename")).clicked() {
                            act = Some(ListAction::StartRename(name.clone()));
                        }
                        if ui.button(tr("Duplicate")).clicked() {
                            act = Some(ListAction::DuplicateNamed(name.clone()));
                        }
                        if ui.button(tr("Remove")).clicked() {
                            act = Some(ListAction::RemoveNamed(name.clone()));
                        }
                    });
                }
            }
        }
    });
    act
}

fn apply(app: &mut TextureCraftApp, ctx: &egui::Context, act: ListAction) {
    app.ui.export.renaming = None;
    match act {
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
            if let Some(p) = selected(app) {
                let name = trf("{name} copy", &[("name", &p.name)]).chars().take(MAX_PRESET_NAME).collect();
                save_new(app, &ExportPreset { name, ..p });
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

/// Save `preset` as a new user preset (renamed when its name is taken) and select it.
fn save_new(app: &mut TextureCraftApp, preset: &ExportPreset) -> Option<String> {
    let v = serde_json::to_value(preset).ok()?;
    let r = app.run("export.preset.save", json!({ "preset": v, "unique": true }))?;
    let name = r.get("name").and_then(Value::as_str)?.to_string();
    app.ui.export.preset = name.clone();
    Some(name)
}

fn rename(app: &mut TextureCraftApp, name: &str) {
    let Some(mut p) = app.ui.export.draft.clone() else { return };
    if name.is_empty() || name == p.name {
        return;
    }
    let old = std::mem::replace(&mut p.name, name.to_string());
    let Ok(v) = serde_json::to_value(&p) else { return };
    if let Some(r) = app.run("export.preset.save", json!({ "preset": v, "rename": old }))
        && let Some(n) = r.get("name").and_then(Value::as_str)
    {
        app.ui.export.preset = n.to_string();
        follow_rename(app, &old, n);
    }
}

/// The Settings tab's template choice follows a renamed or removed template (`new` is the default after a removal)
fn follow_rename(app: &mut TextureCraftApp, old: &str, new: &str) {
    if let Some(draft) = app.ui.export.settings.as_mut()
        && draft.settings.parameters.preset == old
    {
        draft.settings.parameters.preset = new.to_string();
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

/// Keep the user's templates in the app storage (the web has no library folder).
pub fn save_library(app: &TextureCraftApp, storage: &mut dyn eframe::Storage) {
    #[cfg(target_arch = "wasm32")]
    eframe::set_value(storage, STORAGE_KEY, &app.session.export_presets.list);
    #[cfg(not(target_arch = "wasm32"))]
    let _ = (app, storage);
}

/// The selected template's output maps (editable for user templates).
fn outputs(app: &mut TextureCraftApp, ui: &mut Ui) {
    let tk = Tokens::get(ui.ctx());
    let (mut preset, editable) = match (&app.ui.export.draft, selected(app)) {
        (Some(d), _) => (d.clone(), true),
        (None, Some(b)) => (b, false),
        (None, None) => {
            ui.label(RichText::new(tr("Select an output template")).color(tk.text_dim));
            return;
        }
    };
    let preview = preview_names(app, &preset);
    let mut action = None;
    egui::Frame::new().fill(tk.row).inner_margin(egui::Margin::symmetric(8, 4)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(RichText::new(tr("Current preset:")).strong().color(tk.text_strong));
            ui.label(&preset.name);
            if !editable {
                ui.label(RichText::new(ic::LOCK_SIMPLE).color(tk.text_dim)).on_hover_text(tr("Built-in templates are read-only: duplicate one to edit it"));
            }
        });
    });
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new(tr("Output maps")).strong().color(tk.text_strong));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_enabled_ui(editable, |ui| {
                for kind in OutputKind::ALL.into_iter().rev() {
                    if ui.small_button(tr(kind.label())).clicked() {
                        action = Some(MapAction::Add(kind));
                    }
                }
            });
            ui.label(tr("Create:"));
        });
    });
    if let Err(e) = &preview {
        ui.label(RichText::new(trf("Not saved: {error}", &[("error", e)])).color(tk.danger).size(11.0));
    }
    let mut changed = false;
    let n = preset.maps.len();
    let exported = preview.as_ref().map(|names| names.iter().flatten().count()).unwrap_or(0);
    let card_width = ui.available_width() - SCROLL_BAR_ROOM;
    let openpbr = app.ui.export.openpbr_names;
    egui::ScrollArea::vertical().id_salt("exp_outputs").max_height(ui.available_height() - FOOTER_HEIGHT).auto_shrink([false; 2]).show(ui, |ui| {
        if n == 0 {
            ui.add_space(8.0);
            ui.label(RichText::new(tr("No outputs yet: create one above, then drag maps onto its channels.")).color(tk.text_dim));
        }
        ui.add_enabled_ui(editable, |ui| {
            for (mi, map) in preset.maps.iter_mut().enumerate() {
                let file = preview.as_ref().ok().and_then(|v| v.get(mi).cloned().flatten());
                changed |= output_card(ui, card_width, mi, map, file.as_deref(), openpbr, &mut action);
                ui.add_space(4.0);
            }
        });
    });
    ui.label(RichText::new(tr("Drag maps from the columns on the right onto an output's channels")).size(11.0).color(tk.text_dim));
    egui::Frame::new().fill(tk.row).inner_margin(egui::Margin::symmetric(8, 4)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(trf("{n} output maps will be exported", &[("n", &exported)]));
    });
    if !editable {
        return;
    }
    match action {
        Some(MapAction::Add(kind)) => preset.maps.push(kind.new_map()),
        Some(MapAction::Duplicate(i)) if i < n => {
            let copy = preset.maps[i].clone();
            preset.maps.insert(i + 1, copy);
        }
        Some(MapAction::Move(from, to)) if from < n && to < n && from != to => {
            let moved = preset.maps.remove(from);
            preset.maps.insert(to, moved);
        }
        Some(MapAction::Remove(i)) if i < n => {
            preset.maps.remove(i);
        }
        _ if !changed => return,
        _ => {}
    }
    commit(app, preset);
}

/// Keep the edited draft; save it when valid.
fn commit(app: &mut TextureCraftApp, preset: ExportPreset) {
    if preset.validate().is_ok()
        && let Ok(v) = serde_json::to_value(&preset)
    {
        app.run("export.preset.save", json!({ "preset": v }));
    }
    app.ui.export.draft = Some(preset);
}

/// The file each map produces for the current texture set with the dialog's settings (None:
/// the set has none of the map's sources), via `export.plan`.
fn preview_names(app: &mut TextureCraftApp, preset: &ExportPreset) -> Result<Vec<Option<String>>, String> {
    let mut p = crate::export_dialog::global_params(app);
    p["preset"] = serde_json::to_value(preset).map_err(|e| e.to_string())?;
    p["ts"] = json!(app.session.ts());
    let plan = texturecraft_engine::execute(&mut app.session, "export.plan", &p).map_err(|e| e.to_string())?;
    let mut names = vec![None; preset.maps.len()];
    for f in plan.as_array().into_iter().flatten() {
        let map = f.get("map").and_then(Value::as_u64).and_then(|i| usize::try_from(i).ok());
        if let (Some(slot), Some(file)) = (map.and_then(|i| names.get_mut(i)), f.get("file").and_then(Value::as_str)) {
            *slot = Some(file.to_string());
        }
    }
    Ok(names)
}

/// One output file: name template, format, bit depth, layout and its channel sources.
/// One output: drag handle, file name, its channel boxes, then file type and bit depth with duplicate and remove.
/// `width` is the card's outer width, the same for every card (a card sized from what the one above left would creep wider)
fn output_card(ui: &mut Ui, width: f32, mi: usize, map: &mut MapSpec, file: Option<&str>, openpbr: bool, action: &mut Option<MapAction>) -> bool {
    let tk = Tokens::get(ui.ctx());
    let mut changed = false;
    let card = egui::Frame::new().fill(tk.row).corner_radius(tk.radius).inner_margin(CARD_MARGIN).show(ui, |ui| {
        ui.set_width(width - 2.0 * CARD_MARGIN);
        ui.horizontal(|ui| {
            let handle = ui.add(egui::Label::new(RichText::new(ic::DOTS_SIX_VERTICAL).size(16.0).color(tk.text_dim)).sense(egui::Sense::drag()));
            handle.dnd_set_drag_payload(MovedOutput(mi));
            let handle = handle.on_hover_cursor(egui::CursorIcon::Grab);
            if handle.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            }

            // From the right: file type and bit depth, the boxes, then the file name takes what's left
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // A fixed-width stack: a top-down group here would start its rows at the card's left edge
                let row_height = ui.spacing().interact_size.y;
                ui.allocate_ui_with_layout(vec2(FILE_TYPE_STACK_WIDTH, 2.0 * row_height), egui::Layout::top_down(egui::Align::Max), |ui| {
                    ui.allocate_ui_with_layout(vec2(FILE_TYPE_STACK_WIDTH, row_height), egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if icon_button(ui, ic::COPY, tr("Duplicate this output"), false, ICON).clicked() {
                            *action = Some(MapAction::Duplicate(mi));
                        }
                        changed |= format_combo(ui, mi, map);
                    });
                    ui.allocate_ui_with_layout(vec2(FILE_TYPE_STACK_WIDTH, row_height), egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if icon_button(ui, ic::X, tr("Remove this output"), false, ICON).clicked() {
                            *action = Some(MapAction::Remove(mi));
                        }
                        changed |= bit_depth_combo(ui, mi, map);
                    });
                });
                for slot in channel_slots(map).iter().rev() {
                    changed |= channel_box(ui, map, slot, openpbr);
                }
                ui.menu_button(RichText::new(ic::CURRENCY_DOLLAR).size(13.0), |ui| {
                    for (token, what) in FILE_TOKENS {
                        if ui.button(RichText::new(token).monospace()).on_hover_text(what).clicked() {
                            map.file.push_str(token);
                            changed = true;
                            ui.close();
                        }
                    }
                })
                .response
                .on_hover_text(tr("Insert a variable"));
                let writes = match file {
                    Some(file) => trf("Writes {file}", &[("file", &file)]),
                    None => tr("Not exported: no source in this texture set").to_string(),
                };
                changed |= ui
                    .add(egui::TextEdit::singleline(&mut map.file).font(egui::TextStyle::Monospace).desired_width(ui.available_width()))
                    .on_hover_text(writes)
                    .changed();
            });
        });
    });

    // An output's handle dropped on this card moves it here
    if let Some(moved) = card.response.dnd_release_payload::<MovedOutput>() {
        *action = Some(MapAction::Move(moved.0, mi));
    }
    if card.response.dnd_hover_payload::<MovedOutput>().is_some() {
        ui.painter().rect_stroke(card.response.rect, tk.radius, egui::Stroke::new(1.5, tk.accent), egui::StrokeKind::Outside);
    }
    changed
}

fn format_combo(ui: &mut Ui, mi: usize, map: &mut MapSpec) -> bool {
    let mut changed = false;
    egui::ComboBox::from_id_salt(("exp_map_fmt", mi)).width(FILE_TYPE_WIDTH).selected_text(map.format.extension()).show_ui(ui, |ui| {
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
fn bit_depth_combo(ui: &mut Ui, mi: usize, map: &mut MapSpec) -> bool {
    let mut changed = false;
    let shown = crate::export_controls::bit_depth_text(map.bit_depth, map.options.dithering);
    egui::ComboBox::from_id_salt(("exp_map_bits", mi)).width(FILE_TYPE_WIDTH).selected_text(shown).show_ui(ui, |ui| {
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

/// A channel box: coloured by the map it reads, a map dropped on it fills it, a click opens its menu
fn channel_box(ui: &mut Ui, map: &mut MapSpec, slot: &ChannelSlot, openpbr: bool) -> bool {
    let tk = Tokens::get(ui.ctx());
    let source = map.channels.iter().find(|(dest, _)| slot.dests.contains(dest)).map(|(_, s)| s.clone()).unwrap_or(SourceRef::Constant(0.0));
    let name = match &source {
        SourceRef::Constant(v) => format!("{} {v:.2}", tr(source_label(&source, openpbr))),
        other if other.components() == 3 && slot.dests.len() == 1 => {
            format!("{} ({})", tr(source_label(other, openpbr)), RGB.get(component_of(other) as usize).copied().unwrap_or("R"))
        }
        other => tr(source_label(other, openpbr)).to_string(),
    };

    let (rect, response) = ui.allocate_exact_size(vec2(BOX_SIZE, BOX_SIZE), egui::Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, format!("{}: {name}", slot.label)));
    let fill = box_color(&source);
    ui.painter().rect_filled(rect, BOX_RADIUS, fill);
    ui.painter().text(rect.center_top() + vec2(0.0, 3.0), egui::Align2::CENTER_TOP, slot.label, egui::FontId::proportional(11.0), egui::Color32::WHITE);
    let bar = egui::Rect::from_min_max(rect.left_bottom() + vec2(5.0, -9.0), rect.right_bottom() + vec2(-5.0, -7.0));
    ui.painter().rect_filled(bar, 0.0, slot_bar_color(slot));
    if response.dnd_hover_payload::<SourceRef>().is_some() {
        ui.painter().rect_stroke(rect.expand(2.0), 3.0, egui::Stroke::new(1.5, tk.accent), egui::StrokeKind::Outside);
    }

    let mut changed = false;
    if let Some(input) = response.dnd_release_payload::<SourceRef>() {
        drop_input(map, slot, &input);
        changed = true;
    }
    let response = response.on_hover_text(&name);
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

    // A hue from the map's name, at a fixed dark saturation so the white label reads on it
    let hash = source_label(&with_component(source, 0), false).bytes().fold(0u32, |h, b| h.wrapping_mul(31).wrapping_add(u32::from(b)));
    egui::ecolor::Hsva::new((hash % 360) as f32 / 360.0, 0.55, 0.45, 1.0).into()
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
    ui.painter().text(icon_at, egui::Align2::LEFT_CENTER, icon, egui::FontId::proportional(13.0), color);
    let galley = crate::widgets::elided(ui.painter(), name, 12.5, color, rect.width() - 2.0 * ROW_INDENT);
    ui.painter().galley(egui::pos2(rect.left() + 1.5 * ROW_INDENT, rect.center().y - galley.size().y / 2.0), galley, color);

    row
}

/// An input map to drag onto an output channel: a chip
fn palette_item(ui: &mut Ui, label: &str, source: &SourceRef) {
    let item = chip(ui, label, false).interact(egui::Sense::drag());
    item.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
    let item = item.on_hover_cursor(egui::CursorIcon::Grab);
    item.dnd_set_drag_payload(source.clone());
    if item.dragged()
        && let Some(at) = item.interact_pointer_pos()
    {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        egui::Area::new(item.id.with("drag")).order(egui::Order::Tooltip).fixed_pos(at + vec2(8.0, 8.0)).show(ui.ctx(), |ui| chip(ui, label, true));
    }
}

/// A palette group: its name, then its maps as chips that wrap
fn palette_group(ui: &mut Ui, name: &str, items: &[(&str, SourceRef)]) {
    let tk = Tokens::get(ui.ctx());
    ui.add_space(PALETTE_GROUP_GAP);
    if !name.is_empty() {
        ui.label(RichText::new(name).size(11.0).color(tk.text_dim));
    }
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
        for (label, source) in items {
            palette_item(ui, label, source);
        }
    });
}

/// The Input maps column: texture set channels by lobe
fn input_maps(ui: &mut Ui, openpbr: bool) {
    panel_header(ui, tr("Input maps"), |_| {});
    egui::ScrollArea::vertical().id_salt("exp_inputs").auto_shrink([false; 2]).show(ui, |ui| {
        for (group, channels) in INPUT_GROUPS {
            let items: Vec<(&str, SourceRef)> =
                channels.iter().map(|&channel| (tr(channel_name(channel, openpbr)), SourceRef::Channel { channel, component: 0 })).collect();
            palette_group(ui, tr(group), &items);
        }
    });
}

/// The Mesh maps column above the Converted maps (with the constants)
fn mesh_and_converted_maps(ui: &mut Ui) {
    let half = ui.available_height() / 2.0;
    panel_header(ui, tr("Mesh maps"), |_| {});
    egui::ScrollArea::vertical().id_salt("exp_mesh_maps").max_height(half - ROW_HEIGHT).auto_shrink([false; 2]).show(ui, |ui| {
        let items: Vec<(&str, SourceRef)> =
            BakedMap::ALL.iter().map(|&map| (crate::i18n::baked_map_label(map), SourceRef::Baked { map, component: 0 })).collect();
        palette_group(ui, "", &items);
    });
    ui.separator();
    panel_header(ui, tr("Converted maps"), |_| {});
    egui::ScrollArea::vertical().id_salt("exp_converted_maps").auto_shrink([false; 2]).show(ui, |ui| {
        let items: Vec<(&str, SourceRef)> = VirtualMap::ALL.iter().map(|&map| (tr(map.label()), SourceRef::Virtual { map, component: 0 })).collect();
        palette_group(ui, "", &items);
        let constants =
            [(tr(n_("Black")), SourceRef::Constant(0.0)), (tr(n_("Gray 50%")), SourceRef::Constant(0.5)), (tr(n_("White")), SourceRef::Constant(1.0))];
        palette_group(ui, tr("Constants"), &constants);
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
        assert_eq!(labels(&unreal.maps[1]), ["R", "G", "B"], "a packed map shows its channels apart");
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
