//! The Export textures window: it edits a draft of the project's export settings, saved by Save settings or Export and dropped by Cancel

mod bottom_action;
mod maps_action;

use bottom_action::BottomAction;
use egui::{RichText, Ui, vec2};
use egui_phosphor::regular as ic;
use maps_action::MapsAction;
use serde_json::json;
use texturecraft_engine::core::{ExportFileType, ExportOverrides, ExportSettings, NormalFormat, TextureSetExport};
use texturecraft_engine::formats::FileFormat;

use crate::TextureCraftApp;
use crate::export_controls as controls;
use crate::export_draft::ExportDraft;
#[cfg(not(target_arch = "wasm32"))]
use crate::file_dialogs::{Bound, FileDialog};
use crate::i18n::{tr, trf};
use crate::theme::{Tokens, caption, semibold};
use crate::widgets::{icon_button, icon_button_without_tooltip, primary_button};

/// The window's size, made smaller to fit a smaller app window
const EXPORT_DIALOG_SIZE: egui::Vec2 = egui::vec2(1100.0, 640.0);
/// Room kept around the window inside the app window, and the window's own padding around its body
const EXPORT_DIALOG_MARGIN: f32 = 16.0;
const EXPORT_DIALOG_PADDING: f32 = 32.0;
const EXPORT_SET_LIST_WIDTH: f32 = 180.0;
/// Room under the tabs' body for the button row
const EXPORT_FOOTER_HEIGHT: f32 = 44.0;
const OVERRIDE_PEN_SIZE: f32 = 20.0;
/// Tab strip: text size, height and the open tab's underline
const TAB_TEXT_SIZE: f32 = 12.0;
const TAB_HEIGHT: f32 = 28.0;
const TAB_UNDERLINE: f32 = 2.0;
/// Width of the Output templates tab's naming pattern combo
const NAMING_WIDTH: f32 = 140.0;
/// Bottom bar buttons are one size
const BOTTOM_BUTTON_SIZE: egui::Vec2 = egui::vec2(120.0, 28.0);
/// Height of a row in the texture set list
const SET_ROW_HEIGHT: f32 = 26.0;
const SET_ROW_PADDING: f32 = 8.0;
/// Room for the * that marks a set with overrides
const SET_ROW_MARKER: f32 = 12.0;
/// Label column of the parameter grids, and the room left for the controls
const PARAMETER_LABEL_WIDTH: f32 = 220.0;
const PARAMETER_RIGHT_MARGIN: f32 = 32.0;
const PARAMETER_MIN_FIELD_WIDTH: f32 = 240.0;
const PARAMETER_GAP: f32 = 10.0;
const BROWSE_BUTTON_WIDTH: f32 = 40.0;
/// Width of an output map's name in a texture set's Output maps
const MAP_NAME_WIDTH: f32 = 230.0;

