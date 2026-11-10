//! The Export textures window: it edits a draft of the project's export settings, saved by Save settings or Export and dropped by Cancel

use egui::{RichText, Ui, vec2};
use serde_json::json;
use texturecraft_engine::core::NormalFormat;

use crate::TextureCraftApp;
use crate::export_draft::ExportDraft;
#[cfg(not(target_arch = "wasm32"))]
use crate::file_dialogs::{Bound, FileDialog};
use crate::i18n::{tr, trf};
use crate::theme::{Tokens, caption};
use crate::widgets::primary_button;

/// Width of the window and of its texture set column
const EXPORT_DIALOG_WIDTH: f32 = 820.0;
const EXPORT_SET_LIST_WIDTH: f32 = 180.0;
const EXPORT_BODY_HEIGHT: f32 = 360.0;
/// Its title bar, tabs, button row and margins, kept on screen in a short app window
const EXPORT_DIALOG_CHROME_HEIGHT: f32 = 160.0;
const EXPORT_BODY_MIN_HEIGHT: f32 = 120.0;

pub fn show(app: &mut TextureCraftApp, ctx: &egui::Context) {
    // Closed by Cancel, Esc or the title bar: the unsaved edits go
    if !app.ui.windows.export {
        app.ui.export.settings = None;
        return;
    }
    if app.ui.export.settings.as_ref().is_none_or(|draft| draft.sets.len() != app.session.set_count()) {
        app.ui.export.settings = Some(ExportDraft::from_project(&app.session.doc.project, &app.ui.export.dir));
    }

    let tk = Tokens::get(ctx);
    let mut open = true;
    let mut action = None;
    let presets = texturecraft_engine::formats::builtin_presets();
    let body_height = (ctx.content_rect().height() - EXPORT_DIALOG_CHROME_HEIGHT).clamp(EXPORT_BODY_MIN_HEIGHT, EXPORT_BODY_HEIGHT);
    crate::windows::dialog("export_textures", tr("Export textures"), &mut open).min_width(EXPORT_DIALOG_WIDTH).max_width(EXPORT_DIALOG_WIDTH).show(ctx, |ui| {
        ui.horizontal(|ui| {
            for (i, t) in [tr("Settings"), tr("Output templates"), tr("List of exports")].iter().enumerate() {
                if ui.selectable_label(app.ui.export.tab == i as u8, caption(t)).clicked() {
                    app.ui.export.tab = i as u8;
                }
            }
        });
        ui.separator();
        ui.allocate_ui_with_layout(vec2(ui.available_width(), body_height), egui::Layout::left_to_right(egui::Align::Min), |ui| {
            ui.set_height(body_height);
            if app.ui.export.tab == 1 {
                crate::export_templates::tab(app, ui, &presets);
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

/// What a bottom bar button asks for
enum BottomAction {
    Save,
    Export,
    Cancel,
}

/// Reset all, the count of textures, Cancel, Save settings and Export
fn bottom_bar(app: &mut TextureCraftApp, ui: &mut Ui) -> Option<BottomAction> {
    let tk = Tokens::get(ui.ctx());
    let mut action = None;
    let count = exported_count(app);
    ui.horizontal(|ui| {
        if ui.button(tr("Reset all")).clicked()
            && let Some(draft) = app.ui.export.settings.as_mut()
        {
            draft.reset(&app.ui.export.dir);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.add_enabled(count > 0, primary_button(ui, tr("Export"))).clicked() {
                action = Some(BottomAction::Export);
            }
            if ui.button(tr("Save settings")).clicked() {
                action = Some(BottomAction::Save);
            }
            if ui.button(tr("Cancel")).clicked() {
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
    if !parameters.format.is_empty() {
        p["format"] = json!(parameters.format);
    }
    if parameters.bit_depth > 0 {
        p["bitDepth"] = json!(parameters.bit_depth);
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
    let project_normal = app.session.doc.project.normal_format;
    // The preset's own convention, so picking an engine preset shows what it writes
    let preset_normal = app
        .ui
        .export
        .settings
        .as_ref()
        .and_then(|draft| app.session.export_preset(&draft.settings.parameters.preset))
        .and_then(|preset| preset.normal_format(project_normal));
    let user_presets: Vec<String> = app.session.export_presets.list.iter().map(|p| p.name.clone()).collect();
    let Some(draft) = app.ui.export.settings.as_mut() else { return };

    let mut pick_dir = false;
    let settings = &mut draft.settings;
    ui.label(caption(tr("General export parameters")));
    egui::Grid::new("exp_grid").num_columns(2).spacing(vec2(12.0, 8.0)).show(ui, |ui| {
        // The browser downloads the files instead
        #[cfg(not(target_arch = "wasm32"))]
        {
            ui.label(tr("Output directory"));
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut settings.dir).desired_width(360.0));
                pick_dir = ui.button("...").clicked();
            });
            ui.end_row();
        }
        let parameters = &mut settings.parameters;
        ui.label(tr("Output template"));
        egui::ComboBox::from_id_salt("exp_preset").width(360.0).selected_text(parameters.preset.clone()).show_ui(ui, |ui| {
            for p in presets {
                ui.selectable_value(&mut parameters.preset, p.name.clone(), &p.name);
            }
            if !user_presets.is_empty() {
                ui.separator();
            }
            for name in &user_presets {
                ui.selectable_value(&mut parameters.preset, name.clone(), name);
            }
        });
        ui.end_row();
        ui.label(tr("File type"));
        ui.horizontal(|ui| {
            let label = if parameters.format.is_empty() { tr("From template").to_string() } else { parameters.format.clone() };
            egui::ComboBox::from_id_salt("exp_fmt").selected_text(label).show_ui(ui, |ui| {
                ui.selectable_value(&mut parameters.format, String::new(), tr("From template"));
                for f in ["png", "jpeg", "tga", "tiff", "bmp", "webp", "exr", "hdr"] {
                    ui.selectable_value(&mut parameters.format, f.to_string(), f);
                }
            });
            let bit_depth = if parameters.bit_depth == 0 { tr("Default").to_string() } else { trf("{n} bits", &[("n", &parameters.bit_depth)]) };
            egui::ComboBox::from_id_salt("exp_bits").selected_text(bit_depth).show_ui(ui, |ui| {
                for (v, l) in [
                    (0u8, tr("Default").to_string()),
                    (8, trf("{n} bits", &[("n", &8)])),
                    (16, trf("{n} bits", &[("n", &16)])),
                    (32, tr("32 bits (float)").to_string()),
                ] {
                    ui.selectable_value(&mut parameters.bit_depth, v, l);
                }
            });
        });
        ui.end_row();
        ui.label(tr("Size"));
        let size = if parameters.size == 0 { tr("Based on each texture set's size").to_string() } else { format!("{0} x {0}", parameters.size) };
        egui::ComboBox::from_id_salt("exp_size").width(360.0).selected_text(size).show_ui(ui, |ui| {
            ui.selectable_value(&mut parameters.size, 0, tr("Based on each texture set's size"));
            for r in [256u32, 512, 1024, 2048, 4096, 8192] {
                ui.selectable_value(&mut parameters.size, r, format!("{r} x {r}"));
            }
        });
        ui.end_row();
        ui.label(tr("Padding"));
        ui.horizontal(|ui| {
            let label = match parameters.padding.as_str() {
                "none" => tr("No padding (passthrough)"),
                "dilation" => tr("Dilation + transparent"),
                "dilationDefault" => tr("Dilation + default background color"),
                _ => tr("Dilation infinite"),
            };
            egui::ComboBox::from_id_salt("exp_pad").width(260.0).selected_text(label).show_ui(ui, |ui| {
                for (v, l) in [
                    ("infinite", tr("Dilation infinite")),
                    ("dilation", tr("Dilation + transparent")),
                    ("dilationDefault", tr("Dilation + default background color")),
                    ("none", tr("No padding (passthrough)")),
                ] {
                    ui.selectable_value(&mut parameters.padding, v.to_string(), l);
                }
            });
            if parameters.padding.starts_with("dilation") {
                ui.add(egui::DragValue::new(&mut parameters.dilation).range(0..=256).suffix(" px"));
            }
        });
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
        egui::ComboBox::from_id_salt("exp_nf").selected_text(shown).show_ui(ui, |ui| {
            for (value, label) in [("", preset_format), ("openGl", "OpenGL".to_string()), ("directX", "DirectX".to_string())] {
                ui.selectable_value(&mut settings.normal_format, value.to_string(), label);
            }
        });
        ui.end_row();
        ui.label(tr("Color space"));
        let shown = if settings.color_space.is_empty() { tr("Default") } else { settings.color_space.as_str() };
        egui::ComboBox::from_id_salt("exp_cs").selected_text(shown).show_ui(ui, |ui| {
            ui.selectable_value(&mut settings.color_space, String::new(), tr("Default"));
            for name in ["sRGB", "Linear Rec.709", "ACEScg", "ACES2065-1"] {
                ui.selectable_value(&mut settings.color_space, name.to_string(), name);
            }
        });
        ui.end_row();
        export_note(ui, tr("Color maps only: normal, roughness, metallic and other data maps stay linear"));
    });

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

/// A dim one-line note under the setting in the row above
fn export_note(ui: &mut Ui, text: &str) {
    let tk = Tokens::get(ui.ctx());
    ui.label("");
    ui.label(RichText::new(text).size(11.0).color(tk.text_dim));
    ui.end_row();
}

/// The texture sets to export: a scrolling checkbox list with select all / none
fn export_set_list(app: &mut TextureCraftApp, ui: &mut Ui) {
    let names: Vec<String> = app.session.doc.project.texture_sets.iter().map(|set| set.name.clone()).collect();
    let Some(draft) = app.ui.export.settings.as_mut() else { return };

    ui.set_width(EXPORT_SET_LIST_WIDTH);
    ui.label(RichText::new(tr("Texture sets")).strong());
    ui.horizontal(|ui| {
        if ui.small_button(tr("Select all")).clicked() {
            (0..names.len()).for_each(|ts| draft.set_exported(ts, true));
        }
        if ui.small_button(tr("Select none")).clicked() {
            (0..names.len()).for_each(|ts| draft.set_exported(ts, false));
        }
    });
    egui::ScrollArea::vertical().id_salt("exp_sets").auto_shrink([false; 2]).show(ui, |ui| {
        for (ts, name) in names.into_iter().enumerate() {
            let mut exported = draft.is_exported(ts);
            if ui.checkbox(&mut exported, name).changed() {
                draft.set_exported(ts, exported);
            }
        }
    });
}
