//! The Export textures window: it edits a draft of the project's export settings, saved by Save settings or Export and dropped by Cancel

mod bottom_action;
mod maps_action;

use bottom_action::BottomAction;
use egui::{RichText, Ui, vec2};
use egui_phosphor::regular as ic;
use maps_action::MapsAction;
use serde_json::json;
use texturecraft_engine::core::{ExportFileType, ExportOverrides, ExportSettings, NormalFormat, TextureSetExport};
use texturecraft_engine::formats::{FileFormat, MapSpec};

use crate::TextureCraftApp;
use crate::export_controls as controls;
use crate::export_draft::ExportDraft;
use crate::export_tab::ExportTab;
#[cfg(not(target_arch = "wasm32"))]
use crate::file_dialogs::{Bound, FileDialog};
use crate::i18n::{tr, trf};
use crate::theme::{Tokens, caption, semibold};
use crate::widgets::{icon_button, icon_button_without_tooltip, primary_button};

/// The window's starting size (smaller in a smaller app window): the Settings tab's parameters leave under a quarter of it empty
const EXPORT_DIALOG_SIZE: egui::Vec2 = egui::vec2(1040.0, 590.0);
/// The smallest size it can be dragged to: as wide as the Settings tab's columns need, so no tab is wider than another
const EXPORT_DIALOG_MIN_SIZE: egui::Vec2 = egui::vec2(EXPORT_DIALOG_SIZE.x, 400.0);
/// Room kept around the window inside the app window, and the window's own padding around its body
const EXPORT_DIALOG_MARGIN: f32 = 16.0;
const EXPORT_SET_LIST_WIDTH: f32 = 250.0;
/// Room under the tabs' body for the button row
const EXPORT_FOOTER_HEIGHT: f32 = 44.0;
const OVERRIDE_PEN_SIZE: f32 = 20.0;
/// Tabs: text size, height, the room around their names and between them
const TAB_TEXT_SIZE: f32 = 12.5;
const TAB_HEIGHT: f32 = 28.0;
const TAB_PADDING: f32 = 14.0;
const TAB_GAP: f32 = 2.0;
/// The tab and body frame's border; the open tab's gap in it covers twice that, so no line shows through
const TAB_STROKE: f32 = 1.0;
/// The tabs' panel's inner margin
const BODY_MARGIN: f32 = 8.0;
/// Width of the Output templates tab's naming pattern combo
const NAMING_WIDTH: f32 = 140.0;
/// List of exports columns: size, format and bit depth, and the room a texture set's rows are indented by
const LIST_SIZE_WIDTH: f32 = 200.0;
const LIST_FORMAT_WIDTH: f32 = 60.0;
const LIST_BITS_WIDTH: f32 = 70.0;
const LIST_INDENT: f32 = 40.0;
/// The List of exports console's width, and its show / hide button's
const CONSOLE_WIDTH: f32 = 330.0;
const CONSOLE_TOGGLE_WIDTH: f32 = 36.0;
/// Bottom bar buttons are one size
const BOTTOM_BUTTON_SIZE: egui::Vec2 = egui::vec2(120.0, 28.0);
/// Height of a row in the texture set list
const SET_ROW_HEIGHT: f32 = 26.0;
const SET_ROW_PADDING: f32 = 8.0;
/// Room for the * that marks a set with overrides
const SET_ROW_MARKER: f32 = 12.0;
/// Label column of the parameter grids, and the room left for the controls
const PARAMETER_LABEL_WIDTH: f32 = 210.0;
const PARAMETER_GAP: f32 = 10.0;
/// Every parameter's controls end at one right edge, this far from the label column
const PARAMETER_FIELD_WIDTH: f32 = 500.0;
/// Room for the output directory's browse button
const BROWSE_BUTTON_WIDTH: f32 = 34.0;
/// Width of an output map's name in a texture set's Output maps
const MAP_NAME_WIDTH: f32 = 230.0;
/// Size of the naming pattern's info icon
const INFO_ICON_SIZE: f32 = 14.0;
/// Spacing between the List of exports' file cells, and between the Output maps' rows
const LIST_FILES_SPACING: egui::Vec2 = egui::vec2(0.0, 4.0);
const SET_MAPS_SPACING: egui::Vec2 = egui::vec2(12.0, 6.0);
/// Size of a console line
const CONSOLE_TEXT_SIZE: f32 = 11.5;
/// Space between the settings grid and the checkboxes under it
const SETTINGS_CHECKBOX_GAP: f32 = 4.0;
/// Size of a setting's note
const NOTE_TEXT_SIZE: f32 = 11.0;
/// Size of a texture set's name in the list
const SET_NAME_TEXT_SIZE: f32 = 13.0;