pub fn show(app: &mut TextureCraftApp, ctx: &egui::Context) {
    // Closed by Cancel, Esc or the title bar: the unsaved edits go
    if !app.ui.windows.export {
        app.ui.export.settings = None;
        return;
    }
    if app.ui.export.settings.as_ref().is_none_or(|draft| draft.sets.len() != app.session.set_count()) {
        app.ui.export.settings = Some(ExportDraft::from_project(&app.session.doc.project, &app.ui.export.dir));
        // The window always opens on Settings
        app.ui.export.tab = 0;
    }

    let tk = Tokens::get(ctx);
    let mut open = true;
    let mut action = None;
    let presets = texturecraft_engine::formats::builtin_presets();
    // A fixed size: the tabs fill the window, so a window that followed its content would grow every frame
    let size = EXPORT_DIALOG_SIZE.min(ctx.content_rect().size() - egui::Vec2::splat(2.0 * EXPORT_DIALOG_MARGIN)).max(egui::Vec2::ZERO);
    crate::windows::dialog("export_textures", tr("Export textures"), &mut open).fixed_size(size).show(ctx, |ui| {
        ui.horizontal(|ui| {
            tab_strip(app, ui);
            if app.ui.export.tab == 0 {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(tr("Include all")).clicked()
                        && let Some(draft) = app.ui.export.settings.as_mut()
                    {
                        (0..draft.sets.len()).for_each(|ts| draft.set_exported(ts, true));
                    }
                });
            }
            if app.ui.export.tab == 1 {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(ic::INFO).size(14.0)).on_hover_text(tr("Names the input maps on this page as TextureCraft or OpenPBR calls them"));
                    let naming = |openpbr: bool| if openpbr { "OpenPBR" } else { "TextureCraft" };
                    egui::ComboBox::from_id_salt("exp_naming").width(NAMING_WIDTH).selected_text(naming(app.ui.export.openpbr_names)).show_ui(ui, |ui| {
                        for openpbr in [false, true] {
                            ui.selectable_value(&mut app.ui.export.openpbr_names, openpbr, naming(openpbr));
                        }
                    });
                    ui.label(tr("Naming pattern"));
                });
            }
        });
        ui.separator();
        let body_height = (ui.available_height() - EXPORT_FOOTER_HEIGHT).max(0.0);
        ui.allocate_ui_with_layout(vec2(ui.available_width(), body_height), egui::Layout::left_to_right(egui::Align::Min), |ui| {
            ui.set_height(body_height);
            if app.ui.export.tab == 1 {
                crate::export_templates::tab(app, ui, &presets, size.x - EXPORT_DIALOG_PADDING);
                return;
            }
            ui.vertical(|ui| export_set_list(app, ui));
            ui.separator();
            ui.vertical(|ui| match app.ui.export.tab {
                0 => export_settings_tab(app, ui, &presets),
                _ => {
                    let mut params = plan_params(app);
                    params["ts"] = json!("all");
                    match texturecraft_engine::execute(&mut app.session, "export.plan", &params) {
                        Ok(v) => {
                            egui::ScrollArea::vertical().max_height(ui.available_height()).show(ui, |ui| {
                                for f in v.as_array().into_iter().flatten() {
                                    let ts = f.get("ts").and_then(|t| t.as_u64()).unwrap_or(0) as usize;
                                    if app.ui.export.settings.as_ref().is_some_and(|draft| draft.is_exported(ts)) {
                                        ui.label(RichText::new(f.get("file").and_then(|x| x.as_str()).unwrap_or_default()).monospace());
                                    }
                                }
                            });
                        }
                        Err(e) => {
                            ui.label(RichText::new(e.to_string()).color(tk.danger));
                        }
                    }
                }
            });
        });
        ui.add_space(8.0);
        ui.separator();
        action = bottom_bar(app, ui);
    });

    match action {
        Some(BottomAction::Save) => {
            save(app);
        }
        Some(BottomAction::Export) => {
            if save(app) && export_sets(app) {
                app.ui.windows.export = false;
            }
        }
        Some(BottomAction::Cancel) => app.ui.windows.export = false,
        None => {}
    }
    if !open {
        app.ui.windows.export = false;
    }
}

/// The tabs: names in capitals, the open one underlined
fn tab_strip(app: &mut TextureCraftApp, ui: &mut Ui) {
    let tk = Tokens::get(ui.ctx());
    for (index, name) in [tr("Settings"), tr("Output templates"), tr("List of exports")].into_iter().enumerate() {
        let open = app.ui.export.tab == index as u8;
        let text = RichText::new(name.to_uppercase()).font(semibold(TAB_TEXT_SIZE)).color(if open { tk.text_strong } else { tk.text_dim });
        let tab = ui.add(egui::Button::new(text).frame(false).min_size(vec2(0.0, TAB_HEIGHT)));
        if open {
            let bottom = tab.rect.bottom();
            ui.painter().line_segment([egui::pos2(tab.rect.left(), bottom), egui::pos2(tab.rect.right(), bottom)], egui::Stroke::new(TAB_UNDERLINE, tk.accent));
        }
        if tab.clicked() {
            app.ui.export.tab = index as u8;
        }
    }
}

