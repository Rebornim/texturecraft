use super::controls::{choice, hint, indented, integer, number, section, subsection, text_input};
use super::settings_section::SettingsSection;
use crate::TextureCraftApp;
use crate::i18n::tr;
use crate::theme::Tokens;
use crate::widgets::icon_button;
use egui::Ui;
use egui_phosphor::regular as icon;
use serde_json::{Map, Value, json};
use texturecraft_engine::core::{
    BakeAntialiasing, BakePaintTarget, BakeResolution, BakeSettings, CageType, CommonBakeSettings, MeshMatchMode, SkewCorrectionMethod,
};

/// How many rows the high poly list box is tall
const HIGH_POLY_LIST_ROWS: f32 = 5.0;

/// Space between the panel's sections, in points
const SECTION_GAP: f32 = 8.0;

/// Widest dilation the slider offers, in texels (the bake settings command accepts the same)
const MAX_DILATION_WIDTH: u32 = 128;

/// The Common Settings, Cage and Skew Correction sections, cage and skew greyed with the reason when they can't apply
pub(super) fn show(app: &mut TextureCraftApp, ui: &mut Ui, settings: &mut BakeSettings) -> Option<Value> {
    let mut reset = Map::new();
    let has_high = !app.session.bake_meshes.high.is_empty();
    let jump = super::section_jump::take(ui.ctx());
    ui.push_id("common", |ui| {
        super::section_jump::anchor(ui, SettingsSection::Common, jump);
        if section(ui, tr("Common Settings"), &[], |ui| {
            subsection(ui, tr("Output Settings"), |ui| {
                output_size(ui, &mut settings.common);
                choice(
                    ui,
                    tr("Antialiasing"),
                    &mut settings.common.antialiasing,
                    &[
                        (BakeAntialiasing::None, tr("None")),
                        (BakeAntialiasing::Subsampling2x2, tr("Subsampling 2x2")),
                        (BakeAntialiasing::Subsampling4x4, tr("Subsampling 4x4")),
                        (BakeAntialiasing::Subsampling8x8, tr("Subsampling 8x8")),
                    ],
                );
                integer(ui, tr("Dilation Width"), &mut settings.common.dilation, 0..=MAX_DILATION_WIDTH);
                ui.checkbox(&mut settings.common.apply_diffusion, tr("Apply Diffusion"));
            });
            subsection(ui, tr("High-Poly Parameters"), |ui| {
                ui.checkbox(&mut settings.high_poly.use_low_as_high, tr("Use Low Poly Mesh as High Poly Mesh"));
                high_poly_scene(app, ui);
                // Without a high poly the reference shows everything that needs one greyed, not hidden
                ui.add_enabled_ui(has_high, |ui| {
                    ui.checkbox(&mut settings.high_poly.ignore_backface, tr("Ignore Backface"));
                    choice(
                        ui,
                        tr("Match"),
                        &mut settings.high_poly.match_mode,
                        &[(MeshMatchMode::Always, tr("Always")), (MeshMatchMode::ByMeshName, tr("By Mesh Name"))],
                    );
                    text_input(ui, tr("Low Poly Mesh Suffix"), &mut settings.high_poly.low_suffix);
                    text_input(ui, tr("High Poly Mesh Suffix"), &mut settings.high_poly.high_suffix);
                    text_input(ui, tr("Ignore Backface Suffix"), &mut settings.high_poly.ignore_backface_suffix);
                });
            });
        })
        .is_some()
        {
            // Restoring keeps whether the texture set is included in the bake
            settings.common = CommonBakeSettings { is_enabled: settings.common.is_enabled, ..CommonBakeSettings::default() };
            let mut defaults = json!(settings.common);
            if let Some(fields) = defaults.as_object_mut() {
                fields.remove("is_enabled");
            }
            reset.insert("common".into(), defaults);
            settings.high_poly = Default::default();
            reset.insert("high_poly".into(), json!(settings.high_poly));
        }
    });

    ui.add_space(SECTION_GAP);
    ui.push_id("cage", |ui| {
        super::section_jump::anchor(ui, SettingsSection::Cage, jump);
        let menu = [tr("Reset Cage Parameters"), tr("Clear All Cage Paint Strokes"), tr("Reset and Clear All")];
        let choice_made = section(ui, tr("Cage"), &menu, |ui| {
            // The cage steers rays toward a separate high poly, so it has nothing to do without one
            let cage_applies = has_high && !settings.high_poly.use_low_as_high;
            if !has_high {
                hint(ui, tr("The cage applies once a high poly mesh is loaded."));
            } else if !cage_applies {
                hint(ui, tr("The cage is not used while the low poly mesh is used as the high poly mesh."));
            }
            ui.add_enabled_ui(cage_applies, |ui| {
                choice(
                    ui,
                    tr("Cage Type"),
                    &mut settings.cage.kind,
                    &[(CageType::DistanceBased, tr("Distance-Based")), (CageType::CustomFile, tr("Custom File"))],
                );
                if settings.cage.kind == CageType::DistanceBased {
                    paint_button(app, ui, BakePaintTarget::Cage);
                    indented(ui, |ui| {
                        number(ui, tr("Max Frontal Distance"), &mut settings.cage.max_frontal, 0.0..=0.05, 4);
                        number(ui, tr("Max Rear Distance"), &mut settings.cage.max_rear, 0.0..=0.05, 4);
                        ui.checkbox(&mut settings.cage.relative_to_bounding_box, tr("Relative to Bounding Box"));
                        ui.checkbox(&mut settings.cage.average_normals, tr("Average Normals"));
                    });
                }
            });
            if settings.cage.kind == CageType::CustomFile {
                cage_file(app, ui, cage_applies);
            }
        });
        if let Some(index) = choice_made {
            if index != 1 {
                settings.cage = Default::default();
                reset.insert("cage".into(), json!(settings.cage));
            }
            if index != 0 {
                app.run("bake.cage.reset", json!({}));
            }
        }
    });

    // Shown greyed when it can't apply, as the reference does, with the reason on top
    let skew_applies = has_high && settings.skew_applies();
    ui.add_space(SECTION_GAP);
    ui.push_id("skew", |ui| {
        super::section_jump::anchor(ui, SettingsSection::Skew, jump);
        let menu = [tr("Reset Skew Correction Parameters"), tr("Clear All Skew Paint Strokes"), tr("Reset and Clear All")];
        let choice_made = section(ui, tr("Skew Correction"), &menu, |ui| {
            if !skew_applies {
                hint(ui, skew_unavailable_reason(settings, has_high));
            }
            ui.add_enabled_ui(skew_applies, |ui| {
                paint_button(app, ui, BakePaintTarget::Skew);
                ui.checkbox(&mut settings.skew.edge_protection, tr("Edge Protection"));
                ui.add_enabled_ui(settings.skew.edge_protection, |ui| {
                    number(ui, tr("Edge Distance"), &mut settings.skew.edge_distance, 0.0..=1.0, 2);
                    number(ui, tr("Edge Contrast"), &mut settings.skew.edge_contrast, 0.0..=1.0, 2);
                });
                choice(
                    ui,
                    tr("Skew Correction Method"),
                    &mut settings.skew.method,
                    &[(SkewCorrectionMethod::MeshNormalsBased, tr("Mesh Normals Based")), (SkewCorrectionMethod::FaceNormalsBased, tr("Face Normals Based"))],
                );
            });
        });
        if let Some(index) = choice_made {
            if index != 1 {
                settings.skew = Default::default();
                reset.insert("skew".into(), json!(settings.skew));
            }
            if index != 0 {
                app.run("bake.skew.clear", json!({}));
            }
        }
    });

    (!reset.is_empty()).then_some(Value::Object(reset))
}

