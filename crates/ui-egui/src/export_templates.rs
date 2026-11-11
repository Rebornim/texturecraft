//! Export dialog › Output templates: create and edit export presets. Built-in presets are
//! read-only; the selected user preset is edited as a draft that is saved through
//! `export.preset.save` whenever it validates, so a half-typed file name never reaches disk.

use std::sync::LazyLock;

use egui::{RichText, Ui, vec2};
use egui_phosphor::regular as ic;
use serde_json::{Value, json};
use texturecraft_engine::core::{BakedMap, Channel};
use texturecraft_engine::formats::export::{FILE_TOKENS, MAX_PRESET_NAME, sanitize_file_stem};
use texturecraft_engine::formats::{Dest, ExportPreset, FileFormat, MapSpec, SourceRef, VirtualMap};

use crate::TextureCraftApp;
use crate::i18n::{n_, tr, trf};
use crate::theme::{Tokens, caption};
use crate::widgets::{chip, icon_button};

const ICON: f32 = 20.0;
/// Widths of the presets, input maps and mesh / converted maps columns; the outputs take the rest
const LIST_WIDTH: f32 = 220.0;
const INPUTS_WIDTH: f32 = 180.0;
const MESH_WIDTH: f32 = 170.0;
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

/// An output file's channel layout.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Layout {
    Rgb,
    Rgba,
    Gray,
    GrayAlpha,
}

impl Layout {
    const ALL: [Layout; 4] = [Layout::Rgb, Layout::Rgba, Layout::Gray, Layout::GrayAlpha];

    fn label(self) -> &'static str {
        match self {
            Layout::Rgb => "RGB",
            Layout::Rgba => "RGBA",
            Layout::Gray => n_("Gray"),
            Layout::GrayAlpha => n_("Gray + A"),
        }
    }

    fn dests(self) -> &'static [Dest] {
        match self {
            Layout::Rgb => &[Dest::R, Dest::G, Dest::B],
            Layout::Rgba => &[Dest::R, Dest::G, Dest::B, Dest::A],
            Layout::Gray => &[Dest::L],
            Layout::GrayAlpha => &[Dest::L, Dest::A],
        }
    }

    fn of(map: &MapSpec) -> Layout {
        let has = |d: Dest| map.channels.iter().any(|(x, _)| *x == d);
        match (has(Dest::L), has(Dest::A)) {
            (true, true) => Layout::GrayAlpha,
            (true, false) => Layout::Gray,
            (false, true) => Layout::Rgba,
            (false, false) => Layout::Rgb,
        }
    }
}

/// The input maps palette: (group, [(label, source)]).
type Palette = [(&'static str, Vec<(&'static str, SourceRef)>); 4];

static PALETTE: LazyLock<Palette> = LazyLock::new(|| {
    [
        (n_("Texture set channels"), Channel::ALL.iter().map(|&channel| (channel.label(), SourceRef::Channel { channel, component: 0 })).collect()),
        (n_("Converted maps"), VirtualMap::ALL.iter().map(|&map| (map.label(), SourceRef::Virtual { map, component: 0 })).collect()),
        (n_("Mesh maps"), BakedMap::ALL.iter().map(|&map| (crate::i18n::baked_map_label(map), SourceRef::Baked { map, component: 0 })).collect()),
        (n_("Constants"), vec![(n_("Black"), SourceRef::Constant(0.0)), (n_("Gray 50%"), SourceRef::Constant(0.5)), (n_("White"), SourceRef::Constant(1.0))]),
    ]
});

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