/// Reset all, the count of textures, Cancel, Save settings and Export
fn bottom_bar(app: &mut TextureCraftApp, ui: &mut Ui) -> Option<BottomAction> {
    let tk = Tokens::get(ui.ctx());
    let mut action = None;
    let count = exported_count(app);
    // On Output templates, a template without outputs can't be saved into the settings or exported
    let empty_template = app.ui.export.tab == 1
        && app
            .ui
            .export
            .draft
            .as_ref()
            .map(|draft| draft.maps.is_empty())
            .or_else(|| app.session.export_preset(&app.ui.export.preset).map(|preset| preset.maps.is_empty()))
            .unwrap_or(false);
    ui.horizontal(|ui| {
        if app.ui.export.tab == 0
            && ui.add(egui::Button::new(tr("Reset all")).min_size(BOTTOM_BUTTON_SIZE)).clicked()
            && let Some(draft) = app.ui.export.settings.as_mut()
        {
            draft.reset(&app.ui.export.dir);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.add_enabled(count > 0 && !empty_template, primary_button(ui, tr("Export")).min_size(BOTTOM_BUTTON_SIZE)).clicked() {
                action = Some(BottomAction::Export);
            }
            if ui.add_enabled(!empty_template, egui::Button::new(tr("Save settings")).min_size(BOTTOM_BUTTON_SIZE)).clicked() {
                action = Some(BottomAction::Save);
            }
            if ui.add(egui::Button::new(tr("Cancel")).min_size(BOTTOM_BUTTON_SIZE)).clicked() {
                action = Some(BottomAction::Cancel);
            }
            ui.label(RichText::new(trf("{n} textures selected for export", &[("n", &count)])).color(tk.text_dim));
        });
    });

    action
}

/// The files the draft would write
fn exported_count(app: &mut TextureCraftApp) -> usize {
    let params = plan_params(app);
    texturecraft_engine::execute(&mut app.session, "export.plan", &params).ok().and_then(|plan| plan.as_array().map(Vec::len)).unwrap_or(0)
}

/// Write the draft into the project; false when it was refused (the error is shown)
fn save(app: &mut TextureCraftApp) -> bool {
    let Some(draft) = app.ui.export.settings.clone() else { return false };

    // A new project starts in the last folder used
    app.ui.export.dir.clone_from(&draft.settings.dir);
    app.run("export.settings.set", draft.params()).is_some()
}

/// Export with the saved settings; false when a command failed (its error is already shown)
#[cfg(not(target_arch = "wasm32"))]
fn export_sets(app: &mut TextureCraftApp) -> bool {
    let Some(result) = app.run("file.exportTextures", json!({})) else { return false };

    let written = result.get("files").and_then(|f| f.as_array()).map(Vec::len).unwrap_or(0);
    app.toast(trf("Exported {n} textures to {folder}", &[("n", &written), ("folder", &app.ui.export.dir)]), false);
    true
}

/// In the browser the textures download (one zip for several files)
#[cfg(target_arch = "wasm32")]
fn export_sets(app: &mut TextureCraftApp) -> bool {
    crate::web_files::export_textures(app);
    true
}

/// export.plan parameters for the draft (the project's saved settings when the window is closed)
pub(crate) fn plan_params(app: &TextureCraftApp) -> serde_json::Value {
    app.ui.export.settings.as_ref().map_or_else(|| json!({}), ExportDraft::params)
}

/// The draft's global parameters as explicit export options, for previewing a template that isn't saved yet
pub(crate) fn global_params(app: &TextureCraftApp) -> serde_json::Value {
    let settings = app.ui.export.settings.as_ref().map(|draft| draft.settings.clone()).or_else(|| app.session.doc.project.export.clone()).unwrap_or_default();
    let parameters = &settings.parameters;
    let mut p = json!({"preset": parameters.preset, "padding": parameters.padding, "dilation": parameters.dilation});
    if !parameters.file_type.format.is_empty() {
        p["format"] = json!(parameters.file_type.format);
    }
    if parameters.file_type.bit_depth > 0 {
        p["bitDepth"] = json!(parameters.file_type.bit_depth);
    }
    if parameters.file_type.dithering {
        p["dithering"] = json!(true);
    }
    if parameters.size > 0 {
        p["size"] = json!(parameters.size);
    }
    if !settings.normal_format.is_empty() {
        p["normalFormat"] = json!(settings.normal_format);
    }
    if !settings.color_space.is_empty() {
        p["colorSpace"] = json!(settings.color_space);
    }

    p
}