/// Why Skew Correction can't apply: no high poly (`has_high` false), or `settings` rule it out (see [`BakeSettings::skew_applies`])
fn skew_unavailable_reason(settings: &BakeSettings, has_high: bool) -> &'static str {
    if !has_high {
        tr("Skew Correction applies once a high poly mesh is loaded.")
    } else if settings.high_poly.use_low_as_high {
        tr("Skew Correction is not available while the low poly mesh is used as the high poly mesh.")
    } else if settings.cage.kind == CageType::CustomFile {
        tr("Skew Correction needs a distance-based cage: a custom cage file sets the ray directions itself.")
    } else {
        tr("Skew Correction needs Average Normals turned on in the cage settings.")
    }
}

fn output_size(ui: &mut Ui, settings: &mut CommonBakeSettings) {
    ui.label(tr("Output Size"));
    ui.horizontal(|ui| {
        let button_size = ui.spacing().interact_size.y;
        let dropdown_width = ((ui.available_width() - button_size - ui.spacing().item_spacing.x * 2.0) / 2.0).max(40.0);
        let resolution = |ui: &mut Ui, id: &str, value: &mut BakeResolution| {
            egui::ComboBox::from_id_salt(id).width(dropdown_width).selected_text(value.pixels().to_string()).show_ui(ui, |ui| {
                for resolution in BakeResolution::ALL {
                    ui.selectable_value(value, resolution, resolution.pixels().to_string());
                }
            });
        };

        // Width, the lock, then the height it locks, as in the reference
        resolution(ui, "output_width", &mut settings.resolution);
        let locked = settings.resolution_height.is_none();
        if icon_button(ui, if locked { icon::LOCK_SIMPLE } else { icon::LOCK_SIMPLE_OPEN }, tr("Lock Output Size"), locked, button_size).clicked() {
            settings.resolution_height = if locked { Some(settings.resolution) } else { None };
        }

        let locked = settings.resolution_height.is_none();
        let mut height = settings.resolution_height.unwrap_or(settings.resolution);
        ui.add_enabled_ui(!locked, |ui| resolution(ui, "output_height", &mut height));
        if !locked {
            settings.resolution_height = Some(height);
        }
    });
}