fn source_label(s: &SourceRef) -> &'static str {
    match s {
        SourceRef::Channel { channel, .. } => channel.label(),
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

fn is_rgb(d: Dest) -> bool {
    matches!(d, Dest::R | Dest::G | Dest::B)
}

/// Switch `map` to `layout`, keeping the sources of channels both layouts have; the color
/// source moves between R, G, B and L.
fn set_layout(map: &mut MapSpec, layout: Layout) {
    let get = |d: Dest| map.channels.iter().find(|(x, _)| *x == d).map(|(_, s)| s.clone());
    let color = get(Dest::R).or_else(|| get(Dest::L)).unwrap_or(SourceRef::Constant(0.0));
    let channels = layout
        .dests()
        .iter()
        .map(|&d| {
            let src = match d {
                Dest::A => get(d).unwrap_or(SourceRef::Constant(1.0)),
                _ => get(d).unwrap_or_else(|| with_component(&color, slot_component(d))),
            };
            (d, src)
        })
        .collect();
    map.channels = channels;
}

fn new_map(layout: Layout) -> MapSpec {
    let mut map = MapSpec { file: "$mesh_$textureSet_Output".into(), format: FileFormat::Png, bit_depth: 8, channels: Vec::new() };
    set_layout(&mut map, layout);
    map
}

/// An input map dropped on channel `dest`: a color or normal input dropped on R, G or B fills
/// all three.
fn drop_input(map: &mut MapSpec, dest: Dest, input: &SourceRef) {
    let fill_rgb = input.components() == 3 && is_rgb(dest);
    for (d, s) in &mut map.channels {
        if *d == dest || (fill_rgb && is_rgb(*d)) {
            *s = with_component(input, slot_component(*d));
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
    Add(Layout),
    Up(usize),
    Down(usize),
    Remove(usize),
}

/// The Output templates tab: presets, the selected preset's outputs, input maps, then mesh and converted maps
pub fn tab(app: &mut TextureCraftApp, ui: &mut Ui, builtins: &[ExportPreset]) {
    sync_draft(app);
    let height = ui.available_height();
    // The window has a fixed size, so sizing the outputs from it can't feed back
    let outputs_width = (ui.available_width() - LIST_WIDTH - INPUTS_WIDTH - MESH_WIDTH - 3.0 * COLUMN_GAP).max(OUTPUTS_MIN_WIDTH);
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
    column(ui, INPUTS_WIDTH, &mut input_maps);
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
    ui.add_enabled_ui(editable, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(tr("Add output")).color(tk.text_dim));
            for l in Layout::ALL {
                if ui.small_button(format!("{} {}", ic::PLUS, tr(l.label()))).clicked() {
                    action = Some(MapAction::Add(l));
                }
            }
        });
    });
    if let Err(e) = &preview {
        ui.label(RichText::new(trf("Not saved: {error}", &[("error", e)])).color(tk.danger).size(11.0));
    }
    let mut changed = false;
    let n = preset.maps.len();
    let exported = preview.as_ref().map(|names| names.iter().flatten().count()).unwrap_or(0);
    let card_width = ui.available_width() - SCROLL_BAR_ROOM;
    egui::ScrollArea::vertical().id_salt("exp_outputs").max_height(ui.available_height() - FOOTER_HEIGHT).auto_shrink([false; 2]).show(ui, |ui| {
        if n == 0 {
            ui.add_space(8.0);
            ui.label(RichText::new(tr("No outputs yet: add one above, then drag input maps onto its channels.")).color(tk.text_dim));
        }
        ui.add_enabled_ui(editable, |ui| {
            for (mi, map) in preset.maps.iter_mut().enumerate() {
                let file = preview.as_ref().ok().and_then(|v| v.get(mi).cloned().flatten());
                changed |= output_card(ui, card_width, mi, n, map, file.as_deref(), &mut action);
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
        Some(MapAction::Add(l)) => preset.maps.push(new_map(l)),
        Some(MapAction::Up(i)) if i > 0 && i < n => preset.maps.swap(i, i - 1),
        Some(MapAction::Down(i)) if i + 1 < n => preset.maps.swap(i, i + 1),
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
/// `width` is the card's outer width, the same for every card (a card sized from what the previous one left would creep wider)
fn output_card(ui: &mut Ui, width: f32, mi: usize, n: usize, map: &mut MapSpec, file: Option<&str>, action: &mut Option<MapAction>) -> bool {
    let tk = Tokens::get(ui.ctx());
    let mut changed = false;
    egui::Frame::new().fill(tk.row).corner_radius(tk.radius).inner_margin(CARD_MARGIN).show(ui, |ui| {
        ui.set_width(width - 2.0 * CARD_MARGIN);
        // From the right: the buttons, then the file name takes what's left
        ui.allocate_ui_with_layout(vec2(ui.available_width(), ui.spacing().interact_size.y), egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if icon_button(ui, ic::TRASH, tr("Remove this output"), false, ICON).clicked() {
                *action = Some(MapAction::Remove(mi));
            }
            if ui.add_enabled_ui(mi + 1 < n, |ui| icon_button(ui, ic::ARROW_DOWN, tr("Move down"), false, ICON)).inner.clicked() {
                *action = Some(MapAction::Down(mi));
            }
            if ui.add_enabled_ui(mi > 0, |ui| icon_button(ui, ic::ARROW_UP, tr("Move up"), false, ICON)).inner.clicked() {
                *action = Some(MapAction::Up(mi));
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
            changed |= ui
                .add(egui::TextEdit::singleline(&mut map.file).font(egui::TextStyle::Monospace).desired_width(ui.available_width()))
                .on_hover_text(tr("File name without extension; variables insert names"))
                .changed();
        });
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt(("exp_map_fmt", mi)).width(56.0).selected_text(map.format.extension()).show_ui(ui, |ui| {
                for f in FileFormat::ALL {
                    if ui.selectable_value(&mut map.format, f, f.extension()).changed() {
                        map.bit_depth = f.depth(map.bit_depth);
                        changed = true;
                    }
                }
            });
            egui::ComboBox::from_id_salt(("exp_map_bits", mi)).width(64.0).selected_text(trf("{n} bits", &[("n", &map.bit_depth)])).show_ui(ui, |ui| {
                for &d in map.format.bit_depths() {
                    changed |= ui.selectable_value(&mut map.bit_depth, d, trf("{n} bits", &[("n", &d)])).changed();
                }
            });
            let layout = Layout::of(map);
            egui::ComboBox::from_id_salt(("exp_map_layout", mi)).width(72.0).selected_text(tr(layout.label())).show_ui(ui, |ui| {
                for l in Layout::ALL {
                    if ui.selectable_label(l == layout, tr(l.label())).clicked() && l != layout {
                        set_layout(map, l);
                        changed = true;
                    }
                }
            });
            let (text, color) = match file {
                Some(f) => (format!("{} {f}", ic::ARROW_RIGHT), tk.text_dim),
                None => (tr("Not exported: no source in this texture set").to_string(), tk.text_dim),
            };
            ui.add(egui::Label::new(RichText::new(text).size(11.0).color(color)).truncate());
        });
        changed |= channel_slots(ui, mi, map);
    });
    changed
}

/// A source picker per output channel that also takes input maps dropped from the palette.
fn channel_slots(ui: &mut Ui, mi: usize, map: &mut MapSpec) -> bool {
    let tk = Tokens::get(ui.ctx());
    let mut changed = false;
    let mut dropped = None;
    for (k, (dest, src)) in map.channels.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            let dest_name = match *dest {
                Dest::L => tr("Gray"),
                Dest::A => "A",
                d => RGB.get(slot_component(d) as usize).copied().unwrap_or("R"),
            };
            ui.add_sized(vec2(30.0, 18.0), egui::Label::new(RichText::new(dest_name).size(11.0).color(tk.text_dim)));
            let combo = egui::ComboBox::from_id_salt(("exp_slot", mi, k)).width(170.0).selected_text(tr(source_label(src))).show_ui(ui, |ui| {
                for (group, inputs) in PALETTE.iter() {
                    ui.label(caption(tr(group)));
                    for (label, s) in inputs {
                        if ui.selectable_label(same_source(src, s), tr(label)).clicked() {
                            *src = with_component(s, slot_component(*dest));
                            changed = true;
                        }
                    }
                }
            });
            let r = &combo.response;
            if r.dnd_hover_payload::<SourceRef>().is_some() {
                ui.painter().rect_stroke(r.rect.expand(2.0), 3.0, egui::Stroke::new(1.5, tk.accent), egui::StrokeKind::Outside);
            }
            if let Some(input) = r.dnd_release_payload::<SourceRef>() {
                dropped = Some((*dest, (*input).clone()));
            }
            let three = src.components() == 3;
            match src {
                SourceRef::Constant(v) => {
                    changed |= ui.add(egui::DragValue::new(v).range(0.0..=1.0).speed(0.005).fixed_decimals(2)).changed();
                }
                SourceRef::Channel { component, .. } | SourceRef::Virtual { component, .. } | SourceRef::Baked { component, .. } if three => {
                    let shown = RGB.get(*component as usize).copied().unwrap_or("R");
                    egui::ComboBox::from_id_salt(("exp_slot_c", mi, k)).width(36.0).selected_text(shown).show_ui(ui, |ui| {
                        for (i, l) in (0u8..).zip(RGB) {
                            changed |= ui.selectable_value(component, i, l).changed();
                        }
                    });
                }
                _ => {}
            }
        });
    }
    if let Some((dest, input)) = dropped {
        drop_input(map, dest, &input);
        changed = true;
    }
    changed
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

/// An input map to drag onto an output channel: a full-width row
fn palette_item(ui: &mut Ui, label: &str, source: &SourceRef, color: egui::Color32) {
    let tk = Tokens::get(ui.ctx());
    let (rect, item) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_HEIGHT), egui::Sense::drag());
    item.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
    let item = item.on_hover_cursor(egui::CursorIcon::Grab);
    ui.painter().rect_filled(rect.shrink2(vec2(0.0, 1.0)), 0.0, if item.hovered() { tk.row_hover } else { tk.row });
    let galley = crate::widgets::elided(ui.painter(), label, 12.0, color, rect.width() - 2.0 * ROW_INDENT);
    ui.painter().galley(egui::pos2(rect.left() + ROW_INDENT, rect.center().y - galley.size().y / 2.0), galley, color);

    item.dnd_set_drag_payload(source.clone());
    if item.dragged()
        && let Some(at) = item.interact_pointer_pos()
    {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        egui::Area::new(item.id.with("drag")).order(egui::Order::Tooltip).fixed_pos(at + vec2(8.0, 8.0)).show(ui.ctx(), |ui| chip(ui, label, true));
    }
}