fn export_settings_tab(app: &mut TextureCraftApp, ui: &mut Ui, presets: &[texturecraft_engine::formats::ExportPreset]) {
    let builtins: Vec<String> = presets.iter().map(|p| p.name.clone()).collect();
    let user_presets: Vec<String> = app.session.export_presets.list.iter().map(|p| p.name.clone()).collect();
    let selected = app.ui.export.settings.as_ref().and_then(|draft| draft.selected);
    match selected {
        Some(ts) => {
            let set_size = app.session.doc.project.texture_sets.get(ts).map(|set| set.size());
            let maps = set_template_maps(app, ts);
            if let Some(draft) = app.ui.export.settings.as_mut() {
                egui::ScrollArea::vertical().id_salt("exp_set_settings").auto_shrink([false; 2]).show(ui, |ui| {
                    set_settings(ui, draft, ts, set_size, &builtins, &user_presets);
                    ui.separator();
                    set_output_maps(ui, draft, ts, &maps);
                });
            }
        }
        None => {
            egui::ScrollArea::vertical()
                .id_salt("exp_global_settings")
                .auto_shrink([false; 2])
                .show(ui, |ui| global_settings(app, ui, &builtins, &user_presets));
        }
    }
}

/// Global settings: the parameters every texture set starts from
fn global_settings(app: &mut TextureCraftApp, ui: &mut Ui, builtins: &[String], user_presets: &[String]) {
    let project_normal = app.session.doc.project.normal_format;
    // The preset's own convention, so picking an engine preset shows what it writes
    let preset_normal = app
        .ui
        .export
        .settings
        .as_ref()
        .and_then(|draft| app.session.export_preset(&draft.settings.parameters.preset))
        .and_then(|preset| preset.normal_format(project_normal));
    let default_dir = app.ui.export.dir.clone();
    let Some(draft) = app.ui.export.settings.as_mut() else { return };

    let mut pick_dir = false;
    let field = field_width(ui);
    ui.horizontal(|ui| {
        ui.label(caption(tr("General export parameters")));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if icon_button(ui, ic::ARROW_COUNTER_CLOCKWISE, tr("Reset the general export parameters"), false, OVERRIDE_PEN_SIZE).clicked() {
                draft.settings = ExportSettings { dir: default_dir, ..Default::default() };
            }
        });
    });
    let settings = &mut draft.settings;
    ui.add_space(PARAMETER_GAP);
    egui::Grid::new("exp_grid").num_columns(2).min_col_width(PARAMETER_LABEL_WIDTH).spacing(vec2(0.0, PARAMETER_GAP)).show(ui, |ui| {
        // The browser downloads the files instead
        #[cfg(not(target_arch = "wasm32"))]
        {
            ui.label(tr("Output directory"));
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut settings.dir).desired_width(field - BROWSE_BUTTON_WIDTH));
                pick_dir = ui.button("...").clicked();
            });
            ui.end_row();
        }
        let parameters = &mut settings.parameters;
        ui.label(tr("Output template"));
        controls::template_combo(ui, "exp_preset", &mut parameters.preset, builtins, user_presets, field);
        ui.end_row();
        ui.label(tr("File type"));
        controls::file_type_combos(ui, "exp_fmt", &mut parameters.file_type, field);
        ui.end_row();
        ui.label(tr("Size"));
        controls::size_combo(ui, "exp_size", &mut parameters.size, None, field);
        ui.end_row();
        ui.label(tr("Padding"));
        controls::padding_controls(ui, "exp_pad", &mut parameters.padding, &mut parameters.dilation, field);
        ui.end_row();
        ui.label(tr("Normal map format"));
        let preset_format = match preset_normal {
            Some(NormalFormat::OpenGl) => trf("From template / project ({format})", &[("format", &"OpenGL")]),
            Some(NormalFormat::DirectX) => trf("From template / project ({format})", &[("format", &"DirectX")]),
            None => tr("From template / project").to_string(),
        };
        let shown = match settings.normal_format.as_str() {
            "openGl" => "OpenGL".to_string(),
            "directX" => "DirectX".to_string(),
            _ => preset_format.clone(),
        };
        egui::ComboBox::from_id_salt("exp_nf").width(field).selected_text(shown).show_ui(ui, |ui| {
            for (value, label) in [("", preset_format), ("openGl", "OpenGL".to_string()), ("directX", "DirectX".to_string())] {
                ui.selectable_value(&mut settings.normal_format, value.to_string(), label);
            }
        });
        ui.end_row();
        ui.label(tr("Color space"));
        let shown = if settings.color_space.is_empty() { tr("Default") } else { settings.color_space.as_str() };
        egui::ComboBox::from_id_salt("exp_cs").width(field).selected_text(shown).show_ui(ui, |ui| {
            ui.selectable_value(&mut settings.color_space, String::new(), tr("Default"));
            for name in ["sRGB", "Linear Rec.709", "ACEScg", "ACES2065-1"] {
                ui.selectable_value(&mut settings.color_space, name.to_string(), name);
            }
        });
        ui.end_row();
        export_note(ui, tr("Color maps only: normal, roughness, metallic and other data maps stay linear"));
    });
    ui.add_space(4.0);
    ui.checkbox(&mut settings.usd, tr("Export USD asset"));
    ui.checkbox(&mut settings.shader_parameters, tr("Export shader parameters"));

    #[cfg(not(target_arch = "wasm32"))]
    if pick_dir {
        let dir = settings.dir.clone();
        app.ask_file(FileDialog::folder().directory(dir), Bound::App, |app, paths| {
            if let (Some(dir), Some(draft)) = (paths.first(), app.ui.export.settings.as_mut()) {
                draft.settings.dir = dir.display().to_string();
            }
        });
    }
}

