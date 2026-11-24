use super::controls::{choice, integer, number, section, text_input};
use crate::TextureCraftApp;
use crate::i18n::{tr, trf};
use crate::property_sheet::{FileRow, PropertySheet};
use egui_phosphor::regular as icon;
use serde_json::{Map, Value, json};
use texturecraft_engine::core::{
    BakeAntialiasing, BakePaintTarget, BakeResolution, BakeSettings, CageType, CommonBakeSettings, MeshMatchMode, SkewCorrectionMethod,
};

/// The high poly list is never shorter than this many rows, so it reads as a list while empty
const HIGH_POLY_LIST_MIN_ROWS: usize = 2;

/// Widest dilation the slider offers, in texels (the bake settings command accepts the same)
const MAX_DILATION_WIDTH: u32 = 128;

/// The Common Settings, Cage and Skew Correction sections, cage and skew greyed with the reason when they can't apply
pub(super) fn show(app: &mut TextureCraftApp, sheet: &mut PropertySheet<'_>, settings: &mut BakeSettings) -> Option<Value> {
    let mut reset = Map::new();
    let has_high = !app.session.doc.bake_meshes.high.is_empty();
    let common_reset = section(sheet, tr("Common Settings"), &[], |sheet| {
        sheet.sub_header(tr("Output Settings"), |sheet| {
            output_size(sheet, &mut settings.common);
            choice(
                sheet,
                tr("Antialiasing"),
                &mut settings.common.antialiasing,
                &[
                    (BakeAntialiasing::None, tr("None")),
                    (BakeAntialiasing::Subsampling2x2, tr("Subsampling 2x2")),
                    (BakeAntialiasing::Subsampling4x4, tr("Subsampling 4x4")),
                    (BakeAntialiasing::Subsampling8x8, tr("Subsampling 8x8")),
                ],
            );
            integer(sheet, tr("Dilation Width"), &mut settings.common.dilation, 0..=MAX_DILATION_WIDTH);
            sheet.check(tr("Apply Diffusion"), &mut settings.common.apply_diffusion);
        });
        sheet.sub_header(tr("High-Poly Parameters"), |sheet| {
            sheet.check(tr("Use Low Poly Mesh as High Poly Mesh"), &mut settings.high_poly.use_low_as_high);
            high_poly_scene(app, sheet);

            // Without a high poly the reference shows everything that needs one greyed, not hidden
            sheet.greyed(has_high, Some(tr("These settings apply once a high poly mesh is loaded.")), |sheet| {
                sheet.check(tr("Ignore Backface"), &mut settings.high_poly.ignore_backface);
                choice(
                    sheet,
                    tr("Match"),
                    &mut settings.high_poly.match_mode,
                    &[(MeshMatchMode::Always, tr("Always")), (MeshMatchMode::ByMeshName, tr("By Mesh Name"))],
                );
                text_input(sheet, tr("Low Poly Mesh Suffix"), &mut settings.high_poly.low_suffix);
                text_input(sheet, tr("High Poly Mesh Suffix"), &mut settings.high_poly.high_suffix);
                text_input(sheet, tr("Ignore Backface Suffix"), &mut settings.high_poly.ignore_backface_suffix);
            });
        });
    });
    if common_reset.is_some() {
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

    let menu = [tr("Reset Cage Parameters"), tr("Clear All Cage Paint Strokes"), tr("Reset and Clear All")];
    let cage_reset = section(sheet, tr("Cage"), &menu, |sheet| {
        // The cage steers rays toward a separate high poly, so it has nothing to do without one
        let cage_applies = has_high && !settings.high_poly.use_low_as_high;
        let reason = if has_high {
            tr("The cage is not used while the low poly mesh is used as the high poly mesh.")
        } else {
            tr("The cage applies once a high poly mesh is loaded.")
        };
        sheet.greyed(cage_applies, Some(reason), |sheet| {
            choice(
                sheet,
                tr("Cage Type"),
                &mut settings.cage.kind,
                &[
                    (CageType::DistanceBased, tr("Distance-Based")),
                    (CageType::Automatic, tr("Automatic (Experimental)")),
                    (CageType::CustomFile, tr("Custom File")),
                ],
            );
            if settings.cage.kind == CageType::DistanceBased {
                paint_button(app, sheet, BakePaintTarget::Cage);
                sheet.indented(|sheet| {
                    number(sheet, tr("Max Frontal Distance"), &mut settings.cage.max_frontal, 0.0..=0.05, 4);
                    number(sheet, tr("Max Rear Distance"), &mut settings.cage.max_rear, 0.0..=0.05, 4);
                    sheet.check(tr("Relative to Bounding Box"), &mut settings.cage.relative_to_bounding_box);
                    sheet.check(tr("Average Normals"), &mut settings.cage.average_normals);
                });
            }
            if settings.cage.kind == CageType::CustomFile {
                cage_file(app, sheet);
            }
        });
    });
    if let Some(index) = cage_reset {
        if index != 1 {
            settings.cage = Default::default();
            reset.insert("cage".into(), json!(settings.cage));
        }
        if index != 0 {
            app.run("bake.cage.reset", json!({}));
        }
    }

    // Shown greyed when it can't apply, as the reference does, with the reason on hover
    let skew_applies = has_high && settings.skew_applies();
    let menu = [tr("Reset Skew Correction Parameters"), tr("Clear All Skew Paint Strokes"), tr("Reset and Clear All")];
    let skew_reset = section(sheet, tr("Skew Correction"), &menu, |sheet| {
        sheet.greyed(skew_applies, Some(skew_unavailable_reason(settings, has_high)), |sheet| {
            paint_button(app, sheet, BakePaintTarget::Skew);
            sheet.check(tr("Edge Protection"), &mut settings.skew.edge_protection);
            sheet.greyed(settings.skew.edge_protection, None, |sheet| {
                sheet.indented(|sheet| {
                    number(sheet, tr("Edge Distance"), &mut settings.skew.edge_distance, 0.0..=1.0, 2);
                    number(sheet, tr("Edge Contrast"), &mut settings.skew.edge_contrast, 0.0..=1.0, 2);
                });
            });
            choice(
                sheet,
                tr("Skew Correction Method"),
                &mut settings.skew.method,
                &[(SkewCorrectionMethod::MeshNormalsBased, tr("Mesh Normals Based")), (SkewCorrectionMethod::FaceNormalsBased, tr("Face Normals Based"))],
            );
        });
    });
    if let Some(index) = skew_reset {
        if index != 1 {
            settings.skew = Default::default();
            reset.insert("skew".into(), json!(settings.skew));
        }
        if index != 0 {
            app.run("bake.skew.clear", json!({}));
        }
    }

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
    } else if settings.cage.kind == CageType::Automatic {
        tr("Skew Correction needs a distance-based cage: the automatic cage is fitted to the high poly as it is.")
    } else {
        tr("Skew Correction needs Average Normals turned on in the cage settings.")
    }
}