pub fn show(app: &mut TextureCraftApp, ctx: &egui::Context) {
    // Closed by Cancel, Esc or the title bar: the unsaved edits go
    if !app.ui.windows.export {
        app.ui.export.settings = None;
        #[cfg(not(target_arch = "wasm32"))]
        {
            app.export_run = None;
        }
        return;
    }
    #[cfg(not(target_arch = "wasm32"))]
    advance_export(app, ctx);
    if app.ui.export.settings.as_ref().is_none_or(|draft| draft.sets.len() != app.session.set_count()) {
        app.ui.export.settings = Some(ExportDraft::from_project(&app.session.doc.project, &app.ui.export.dir));
        // The window always opens on Settings
        app.ui.export.tab = ExportTab::Settings;
    }

    let tk = Tokens::get(ctx);
    let mut open = true;
    let mut action = None;
    let mut presets = texturecraft_engine::formats::builtin_presets();
    controls::sort_templates(&mut presets);
    let max_size = (ctx.content_rect().size() - egui::Vec2::splat(2.0 * EXPORT_DIALOG_MARGIN)).max(egui::Vec2::ZERO);
    // Opens centred, moves by its title bar and sizes by its edges (the other dialogs are pinned to the centre); the tabs fill whatever size it has
    let window = egui::Window::new(tr("Export textures"))
        .id(egui::Id::unique("export_textures"))
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_size(EXPORT_DIALOG_SIZE.min(max_size))
        .min_size(EXPORT_DIALOG_MIN_SIZE.min(max_size))
        .max_size(max_size)
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(ctx.content_rect().center());
    window.show(ctx, |ui| {
        let mut open_tab = None;
        ui.horizontal(|ui| {
            open_tab = tab_strip(app, ui, exporting(app));
            if app.ui.export.tab == ExportTab::Settings {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(tr("Include all")).clicked()
                        && let Some(draft) = app.ui.export.settings.as_mut()
                    {
                        (0..draft.sets.len()).for_each(|ts| draft.set_exported(ts, true));
                    }
                });
            }
            #[cfg(not(target_arch = "wasm32"))]
            if app.ui.export.tab == ExportTab::ListOfExports {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(tr("Open output directory")).clicked()
                        && let Some(dir) = app.ui.export.settings.as_ref().map(|draft| draft.settings.dir.clone())
                        && let Err(error) = open_folder(&dir)
                    {
                        app.toast(trf("Cannot open {dir}: {error}", &[("dir", &dir), ("error", &error)]), true);
                    }
                });
            }
            if app.ui.export.tab == ExportTab::OutputTemplates {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add(egui::Label::new(RichText::new(ic::INFO).size(INFO_ICON_SIZE)).selectable(false))
                        .on_hover_text(tr("Names the input maps on this page as TextureCraft or OpenPBR calls them"));
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
        // The open tab joins the panel below it
        ui.add_space(-ui.spacing().item_spacing.y);
        let body_height = (ui.available_height() - EXPORT_FOOTER_HEIGHT - 2.0 * BODY_MARGIN).max(0.0);
        let body_corners = egui::CornerRadius { nw: 0, ne: tk.radius, sw: tk.radius, se: tk.radius };
        let border = egui::Stroke::new(TAB_STROKE, tk.separator);
        egui::Frame::new().fill(tk.panel).stroke(border).corner_radius(body_corners).inner_margin(BODY_MARGIN).show(ui, |ui| {
            ui.allocate_ui_with_layout(vec2(ui.available_width(), body_height), egui::Layout::left_to_right(egui::Align::Min), |ui| {
                // Every tab's frame fills the window, so its right edge lines up with the row above it
                let body_width = ui.available_width();
                ui.set_width(body_width);
                ui.set_height(body_height);
                if app.ui.export.tab == ExportTab::OutputTemplates {
                    crate::export_templates::tab(app, ui, &presets, body_width);
                    return;
                }
                if app.ui.export.tab == ExportTab::ListOfExports {
                    list_of_exports(app, ui);
                    return;
                }
                ui.vertical(|ui| export_set_list(app, ui));
                ui.separator();
                ui.vertical(|ui| export_settings_tab(app, ui, &presets));
            });
        });
        // The body's top border stops under the open tab, so the tab opens into it
        if let Some(tab) = open_tab {
            let gap = [tab.left_bottom() + vec2(TAB_STROKE, 0.0), tab.right_bottom() - vec2(TAB_STROKE, 0.0)];
            ui.painter().line_segment(gap, egui::Stroke::new(2.0 * TAB_STROKE, tk.panel));
        }
        ui.add_space(BODY_MARGIN);
        action = bottom_bar(app, ui);
    });

    match action {
        // As the reference: Save settings keeps them and closes the window
        Some(BottomAction::Save) => {
            if save(app) {
                app.ui.windows.export = false;
            }
        }
        Some(BottomAction::Export) => {
            if save(app) {
                export_sets(app);
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        Some(BottomAction::CancelExport) => {
            if let Some(run) = app.export_run.as_mut() {
                run.cancelled = true;
            }
        }
        Some(BottomAction::Cancel) => app.ui.windows.export = false,
        None => {}
    }
    if !open {
        app.ui.windows.export = false;
    }
}

/// The tabs: the open one is framed and opens onto the panel below, the others sit back; returns the open tab's rect
fn tab_strip(app: &mut TextureCraftApp, ui: &mut Ui, locked: bool) -> Option<egui::Rect> {
    let tk = Tokens::get(ui.ctx());
    let mut open_rect = None;
    ui.spacing_mut().item_spacing.x = TAB_GAP;
    for kind in ExportTab::ALL {
        let name = kind.label();
        let open = app.ui.export.tab == kind;
        let color = if open { tk.text_strong } else { tk.text_dim };
        let galley = ui.painter().layout_no_wrap(name.to_string(), semibold(TAB_TEXT_SIZE), color);
        let (rect, tab) = ui.allocate_exact_size(vec2(galley.size().x + 2.0 * TAB_PADDING, TAB_HEIGHT), egui::Sense::click());
        tab.widget_info(|| egui::WidgetInfo::selected(egui::Role::Tab, true, open, name));
        let fill = if open {
            tk.panel
        } else if tab.hovered() {
            tk.row_hover
        } else {
            tk.row
        };
        let corners = egui::CornerRadius { nw: tk.radius, ne: tk.radius, sw: 0, se: 0 };
        ui.painter().rect_filled(rect, corners, fill);
        if open {
            ui.painter().rect_stroke(rect, corners, egui::Stroke::new(TAB_STROKE, tk.separator), egui::StrokeKind::Inside);
            open_rect = Some(rect);
        }
        ui.painter().galley(rect.center() - galley.size() / 2.0, galley, color);
        if tab.clicked() && !locked {
            app.ui.export.tab = kind;
        }
    }

    open_rect
}

/// Reset all, the count of textures, Cancel, Save settings and Export
fn bottom_bar(app: &mut TextureCraftApp, ui: &mut Ui) -> Option<BottomAction> {
    let tk = Tokens::get(ui.ctx());
    let mut action = None;
    // While an export runs: its progress, and a way to stop it
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(run) = app.export_run.as_ref().filter(|run| !run.finished) {
        ui.horizontal(|ui| {
            let bar_width = ui.available_width() - BOTTOM_BUTTON_SIZE.x - ui.spacing().item_spacing.x;
            ui.add(egui::ProgressBar::new(run.fraction()).show_percentage().desired_width(bar_width).desired_height(BOTTOM_BUTTON_SIZE.y));
            if ui.add(egui::Button::new(tr("Cancel Export")).min_size(BOTTOM_BUTTON_SIZE)).clicked() {
                action = Some(BottomAction::CancelExport);
            }
        });
        return action;
    }
    let count = exported_count(app);
    // On Output templates, a template without outputs can't be saved into the settings or exported
    let empty_template = app.ui.export.tab == ExportTab::OutputTemplates
        && app
            .ui
            .export
            .draft
            .as_ref()
            .map(|draft| draft.maps.is_empty())
            .or_else(|| app.session.export_preset(&app.ui.export.preset).map(|preset| preset.maps.is_empty()))
            .unwrap_or(false);
    ui.horizontal(|ui| {
        if app.ui.export.tab == ExportTab::Settings
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

/// Show `dir` in the system's file manager, creating it first so a folder nothing was exported to yet opens too
#[cfg(not(target_arch = "wasm32"))]
fn open_folder(dir: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;

    let file_manager = if cfg!(target_os = "windows") {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    // Explorer reads only backslashes
    let dir = if cfg!(target_os = "windows") { dir.replace('/', "\\") } else { dir.to_string() };
    std::process::Command::new(file_manager).arg(dir).spawn().map(drop)
}

/// Start exporting with the saved settings: the List of exports tab shows the run
#[cfg(not(target_arch = "wasm32"))]
fn export_sets(app: &mut TextureCraftApp) -> bool {
    let dir = app.session.doc.project.export.as_ref().map(|settings| settings.dir.clone()).unwrap_or_default();
    match texturecraft_engine::texture_export::TextureExport::start(&app.session, &json!({}), &dir, "file.exportTextures") {
        Ok(export) => {
            app.export_run = Some(crate::export_run::ExportRun::new(export));
            app.ui.export.tab = ExportTab::ListOfExports;
            true
        }
        Err(error) => {
            app.toast(error.to_string(), true);
            false
        }
    }
}

/// An export is running (the tabs stay on List of exports until it ends)
fn exporting(app: &TextureCraftApp) -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    {
        app.export_run.as_ref().is_some_and(|run| !run.finished)
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = app;
        false
    }
}

/// One step of the running export a frame: a texture set, then the extras, logging what it writes
#[cfg(not(target_arch = "wasm32"))]
fn advance_export(app: &mut TextureCraftApp, ctx: &egui::Context) {
    let names: Vec<String> = app.session.doc.project.texture_sets.iter().map(|set| set.name.clone()).collect();
    let (Some(run), session) = (app.export_run.as_mut(), &mut app.session) else { return };
    let frame = ctx.cumulative_frame_nr();
    if run.finished || run.stepped_frame == Some(frame) {
        return;
    }
    run.stepped_frame = Some(frame);
    if run.cancelled {
        run.log.push((tr("Export cancelled").to_string(), false));
        run.finished = true;
        return;
    }

    if let Some(sets) = run.export.next_sets() {
        for &ts in sets {
            let name = names.get(ts).map(String::as_str).unwrap_or_default();
            run.log.push((trf("Exporting texture set {name}...", &[("name", &name)]), false));
        }
    }
    let step = match run.export.step(session) {
        Ok(Some(written)) => Ok(written),
        Ok(None) => run.export.finish(session, "file.exportTextures").inspect(|_| run.finished = true),
        Err(error) => Err(error),
    };
    match step {
        Ok(written) => {
            for path in written {
                let file = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
                run.log.push((trf("Wrote {file}", &[("file", &file)]), false));
            }
            if run.finished {
                run.log.extend(run.export.warnings.iter().map(|warning| (warning.clone(), false)));
                run.log.push((trf("Export successfully finished: {n} files", &[("n", &run.export.written.len())]), true));
            }
        }
        Err(error) => {
            run.log.push((error.to_string(), false));
            run.finished = true;
        }
    }
    ctx.request_repaint();
}

/// In the browser the textures download (one zip for several files)
#[cfg(target_arch = "wasm32")]
fn export_sets(app: &mut TextureCraftApp) -> bool {
    crate::web_files::export_textures(app);
    true
}

/// List of exports: every file per texture set, then the export's console
fn list_of_exports(app: &mut TextureCraftApp, ui: &mut Ui) {
    let tk = Tokens::get(ui.ctx());
    let names: Vec<String> = app.session.doc.project.texture_sets.iter().map(|set| set.name.clone()).collect();
    let params = plan_params(app);
    let plan = texturecraft_engine::execute(&mut app.session, "export.plan", &params);
    let console_open = !app.ui.export.console_closed;
    let tree_width = ui.available_width() - if console_open { CONSOLE_WIDTH } else { 0.0 } - CONSOLE_TOGGLE_WIDTH;

    ui.vertical(|ui| {
        ui.set_width(tree_width);
        let files = match &plan {
            Ok(plan) => plan.as_array().cloned().unwrap_or_default(),
            Err(error) => {
                ui.label(RichText::new(error.to_string()).color(tk.danger));
                return;
            }
        };
        egui::ScrollArea::vertical().id_salt("exp_list").auto_shrink([false; 2]).show(ui, |ui| {
            for (ts, name) in names.iter().enumerate() {
                let set_files: Vec<&serde_json::Value> = files.iter().filter(|file| file["ts"].as_u64() == Some(ts as u64)).collect();
                if set_files.is_empty() {
                    continue;
                }
                egui::CollapsingHeader::new(RichText::new(name).strong()).id_salt(("exp_list_set", ts)).default_open(true).show(ui, |ui| {
                    // Fixed column widths, so every texture set's columns line up
                    let name_width = (tree_width - LIST_SIZE_WIDTH - LIST_FORMAT_WIDTH - LIST_BITS_WIDTH - LIST_INDENT).max(LIST_SIZE_WIDTH);
                    let cell = |ui: &mut Ui, width: f32, text: RichText| {
                        ui.allocate_ui_with_layout(vec2(width, ui.spacing().interact_size.y), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.set_width(width);
                            ui.add(egui::Label::new(text).truncate());
                        });
                    };
                    egui::Grid::new(("exp_list_files", ts)).num_columns(4).spacing(LIST_FILES_SPACING).show(ui, |ui| {
                        for file in set_files {
                            cell(ui, name_width, RichText::new(file["file"].as_str().unwrap_or_default()));
                            let (width, height) = (file["size"][0].as_u64().unwrap_or(0), file["size"][1].as_u64().unwrap_or(0));
                            let size = if file["setSize"].as_bool().unwrap_or(false) {
                                trf("{width}x{height} (texture set size)", &[("width", &width), ("height", &height)])
                            } else {
                                format!("{width} x {height}")
                            };
                            cell(ui, LIST_SIZE_WIDTH, RichText::new(size).color(tk.text_dim));
                            cell(ui, LIST_FORMAT_WIDTH, RichText::new(file["format"].as_str().unwrap_or_default()).color(tk.text_dim));
                            cell(ui, LIST_BITS_WIDTH, RichText::new(trf("{n} bits", &[("n", &file["bitDepth"].as_u64().unwrap_or(0))])).color(tk.text_dim));
                            ui.end_row();
                        }
                    });
                });
            }
        });
    });

    let toggle = if console_open { ic::CARET_DOUBLE_RIGHT } else { ic::CARET_DOUBLE_LEFT };
    if icon_button(ui, toggle, tr("Show or hide the console"), false, OVERRIDE_PEN_SIZE).clicked() {
        app.ui.export.console_closed = console_open;
    }
    if console_open {
        ui.separator();
        ui.vertical(|ui| {
            ui.label(caption(tr("Console")));
            egui::ScrollArea::vertical().id_salt("exp_console").auto_shrink([false; 2]).stick_to_bottom(true).show(ui, |ui| {
                #[cfg(not(target_arch = "wasm32"))]
                for (line, success) in app.export_run.iter().flat_map(|run| run.log.iter()) {
                    ui.label(RichText::new(line).size(CONSOLE_TEXT_SIZE).color(if *success { tk.accent } else { tk.text }));
                }
            });
        });
    }
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

    #[cfg(not(target_arch = "wasm32"))]
    let mut pick_dir = false;
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
                ui.add(egui::TextEdit::singleline(&mut settings.dir).desired_width(PARAMETER_FIELD_WIDTH - BROWSE_BUTTON_WIDTH));
                pick_dir = ui.button("...").clicked();
            });
            ui.end_row();
        }
        let parameters = &mut settings.parameters;
        ui.label(tr("Output template"));
        controls::template_combo(ui, "exp_preset", &mut parameters.preset, builtins, user_presets, PARAMETER_FIELD_WIDTH);
        ui.end_row();
        ui.label(tr("File type"));
        controls::file_type_combos(ui, "exp_fmt", &mut parameters.file_type, PARAMETER_FIELD_WIDTH);
        ui.end_row();
        ui.label(tr("Size"));
        controls::size_combo(ui, "exp_size", &mut parameters.size, None, PARAMETER_FIELD_WIDTH);
        ui.end_row();
        ui.label(tr("Padding"));
        controls::padding_controls(ui, "exp_pad", &mut parameters.padding, &mut parameters.dilation, PARAMETER_FIELD_WIDTH);
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
        egui::ComboBox::from_id_salt("exp_nf").width(PARAMETER_FIELD_WIDTH).selected_text(shown).show_ui(ui, |ui| {
            for (value, label) in [("", preset_format), ("openGl", "OpenGL".to_string()), ("directX", "DirectX".to_string())] {
                ui.selectable_value(&mut settings.normal_format, value.to_string(), label);
            }
        });
        ui.end_row();
        ui.label(tr("Color space"));
        let shown = if settings.color_space.is_empty() { tr("Default") } else { settings.color_space.as_str() };
        egui::ComboBox::from_id_salt("exp_cs").width(PARAMETER_FIELD_WIDTH).selected_text(shown).show_ui(ui, |ui| {
            ui.selectable_value(&mut settings.color_space, String::new(), tr("Default"));
            for name in ["sRGB", "Linear Rec.709", "ACEScg", "ACES2065-1"] {
                ui.selectable_value(&mut settings.color_space, name.to_string(), name);
            }
        });
        ui.end_row();
        export_note(ui, tr("Color maps only: normal, roughness, metallic and other data maps stay linear"));
    });
    ui.add_space(SETTINGS_CHECKBOX_GAP);
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
    let inherited = |ui: &mut Ui, text: String| ui.label(RichText::new(text).color(tk.text_dim));
    ui.add_space(PARAMETER_GAP);
    egui::Grid::new(("exp_set_grid", ts)).num_columns(3).spacing(vec2(PARAMETER_GAP, PARAMETER_GAP)).show(ui, |ui| {
        parameter_label(ui, tr("Output template"), PARAMETER_LABEL_WIDTH - OVERRIDE_PEN_SIZE - PARAMETER_GAP);
        if override_pen(ui, overrides.preset.is_some()) {
            overrides.preset = overrides.preset.take().xor(Some(global.preset.clone()));
        }
        match overrides.preset.as_mut() {
            Some(preset) => controls::template_combo(ui, "exp_set_preset", preset, builtins, user_presets, PARAMETER_FIELD_WIDTH),
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
            Some(file_type) => controls::file_type_combos(ui, "exp_set_fmt", file_type, PARAMETER_FIELD_WIDTH),
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
            Some(size) => controls::size_combo(ui, "exp_set_size", size, set_size, PARAMETER_FIELD_WIDTH),
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
            Some((padding, dilation)) => controls::padding_controls(ui, "exp_set_pad", padding, dilation, PARAMETER_FIELD_WIDTH),
            None => {
                inherited(ui, controls::padding_text(&global.padding, global.dilation));
            }
        }
        ui.end_row();
    });
}