/// A texture set's parameters: each shows the global value until its pen overrides it for this set
fn set_settings(ui: &mut Ui, draft: &mut ExportDraft, ts: usize, set_size: Option<(u32, u32)>, builtins: &[String], user_presets: &[String]) {
    let global = draft.settings.parameters.clone();
    let Some(overrides) = draft.sets.get_mut(ts).map(|set| &mut set.overrides) else { return };

    ui.horizontal(|ui| {
        ui.label(caption(tr("General export parameters")));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.menu_button(ic::LIST, |ui| {
                if ui.button(tr("Enable override for all items")).clicked() {
                    *overrides = ExportOverrides {
                        preset: Some(global.preset.clone()),
                        file_type: Some(global.file_type.clone()),
                        size: Some(global.size),
                        padding: Some((global.padding.clone(), global.dilation)),
                    };
                }
                if ui.button(tr("Disable override for all items")).clicked() {
                    *overrides = ExportOverrides::default();
                }
            });
        });
    });
    let tk = Tokens::get(ui.ctx());
    let field = field_width(ui) - OVERRIDE_PEN_SIZE - PARAMETER_GAP;
    let inherited = |ui: &mut Ui, text: String| ui.label(RichText::new(text).color(tk.text_dim));
    ui.add_space(PARAMETER_GAP);
    egui::Grid::new(("exp_set_grid", ts)).num_columns(3).spacing(vec2(PARAMETER_GAP, PARAMETER_GAP)).show(ui, |ui| {
        parameter_label(ui, tr("Output template"), PARAMETER_LABEL_WIDTH - OVERRIDE_PEN_SIZE - PARAMETER_GAP);
        if override_pen(ui, overrides.preset.is_some()) {
            overrides.preset = overrides.preset.take().xor(Some(global.preset.clone()));
        }
        match overrides.preset.as_mut() {
            Some(preset) => controls::template_combo(ui, "exp_set_preset", preset, builtins, user_presets, field),
            None => {
                inherited(ui, global.preset.clone());
            }
        }
        ui.end_row();

        parameter_label(ui, tr("File type"), PARAMETER_LABEL_WIDTH - OVERRIDE_PEN_SIZE - PARAMETER_GAP);
        if override_pen(ui, overrides.file_type.is_some()) {
            overrides.file_type = overrides.file_type.take().xor(Some(global.file_type.clone()));
        }
        match overrides.file_type.as_mut() {
            Some(file_type) => controls::file_type_combos(ui, "exp_set_fmt", file_type, field),
            None => {
                inherited(ui, controls::file_type_with_depth_text(&global.file_type));
            }
        }
        ui.end_row();

        parameter_label(ui, tr("Size"), PARAMETER_LABEL_WIDTH - OVERRIDE_PEN_SIZE - PARAMETER_GAP);
        if override_pen(ui, overrides.size.is_some()) {
            overrides.size = overrides.size.take().xor(Some(global.size));
        }
        match overrides.size.as_mut() {
            Some(size) => controls::size_combo(ui, "exp_set_size", size, set_size, field),
            None => {
                inherited(ui, controls::size_text(global.size, set_size));
            }
        }
        ui.end_row();

        parameter_label(ui, tr("Padding"), PARAMETER_LABEL_WIDTH - OVERRIDE_PEN_SIZE - PARAMETER_GAP);
        if override_pen(ui, overrides.padding.is_some()) {
            overrides.padding = overrides.padding.take().xor(Some((global.padding.clone(), global.dilation)));
        }
        match overrides.padding.as_mut() {
            Some((padding, dilation)) => controls::padding_controls(ui, "exp_set_pad", padding, dilation, field),
            None => {
                inherited(ui, controls::padding_text(&global.padding, global.dilation));
            }
        }
        ui.end_row();
    });
}