/// A group name in the palettes
fn palette_group(ui: &mut Ui, name: &str) {
    let tk = Tokens::get(ui.ctx());
    ui.add_space(2.0);
    ui.label(RichText::new(name).italics().size(12.0).color(tk.text));
}

/// The Input maps column: texture set channels by lobe
fn input_maps(ui: &mut Ui) {
    let tk = Tokens::get(ui.ctx());
    ui.label(caption(tr("Input maps")));
    egui::ScrollArea::vertical().id_salt("exp_inputs").auto_shrink([false; 2]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        for (group, channels) in INPUT_GROUPS {
            palette_group(ui, tr(group));
            for &channel in channels {
                palette_item(ui, tr(channel.label()), &SourceRef::Channel { channel, component: 0 }, tk.text);
            }
        }
    });
}

/// The Mesh maps column above the Converted maps (with the constants)
fn mesh_and_converted_maps(ui: &mut Ui) {
    let tk = Tokens::get(ui.ctx());
    let half = ui.available_height() / 2.0;
    ui.label(caption(tr("Mesh maps")));
    egui::ScrollArea::vertical().id_salt("exp_mesh_maps").max_height(half - ROW_HEIGHT).auto_shrink([false; 2]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        for &map in BakedMap::ALL.iter() {
            palette_item(ui, crate::i18n::baked_map_label(map), &SourceRef::Baked { map, component: 0 }, tk.text);
        }
    });
    ui.separator();
    ui.label(caption(tr("Converted maps")));
    egui::ScrollArea::vertical().id_salt("exp_converted_maps").auto_shrink([false; 2]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        for &map in VirtualMap::ALL.iter() {
            palette_item(ui, tr(map.label()), &SourceRef::Virtual { map, component: 0 }, tk.warning);
        }
        palette_group(ui, tr("Constants"));
        for (label, value) in [(n_("Black"), 0.0), (n_("Gray 50%"), 0.5), (n_("White"), 1.0)] {
            palette_item(ui, tr(label), &SourceRef::Constant(value), tk.text);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ch(channel: Channel, component: u8) -> SourceRef {
        SourceRef::Channel { channel, component }
    }

    #[test]
    fn layouts_keep_sources_and_drops_fill_rgb() {
        let mut map = new_map(Layout::Rgb);
        drop_input(&mut map, Dest::G, &ch(Channel::BaseColor, 0));
        assert_eq!(map.channels, vec![(Dest::R, ch(Channel::BaseColor, 0)), (Dest::G, ch(Channel::BaseColor, 1)), (Dest::B, ch(Channel::BaseColor, 2))]);
        drop_input(&mut map, Dest::B, &ch(Channel::Metallic, 0));
        assert_eq!(map.channels.get(2), Some(&(Dest::B, ch(Channel::Metallic, 0))), "a gray input fills only its channel");
        set_layout(&mut map, Layout::Rgba);
        assert_eq!(Layout::of(&map), Layout::Rgba);
        assert_eq!(map.channels.get(3), Some(&(Dest::A, SourceRef::Constant(1.0))));
        set_layout(&mut map, Layout::GrayAlpha);
        assert_eq!(map.channels, vec![(Dest::L, ch(Channel::BaseColor, 0)), (Dest::A, SourceRef::Constant(1.0))]);
        set_layout(&mut map, Layout::Rgb);
        assert_eq!(map.channels.get(1), Some(&(Dest::G, ch(Channel::BaseColor, 1))), "gray to RGB spreads a color source");
        for l in Layout::ALL {
            let m = new_map(l);
            assert_eq!(Layout::of(&m), l);
            ExportPreset { name: "t".into(), maps: vec![m] }.validate().unwrap();
        }
        assert!(same_source(&ch(Channel::Normal, 2), &ch(Channel::Normal, 0)));
        assert!(!same_source(&SourceRef::Constant(0.0), &SourceRef::Constant(1.0)));
    }
}