/// The smallest output size offered, as in the reference; a project saved with a smaller one keeps it
const SMALLEST_OUTPUT_SIZE: u32 = 32;

/// Width, the link, then the height it ties to the width, as in the reference
fn output_size(sheet: &mut PropertySheet<'_>, settings: &mut CommonBakeSettings) {
    let sizes: Vec<BakeResolution> = BakeResolution::ALL.into_iter().filter(|resolution| resolution.pixels() >= SMALLEST_OUTPUT_SIZE).collect();
    let mut linked = settings.resolution_height.is_none();
    let height = settings.resolution_height.unwrap_or(settings.resolution);
    let picked = sheet.dropdown_pair_above(
        tr("Output Size"),
        [settings.resolution, height],
        &sizes,
        |size| size.pixels().to_string(),
        &mut linked,
        tr("Lock Output Size"),
    );
    match picked {
        Some((0, size)) => settings.resolution = size,
        Some((_, size)) => settings.resolution_height = Some(size),
        None => {}
    }
    if linked {
        settings.resolution_height = None;
    } else if settings.resolution_height.is_none() {
        settings.resolution_height = Some(settings.resolution);
    }
}

/// The high poly meshes, one row each with × to remove it, and + beside them to add some; files still loading show greyed
fn high_poly_scene(app: &mut TextureCraftApp, sheet: &mut PropertySheet<'_>) {
    let loading: Vec<String> = app.high_poly_job.iter().flat_map(|job| job.file_names()).map(|name| trf("Loading {name}...", &[("name", &name)])).collect();
    let meshes = &app.session.doc.bake_meshes.high;
    let rows: Vec<FileRow<'_>> = meshes
        .iter()
        .map(|mesh| FileRow { name: &mesh.name, folder: mesh.folder.as_deref(), loading: false })
        .chain(loading.iter().map(|name| FileRow { name, folder: None, loading: true }))
        .collect();
    let answer = sheet.file_list(tr("High Poly Scene"), &rows, HIGH_POLY_LIST_MIN_ROWS, icon::PLUS, tr("Add High-Poly Meshes"), tr("Remove"));
    if let Some(index) = answer.removed {
        app.run("bake.highPoly.remove", json!({ "index": index }));
    }
    if answer.add {
        crate::web_files::import_bake_mesh(app, &sheet.ui.ctx().clone(), false);
    }
}

/// The cage file's row, with the folder button that loads one
fn cage_file(app: &mut TextureCraftApp, sheet: &mut PropertySheet<'_>) {
    let cage = app.session.doc.bake_meshes.cage.as_ref();
    let rows: Vec<FileRow<'_>> = cage.iter().map(|mesh| FileRow { name: &mesh.name, folder: mesh.folder.as_deref(), loading: false }).collect();
    let answer = sheet.file_list(tr("Cage Scene"), &rows, 1, icon::FOLDER_OPEN, tr("Load Cage"), tr("Remove Cage"));
    if answer.removed.is_some() {
        app.run("bake.cage.clear", json!({}));
    }
    if answer.add {
        crate::web_files::import_bake_mesh(app, &sheet.ui.ctx().clone(), true);
    }
}

/// "Paint Cage" or "Paint Skew Correction", held down as "Stop Painting" while that brush is on, with the brush sliders under it
fn paint_button(app: &mut TextureCraftApp, sheet: &mut PropertySheet<'_>, target: BakePaintTarget) {
    let label = match target {
        BakePaintTarget::Cage => tr("Paint Cage"),
        BakePaintTarget::Skew => tr("Paint Skew Correction"),
    };
    let mut brush = app.ui.bake_paint_brush;
    let active = brush.on && brush.target == target;
    let (glyph, text) = if active { (icon::PROHIBIT, tr("Stop Painting")) } else { (icon::PENCIL_SIMPLE, label) };
    if sheet.wide_button(glyph, text, active).clicked() {
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
        sheet.indented(|sheet| {
            number(sheet, tr("Radius"), &mut brush.radius, 0.002..=0.2, 3);
            number(sheet, tr("Value"), &mut brush.value, 0.0..=1.0, 2);
            number(sheet, tr("Hardness"), &mut brush.hardness, 0.0..=1.0, 2);
        });
        if app.ui.layout == crate::state::ViewLayout::Only3D {
            app.ui.layout = crate::state::ViewLayout::Split;
        }
    }
    app.ui.bake_paint_brush = brush;
}