/// The output maps of texture set `ts`'s template (its own or the global one): key, format and bit depth
fn set_template_maps(app: &TextureCraftApp, ts: usize) -> Vec<(String, FileFormat, u8)> {
    let Some(draft) = app.ui.export.settings.as_ref() else { return Vec::new() };

    let preset = draft.sets.get(ts).and_then(|set| set.overrides.preset.clone()).unwrap_or_else(|| draft.settings.parameters.preset.clone());
    app.session.export_preset(&preset).map(|preset| preset.maps.iter().map(|map| (map.key(), map.format, map.bit_depth)).collect()).unwrap_or_default()
}

/// A texture set's output maps: a tick writes a map, its pen gives it its own file type
fn set_output_maps(ui: &mut Ui, draft: &mut ExportDraft, ts: usize, maps: &[(String, FileFormat, u8)]) {
    let parameters = draft.sets.get(ts).map(|set| set.overrides.apply(&draft.settings.parameters)).unwrap_or_default();
    let set_type = parameters.file_type;
    let set_format = FileFormat::from_extension(&set_type.format);
    // What a map writes without its own override: the set's file type, else the template's
    let inherited_type = |format: FileFormat, bit_depth: u8| {
        let format = set_format.unwrap_or(format);
        let bit_depth = format.depth(if set_type.bit_depth > 0 { set_type.bit_depth } else { bit_depth });
        ExportFileType { format: format.extension().to_string(), bit_depth, dithering: set_type.dithering && bit_depth == 8 }
    };
    let Some(set) = draft.sets.get_mut(ts) else { return };

    ui.horizontal(|ui| {
        ui.label(caption(tr("Output maps")));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.push_id("exp_maps_menu", |ui| {
                ui.menu_button(ic::LIST, |ui| {
                    let mut action = None;
                    for (label, choice) in [
                        (tr("Check all"), MapsAction::Check),
                        (tr("Uncheck all"), MapsAction::Uncheck),
                        (tr("Invert all"), MapsAction::Invert),
                        (tr("Enable override for all items"), MapsAction::Override),
                        (tr("Disable override for all items"), MapsAction::Inherit),
                    ] {
                        if choice == MapsAction::Override {
                            ui.separator();
                        }
                        if ui.button(label).clicked() {
                            action = Some(choice);
                        }
                    }
                    let Some(action) = action else { return };

                    for (key, format, bit_depth) in maps {
                        let map = set.maps.entry(key.clone()).or_default();
                        match action {
                            MapsAction::Check => map.skip = false,
                            MapsAction::Uncheck => map.skip = true,
                            MapsAction::Invert => map.skip = !map.skip,
                            MapsAction::Override => map.file_type = Some(inherited_type(*format, *bit_depth)),
                            MapsAction::Inherit => map.file_type = None,
                        }
                    }
                });
            });
        });
    });
    let tk = Tokens::get(ui.ctx());
    egui::Grid::new(("exp_set_maps", ts)).num_columns(3).spacing(vec2(12.0, 6.0)).show(ui, |ui| {
        for (key, format, bit_depth) in maps {
            let map = set.maps.entry(key.clone()).or_default();
            ui.horizontal(|ui| {
                let mut exported = !map.skip;
                ui.checkbox(&mut exported, "");
                map.skip = !exported;
                ui.allocate_ui(vec2(MAP_NAME_WIDTH, ui.spacing().interact_size.y), |ui| {
                    ui.set_width(MAP_NAME_WIDTH);
                    ui.add(egui::Label::new(RichText::new(key).monospace()).truncate());
                });
            });
            if override_pen(ui, map.file_type.is_some()) {
                map.file_type = map.file_type.take().xor(Some(inherited_type(*format, *bit_depth)));
            }
            match map.file_type.as_mut() {
                Some(file_type) => controls::map_file_type_combos(ui, &format!("exp_map_{key}"), file_type),
                None => {
                    let file_type = inherited_type(*format, *bit_depth);
                    ui.label(RichText::new(controls::file_type_with_depth_text(&file_type)).color(tk.text_dim));
                }
            }
            ui.end_row();
        }
    });

    // Maps back at their defaults aren't kept
    set.maps.retain(|_, map| !map.is_empty());
}