/// The output maps of texture set `ts`'s template (its own or the global one)
fn set_template_maps(app: &TextureCraftApp, ts: usize) -> Vec<MapSpec> {
    let Some(draft) = app.ui.export.settings.as_ref() else { return Vec::new() };

    let preset = draft.sets.get(ts).and_then(|set| set.overrides.preset.clone()).unwrap_or_else(|| draft.settings.parameters.preset.clone());
    app.session.export_preset(&preset).map(|preset| preset.maps).unwrap_or_default()
}

/// A texture set's output maps: a tick writes a map, its pen gives it its own file type
fn set_output_maps(ui: &mut Ui, draft: &mut ExportDraft, ts: usize, maps: &[MapSpec]) {
    let parameters = draft.sets.get(ts).map(|set| set.overrides.apply(&draft.settings.parameters)).unwrap_or_default();
    let set_type = parameters.file_type;
    let set_format = FileFormat::from_extension(&set_type.format);
    // What a map writes without its own override: the set's file type, else the template's (as the engine's apply_file_types)
    let inherited_type = |template: &MapSpec| {
        let format = set_format.unwrap_or(template.format);
        let bit_depth = format.depth(if set_type.bit_depth > 0 { set_type.bit_depth } else { template.bit_depth });
        let dithering = if set_format.is_some() { set_type.dithering } else { template.options.dithering };
        ExportFileType { format: format.extension().to_string(), bit_depth, dithering: dithering && bit_depth == 8 }
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

                    for template in maps {
                        let key = template.key();
                        let map = set.maps.entry(key.clone()).or_default();
                        match action {
                            MapsAction::Check => map.skip = false,
                            MapsAction::Uncheck => map.skip = true,
                            MapsAction::Invert => map.skip = !map.skip,
                            MapsAction::Override => map.file_type = Some(inherited_type(template)),
                            MapsAction::Inherit => map.file_type = None,
                        }
                    }
                });
            });
        });
    });
    let tk = Tokens::get(ui.ctx());
    egui::Grid::new(("exp_set_maps", ts)).num_columns(3).spacing(SET_MAPS_SPACING).show(ui, |ui| {
        for template in maps {
            let key = template.key();
            let map = set.maps.entry(key.clone()).or_default();
            ui.horizontal(|ui| {
                let mut exported = !map.skip;
                ui.checkbox(&mut exported, "");
                map.skip = !exported;
                ui.allocate_ui(vec2(MAP_NAME_WIDTH, ui.spacing().interact_size.y), |ui| {
                    ui.set_width(MAP_NAME_WIDTH);
                    ui.add(egui::Label::new(RichText::new(&key).monospace()).truncate());
                });
            });
            if override_pen(ui, map.file_type.is_some()) {
                map.file_type = map.file_type.take().xor(Some(inherited_type(template)));
            }
            match map.file_type.as_mut() {
                Some(file_type) => controls::map_file_type_combos(ui, &format!("exp_map_{key}"), file_type),
                None => {
                    let file_type = inherited_type(template);
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

/// The pen beside a parameter, pressed while this texture set overrides it; true when clicked
fn override_pen(ui: &mut Ui, overridden: bool) -> bool {
    icon_button_without_tooltip(ui, ic::PENCIL_SIMPLE, tr("Override for this texture set"), overridden, OVERRIDE_PEN_SIZE).clicked()
}

/// A dim one-line note under the setting in the row above
fn export_note(ui: &mut Ui, text: &str) {
    let tk = Tokens::get(ui.ctx());
    ui.label("");
    ui.label(RichText::new(text).size(NOTE_TEXT_SIZE).color(tk.text_dim));
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
    let galley = crate::widgets::elided(ui.painter(), name, SET_NAME_TEXT_SIZE, tk.text_strong, width);
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