/// The high poly meshes as a list box, with + to add some and − to remove the selected one
fn high_poly_scene(app: &mut TextureCraftApp, ui: &mut Ui) {
    let tokens = Tokens::get(ui.ctx());
    let names: Vec<String> = app.session.bake_meshes.high.iter().map(|mesh| mesh.name.clone()).collect();
    let selected_id = ui.make_persistent_id("high_poly_selected");
    let mut selected = ui.data(|data| data.get_temp::<usize>(selected_id)).filter(|index| *index < names.len());

    ui.label(tr("High Poly Scene"));
    let (mut add, mut remove) = (false, false);
    ui.horizontal_top(|ui| {
        let button_size = ui.spacing().interact_size.y;
        let list_width = (ui.available_width() - button_size - ui.spacing().item_spacing.x).max(0.0);
        let list_height = HIGH_POLY_LIST_ROWS * ui.spacing().interact_size.y;
        egui::Frame::NONE.fill(tokens.field).stroke(egui::Stroke::new(1.0, tokens.field_border)).corner_radius(tokens.cr()).show(ui, |ui| {
            ui.set_min_size(egui::vec2(list_width, list_height));
            ui.set_max_width(list_width);
            egui::ScrollArea::both().id_salt("high_poly_list").max_height(list_height).auto_shrink([false; 2]).show(ui, |ui| {
                for (index, name) in names.iter().enumerate() {
                    if ui.selectable_label(selected == Some(index), name).on_hover_text(name).clicked() {
                        selected = Some(index);
                    }
                }
            });
        });
        ui.vertical(|ui| {
            add = icon_button(ui, icon::PLUS, tr("Add High-Poly Meshes"), false, button_size).clicked();
            remove = ui.add_enabled_ui(selected.is_some(), |ui| icon_button(ui, icon::MINUS, tr("Remove"), false, button_size)).inner.clicked();
        });
    });

    if let (true, Some(index)) = (remove, selected) {
        app.run("bake.highPoly.remove", json!({ "index": index }));
        selected = None;
    }
    ui.data_mut(|data| {
        if let Some(index) = selected {
            data.insert_temp(selected_id, index);
        } else {
            data.remove_temp::<usize>(selected_id);
        }
    });
    if add {
        crate::web_files::import_bake_mesh(app, &ui.ctx().clone(), false);
    }
}

/// The cage file row; while the cage can't `apply` only removing a loaded one stays available, so a saved cage can still be dropped
fn cage_file(app: &mut TextureCraftApp, ui: &mut Ui, apply: bool) {
    ui.add_enabled(apply, egui::Label::new(tr("Cage Scene")));
    let cage = app.session.bake_meshes.cage.as_ref().map(|mesh| mesh.name.clone());
    ui.horizontal(|ui| {
        let button_size = ui.spacing().interact_size.y;
        let label_width = (ui.available_width() - 2.0 * (button_size + ui.spacing().item_spacing.x)).max(0.0);
        ui.add_enabled_ui(apply, |ui| {
            ui.add_sized([label_width, ui.spacing().interact_size.y], egui::Label::new(cage.as_deref().unwrap_or("")).truncate());
            if icon_button(ui, icon::FOLDER_OPEN, tr("Load Cage"), false, button_size).clicked() {
                crate::web_files::import_bake_mesh(app, &ui.ctx().clone(), true);
            }
        });
        if cage.is_some() && icon_button(ui, icon::TRASH, tr("Remove Cage"), false, button_size).clicked() {
            app.run("bake.cage.clear", json!({}));
        }
    });
}

/// "Paint Cage" or "Paint Skew Correction", which turns into "Stop Painting" while that brush is on, with the brush sliders below it
fn paint_button(app: &mut TextureCraftApp, ui: &mut Ui, target: BakePaintTarget) {
    let label = match target {
        BakePaintTarget::Cage => tr("Paint Cage"),
        BakePaintTarget::Skew => tr("Paint Skew Correction"),
    };
    let mut brush = app.ui.bake_paint_brush;
    let active = brush.on && brush.target == target;
    let text = if active { format!("{}  {}", icon::PROHIBIT, tr("Stop Painting")) } else { format!("{}  {label}", icon::PENCIL_SIMPLE) };
    if ui.add_sized([ui.available_width(), ui.spacing().interact_size.y], egui::Button::new(text).selected(active)).clicked() {
        brush.on = !active;
        if brush.target != target {
            brush.value = match target {
                BakePaintTarget::Cage => 0.5,
                BakePaintTarget::Skew => 1.0,
            };
        }
        brush.target = target;
    }
    if brush.on && brush.target == target {
        number(ui, tr("Radius"), &mut brush.radius, 0.002..=0.2, 3);
        number(ui, tr("Value"), &mut brush.value, 0.0..=1.0, 2);
        number(ui, tr("Hardness"), &mut brush.hardness, 0.0..=1.0, 2);
        if app.ui.layout == crate::state::ViewLayout::Only3D {
            app.ui.layout = crate::state::ViewLayout::Split;
        }
    }
    app.ui.bake_paint_brush = brush;
}