/// A parameter's name, left-aligned in a column `width` wide
fn parameter_label(ui: &mut Ui, text: &str, width: f32) {
    ui.allocate_ui_with_layout(vec2(width, ui.spacing().interact_size.y), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_width(width);
        ui.label(text);
    });
}

/// Width of a parameter's controls: what's left of the panel after the label column
fn field_width(ui: &Ui) -> f32 {
    (ui.available_width() - PARAMETER_LABEL_WIDTH - PARAMETER_RIGHT_MARGIN).max(PARAMETER_MIN_FIELD_WIDTH)
}

/// The pen beside a parameter, pressed while this texture set overrides it; true when clicked
fn override_pen(ui: &mut Ui, overridden: bool) -> bool {
    icon_button_without_tooltip(ui, ic::PENCIL_SIMPLE, tr("Override for this texture set"), overridden, OVERRIDE_PEN_SIZE).clicked()
}

/// A dim one-line note under the setting in the row above
fn export_note(ui: &mut Ui, text: &str) {
    let tk = Tokens::get(ui.ctx());
    ui.label("");
    ui.label(RichText::new(text).size(11.0).color(tk.text_dim));
    ui.end_row();
}

/// A full-width row of the set list, hover and selection painted and a line under it: `tick` adds a checkbox before the name; the row's response selects it
fn set_row(ui: &mut Ui, name: &str, selected: bool, tick: Option<&mut dyn FnMut(&mut Ui)>) -> egui::Response {
    let tk = Tokens::get(ui.ctx());
    let (rect, row) = ui.allocate_exact_size(vec2(ui.available_width(), SET_ROW_HEIGHT), egui::Sense::click());
    row.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, selected, name));
    if row.hovered() {
        ui.painter().rect_filled(rect, 0.0, tk.row_hover);
    }
    if selected {
        ui.painter().rect_stroke(rect.shrink(1.0), 0.0, egui::Stroke::new(1.0, tk.accent), egui::StrokeKind::Inside);
    }
    ui.painter().line_segment([rect.left_bottom(), rect.right_bottom()], egui::Stroke::new(1.0, tk.separator));

    let mut text_left = rect.left() + SET_ROW_PADDING;
    if let Some(tick) = tick {
        let tick_rect = egui::Rect::from_min_size(egui::pos2(text_left, rect.top()), vec2(SET_ROW_HEIGHT, SET_ROW_HEIGHT));
        ui.scope_builder(egui::UiBuilder::new().max_rect(tick_rect).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| tick(ui));
        text_left += SET_ROW_HEIGHT;
    }
    let width = rect.right() - text_left - SET_ROW_PADDING * 2.0;
    let galley = crate::widgets::elided(ui.painter(), name, 13.0, tk.text_strong, width);
    ui.painter().galley(egui::pos2(text_left, rect.center().y - galley.size().y / 2.0), galley, tk.text_strong);

    row
}

/// Global settings, then the texture sets: a tick exports a set (drag across ticks to set several), its name shows its settings, and * marks its overrides
fn export_set_list(app: &mut TextureCraftApp, ui: &mut Ui) {
    let names: Vec<String> = app.session.doc.project.texture_sets.iter().map(|set| set.name.clone()).collect();
    let Some(draft) = app.ui.export.settings.as_mut() else { return };

    ui.set_width(EXPORT_SET_LIST_WIDTH);
    ui.allocate_ui_with_layout(vec2(ui.available_width(), SET_ROW_HEIGHT), egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.menu_button(ic::LIST, |ui| {
            for (label, exported) in [(tr("Check all"), Some(true)), (tr("Uncheck all"), Some(false)), (tr("Invert all"), None)] {
                if ui.button(label).clicked() {
                    for ts in 0..names.len() {
                        draft.set_exported(ts, exported.unwrap_or(!draft.is_exported(ts)));
                    }
                }
            }
        });
    });
    if set_row(ui, tr("Global settings"), draft.selected.is_none(), None).clicked() {
        draft.selected = None;
    }

    // The value a press on a tick set, given to every tick the pointer then passes over
    let drag_id = egui::Id::unique("exp_sets_drag");
    let mut drag: Option<bool> = ui.ctx().data(|data| data.get_temp(drag_id));
    let (pointer, pressed, held) = ui.input(|input| (input.pointer.interact_pos(), input.pointer.primary_pressed(), input.pointer.primary_down()));
    egui::ScrollArea::vertical().id_salt("exp_sets").auto_shrink([false; 2]).show(ui, |ui| {
        for (ts, name) in names.into_iter().enumerate() {
            let mut exported = draft.is_exported(ts);
            let marked = draft.sets.get(ts).is_some_and(TextureSetExport::overrides_anything);
            let mut tick = None;
            let row = set_row(ui, &name, draft.selected == Some(ts), Some(&mut |ui: &mut Ui| tick = Some(ui.add(egui::Checkbox::without_text(&mut exported)))));
            if marked {
                let marker = egui::Rect::from_center_size(
                    row.rect.right_center() - vec2(SET_ROW_PADDING + SET_ROW_MARKER / 2.0, 0.0),
                    vec2(SET_ROW_MARKER, SET_ROW_HEIGHT),
                );
                ui.put(marker, egui::Label::new("*").selectable(false));
            }
            if row.clicked() {
                draft.selected = Some(ts);
            }
            let Some(tick) = tick else { continue };

            let under_pointer = pointer.is_some_and(|at| tick.rect.contains(at));
            if pressed && under_pointer {
                drag = Some(!draft.is_exported(ts));
            }
            match drag {
                Some(value) if held && under_pointer => draft.set_exported(ts, value),
                // Keyboard and accessibility toggles; a pointer press already set the tick
                None if tick.changed() => draft.set_exported(ts, exported),
                _ => {}
            }
        }
    });
    ui.ctx().data_mut(|data| match drag {
        Some(value) if held => {
            data.insert_temp(drag_id, value);
        }
        _ => {
            data.remove_temp::<bool>(drag_id);
        }
    });
}
