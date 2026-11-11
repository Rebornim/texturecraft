//! Export settings saved in the project, and each texture set's options from them: global, then the set's overrides, then its maps'

use std::collections::BTreeMap;

use serde_json::{Value, json};
use texturecraft_core::{ExportFileType, ExportSettings, MapExportOverride, TextureSetExport};
use texturecraft_formats::{ExportOptions, FileFormat};

use crate::commands::{CommandSpec, always, bad};
use crate::file_cmds::{export_options, sets_param};
use crate::{Result, Session, cmd};

/// Export command parameters that set options for every texture set, bypassing the saved settings
const EXPLICIT_OPTIONS: [&str; 9] = ["preset", "format", "bitDepth", "padding", "dilation", "size", "dithering", "normalFormat", "colorSpace"];

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "export.settings.get",
            "Get the export settings",
            [],
            None,
            "{} → {saved: bool, settings: {dir, parameters: {preset, format, bit_depth, size, padding, dilation}, normal_format, color_space}, sets: [{name, export: {skip, overrides, maps}}]}",
            always,
            get
        ),
        cmd!(
            "export.settings.set",
            "Save export settings",
            [],
            None,
            "{settings?: as export.settings.get, sets?: [{skip?, overrides?: {preset?, file_type?: [format, bitDepth], size?, padding?: [mode, px]}, maps?: {<map key: file template with $channel filled in>: {skip?, file_type?}}}] one per texture set} → the saved settings. file.exportTextures and export.plan use them when called without preset, format, bitDepth, padding, dilation, size, dithering, normalFormat or colorSpace, and also take `settings` / `sets` there to use unsaved ones",
            always,
            set
        ),
    ]
}

fn get(s: &mut Session, _: &Value) -> Result<Value> {
    let project = &s.doc.project;
    let sets: Vec<Value> = project.texture_sets.iter().map(|set| json!({"name": set.name, "export": set.export})).collect();

    Ok(json!({"saved": project.export.is_some(), "settings": project.export.clone().unwrap_or_default(), "sets": sets}))
}

/// The `settings` parameter, if given
fn settings_param(p: &Value, cmd: &str) -> Result<Option<ExportSettings>> {
    match p.get("settings") {
        None | Some(Value::Null) => Ok(None),
        Some(v) => serde_json::from_value(v.clone()).map(Some).map_err(|e| bad(cmd, format!("invalid `settings`: {e}"))),
    }
}

/// The `sets` parameter, if given: one entry per texture set
fn sets_export_param(s: &Session, p: &Value, cmd: &str) -> Result<Option<Vec<TextureSetExport>>> {
    let sets: Vec<TextureSetExport> = match p.get("sets") {
        None | Some(Value::Null) => return Ok(None),
        Some(v) => serde_json::from_value(v.clone()).map_err(|e| bad(cmd, format!("invalid `sets`: {e}")))?,
    };
    if sets.len() != s.set_count() {
        return Err(bad(cmd, format!("`sets` has {} entries for {} texture sets", sets.len(), s.set_count())));
    }

    Ok(Some(sets))
}

fn set(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "export.settings.set";
    let settings = settings_param(p, CMD)?;
    let sets = sets_export_param(s, p, CMD)?;

    s.edit("Save export settings", |s| {
        let project = &mut s.doc.project;
        project.export = Some(settings.unwrap_or_else(|| project.export.clone().unwrap_or_default()));
        for (texture_set, export) in project.texture_sets.iter_mut().zip(sets.unwrap_or_default()) {
            texture_set.export = export;
        }

        // Every set must resolve, so a bad template name or size is refused, not saved
        let settings = s.doc.project.export.clone().unwrap_or_default();
        for set in &s.doc.project.texture_sets {
            set_export_options(s, &settings, &set.export, CMD)?;
        }
        Ok(())
    })?;

    get(s, &Value::Null)
}

/// Whether an export call also writes the shader parameters: `p`'s `shaderParameters`, else the settings' (none with explicit options)
pub fn writes_shader_parameters(s: &Session, p: &Value, cmd: &str) -> Result<bool> {
    writes_extra(s, p, cmd, "shaderParameters", |settings| settings.shader_parameters)
}

/// Whether an export call also writes a USD asset: `p`'s `usd`, else the settings' (none with explicit options)
pub fn writes_usd(s: &Session, p: &Value, cmd: &str) -> Result<bool> {
    writes_extra(s, p, cmd, "usd", |settings| settings.usd)
}

fn writes_extra(s: &Session, p: &Value, cmd: &str, key: &str, setting: fn(&ExportSettings) -> bool) -> Result<bool> {
    if let Some(asked) = crate::commands::boolp(p, key) {
        return Ok(asked);
    }
    if has_explicit_options(p) {
        return Ok(false);
    }

    let settings = settings_param(p, cmd)?.or_else(|| s.doc.project.export.clone()).unwrap_or_default();
    Ok(setting(&settings))
}

/// Whether `p` sets export options itself instead of using the saved settings
pub fn has_explicit_options(p: &Value) -> bool {
    EXPLICIT_OPTIONS.iter().any(|key| p.get(key).is_some_and(|v| !v.is_null()))
}

/// What an export call writes: one job for all sets when `p` sets options, else one per set from `p`'s `settings` / `sets` or the project's
pub fn export_jobs(s: &Session, p: &Value, cmd: &str) -> Result<Vec<(Vec<usize>, ExportOptions)>> {
    if has_explicit_options(p) {
        return Ok(vec![(sets_param(s, p, cmd)?, export_options(s, p, cmd)?)]);
    }

    let settings = match settings_param(p, cmd)? {
        Some(settings) => settings,
        None => s.doc.project.export.clone().unwrap_or_default(),
    };
    let sets_export = match sets_export_param(s, p, cmd)? {
        Some(sets) => sets,
        None => s.doc.project.texture_sets.iter().map(|set| set.export.clone()).collect(),
    };
    let sets = match p.get("ts") {
        Some(_) => sets_param(s, p, cmd)?,
        None => sets_export.iter().enumerate().filter(|(_, export)| !export.skip).map(|(ts, _)| ts).collect(),
    };

    let mut jobs = Vec::with_capacity(sets.len());
    for ts in sets {
        let export = sets_export.get(ts).ok_or_else(|| bad(cmd, format!("no texture set {ts}")))?;
        jobs.push((vec![ts], set_export_options(s, &settings, export, cmd)?));
    }

    Ok(jobs)
}

/// The options a texture set exports with: `settings`, then its overrides and its maps'
pub fn set_export_options(s: &Session, settings: &ExportSettings, export: &TextureSetExport, cmd: &str) -> Result<ExportOptions> {
    let parameters = export.overrides.apply(&settings.parameters);
    let mut p = json!({"preset": parameters.preset, "padding": parameters.padding, "dilation": parameters.dilation});
    if parameters.size > 0 {
        p["size"] = json!(parameters.size);
    }
    if !settings.normal_format.is_empty() {
        p["normalFormat"] = json!(settings.normal_format);
    }
    if !settings.color_space.is_empty() {
        p["colorSpace"] = json!(settings.color_space);
    }
    let mut options = export_options(s, &p, cmd)?;

    // File types go into the template's own maps, so a map's override wins over the set's file type
    apply_file_types(&mut options, &parameters.file_type, &export.maps, cmd)?;

    Ok(options)
}

/// None for an empty extension (the template's formats)
fn file_format(extension: &str, cmd: &str) -> Result<Option<FileFormat>> {
    if extension.is_empty() {
        return Ok(None);
    }

    FileFormat::from_extension(extension).map(Some).ok_or_else(|| bad(cmd, format!("unknown format `{extension}`")))
}

/// Drop the maps left out and write each map's file type: its own override, else the set's, else the template's
fn apply_file_types(options: &mut ExportOptions, set_type: &ExportFileType, maps: &BTreeMap<String, MapExportOverride>, cmd: &str) -> Result<()> {
    let set_format = file_format(&set_type.format, cmd)?;
    options.preset.maps.retain(|map| !maps.get(&map.key()).is_some_and(|map_override| map_override.skip));
    for map in &mut options.preset.maps {
        let (format, bit_depth, dithering) = match maps.get(&map.key()).and_then(|map_override| map_override.file_type.as_ref()) {
            Some(own) => (file_format(&own.format, cmd)?.unwrap_or(map.format), own.bit_depth, own.dithering),
            // "Based on output template" keeps the template's own dithering
            None if set_format.is_none() => (map.format, if set_type.bit_depth > 0 { set_type.bit_depth } else { map.bit_depth }, map.options.dithering),
            None => (set_format.unwrap_or(map.format), if set_type.bit_depth > 0 { set_type.bit_depth } else { map.bit_depth }, set_type.dithering),
        };
        map.format = format;
        map.bit_depth = format.depth(bit_depth);
        map.options.dithering = dithering && map.bit_depth == 8;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use texturecraft_core::{ExportOverrides, ExportParameters};

    use super::*;
    use crate::execute;

    fn session() -> Session {
        crate::sample_session(128, 32).unwrap()
    }

    fn planned(s: &mut Session) -> Vec<(u64, String, String, u64)> {
        let plan = execute(s, "export.plan", &json!({})).unwrap();
        plan.as_array()
            .unwrap()
            .iter()
            .map(|f| {
                (f["ts"].as_u64().unwrap(), f["file"].as_str().unwrap().to_string(), f["format"].as_str().unwrap().to_string(), f["bitDepth"].as_u64().unwrap())
            })
            .collect()
    }

    #[test]
    fn a_project_without_saved_settings_exports_as_before() {
        let mut s = session();
        assert!(!execute(&mut s, "export.settings.get", &json!({})).unwrap()["saved"].as_bool().unwrap());
        let explicit = export_options(&s, &json!({"preset": "Document channels + Normal + AO"}), "test").unwrap();
        let before = s.export_files(&[0, 1], &explicit).unwrap();

        let mut after = Vec::new();
        for (sets, options) in export_jobs(&s, &json!({}), "test").unwrap() {
            after.extend(s.export_files(&sets, &options).unwrap());
        }
        assert_eq!(before.len(), after.len());
        for (old, new) in before.iter().zip(&after) {
            assert_eq!((&old.name, &old.bytes), (&new.name, &new.bytes), "same file, same bytes");
        }
    }

    #[test]
    fn a_set_override_changes_only_that_set() {
        let mut s = session();
        let mut overridden = TextureSetExport::default();
        overridden.overrides = ExportOverrides { file_type: Some(ExportFileType::new("tga", 8)), ..Default::default() };
        let sets = json!([overridden, TextureSetExport::default()]);
        execute(&mut s, "export.settings.set", &json!({"sets": sets})).unwrap();

        let plan = planned(&mut s);
        assert!(plan.iter().filter(|f| f.0 == 0).all(|f| f.2 == "tga"), "set 0 writes its own file type: {plan:?}");
        assert!(plan.iter().filter(|f| f.0 == 1).all(|f| f.2 == "png"), "set 1 keeps the template's: {plan:?}");
    }

    #[test]
    fn a_map_override_wins_over_the_set_and_a_skipped_map_is_not_written() {
        let mut s = session();
        let preset = s.export_preset("Document channels + Normal + AO").unwrap();
        let (kept, skipped) = (preset.maps[0].key(), preset.maps[1].key());
        let mut export = TextureSetExport::default();
        export.overrides.file_type = Some(ExportFileType::new("tga", 8));
        export.maps.insert(kept.clone(), MapExportOverride { skip: false, file_type: Some(ExportFileType::new("png", 16)) });
        export.maps.insert(skipped, MapExportOverride { skip: true, file_type: None });
        execute(&mut s, "export.settings.set", &json!({"sets": [export, TextureSetExport::default()]})).unwrap();

        let set_zero: Vec<_> = planned(&mut s).into_iter().filter(|f| f.0 == 0).collect();
        let with_explicit_maps = execute(&mut s, "export.plan", &json!({"ts": 0, "preset": "Document channels + Normal + AO"})).unwrap();
        assert_eq!(set_zero.len() + 1, with_explicit_maps.as_array().unwrap().len(), "one map is left out");
        assert_eq!(set_zero.iter().filter(|f| f.2 == "png" && f.3 == 16).count(), 1, "the map's own type wins: {set_zero:?}");
    }

    #[test]
    fn unsaved_settings_plan_without_changing_the_project() {
        let mut s = session();
        let settings = ExportSettings { parameters: ExportParameters { file_type: ExportFileType::new("tga", 0), ..Default::default() }, ..Default::default() };
        let plan = execute(&mut s, "export.plan", &json!({"settings": settings})).unwrap();
        assert!(plan.as_array().unwrap().iter().all(|f| f["format"] == "tga"));
        assert!(s.doc.project.export.is_none());
        assert!(s.undo_label().is_none(), "planning is not an edit");
    }

    #[test]
    fn dithering_reaches_the_8_bit_maps_it_was_asked_for() {
        let s = session();
        let preset = s.export_preset("Document channels + Normal + AO").unwrap();
        let height = preset.maps.iter().position(|map| map.key().ends_with("height")).unwrap();
        let mut export = TextureSetExport::default();
        export.maps.insert(
            preset.maps[0].key(),
            MapExportOverride { skip: false, file_type: Some(ExportFileType { format: "png".into(), bit_depth: 8, dithering: true }) },
        );
        export.maps.insert(
            preset.maps[height].key(),
            MapExportOverride { skip: false, file_type: Some(ExportFileType { format: "png".into(), bit_depth: 16, dithering: true }) },
        );

        let dithered = |options: &ExportOptions| options.preset.maps.iter().map(|map| map.options.dithering).collect::<Vec<_>>();
        let options = set_export_options(&s, &ExportSettings::default(), &export, "test").unwrap();
        assert!(dithered(&options)[0] && !dithered(&options)[height], "16-bit files aren't dithered");

        let png = ExportFileType { format: "png".into(), bit_depth: 8, dithering: true };
        let settings = ExportSettings { parameters: ExportParameters { file_type: png, ..Default::default() }, ..Default::default() };
        let everywhere = set_export_options(&s, &settings, &TextureSetExport::default(), "test").unwrap();
        assert!(dithered(&everywhere).iter().all(|d| *d), "a set's dithered 8-bit png applies to every map");
        let template = set_export_options(&s, &ExportSettings::default(), &TextureSetExport::default(), "test").unwrap();
        assert!(dithered(&template).iter().all(|d| !*d), "based on the template: the template's own dithering, none here");
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn saved_settings_export_to_the_saved_folder_with_the_shader_parameters() {
        let mut s = session();
        let dir = std::env::temp_dir().join(format!("tc_saved_export_settings_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let settings = ExportSettings { dir: dir.display().to_string(), shader_parameters: true, ..Default::default() };
        execute(&mut s, "export.settings.set", &json!({"settings": settings})).unwrap();

        let result = execute(&mut s, "file.exportTextures", &json!({})).unwrap();
        let json = dir.join("Material_Ball_shaders.json");
        assert!(result["files"].as_array().unwrap().iter().any(|file| file.as_str().unwrap().ends_with("Material_Ball_shaders.json")));
        let written: Value = serde_json::from_str(&std::fs::read_to_string(&json).unwrap()).unwrap();
        assert_eq!(written["shaders"][0]["textureSets"], json!(["Ball", "Base"]));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn the_usd_asset_reads_the_channels_the_template_packed() {
        let mut s = session();
        let dir = std::env::temp_dir().join(format!("tc_usd_asset_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let settings = ExportSettings {
            dir: dir.display().to_string(),
            parameters: ExportParameters { preset: "Unreal Engine (Packed)".into(), ..Default::default() },
            usd: true,
            ..Default::default()
        };
        execute(&mut s, "export.settings.set", &json!({"settings": settings})).unwrap();
        execute(&mut s, "file.exportTextures", &json!({})).unwrap();

        let layer = std::fs::read_to_string(dir.join("Material_Ball.usda")).unwrap();
        let shader = |name: &str| layer.split(&format!("def Shader \"{name}\"")).nth(1).unwrap().split("def ").next().unwrap().to_string();
        let roughness = shader("Roughness");
        assert!(roughness.contains("OcclusionRoughnessMetallic.png") && roughness.contains("outputs:g"), "{roughness}");
        assert!(shader("Metallic").contains("outputs:b"));
        assert!(shader("BaseColor").contains("outputs:rgb") && shader("BaseColor").contains("sRGB"));
        assert!(shader("Normal").contains("(2, -2, 2, 1)"), "the DirectX normal's green is flipped back: {}", shader("Normal"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn skipped_sets_are_not_exported_and_explicit_options_ignore_the_saved_ones() {
        let mut s = session();
        let left_out = TextureSetExport { skip: true, ..Default::default() };
        let settings = ExportSettings { parameters: ExportParameters { file_type: ExportFileType::new("tga", 0), ..Default::default() }, ..Default::default() };
        execute(&mut s, "export.settings.set", &json!({"settings": settings, "sets": [left_out, TextureSetExport::default()]})).unwrap();

        let plan = planned(&mut s);
        assert!(plan.iter().all(|f| f.0 == 1 && f.2 == "tga"), "{plan:?}");
        let explicit = execute(&mut s, "export.plan", &json!({"preset": "Document channels + Normal + AO"})).unwrap();
        assert!(explicit.as_array().unwrap().iter().any(|f| f["ts"] == 0 && f["format"] == "png"));
    }

    #[test]
    fn saving_is_one_undo_step_and_bad_settings_are_refused() {
        let mut s = session();
        let unknown = ExportSettings { parameters: ExportParameters { preset: "No such template".into(), ..Default::default() }, ..Default::default() };
        assert!(execute(&mut s, "export.settings.set", &json!({"settings": unknown})).is_err());
        assert!(s.doc.project.export.is_none(), "a refused save leaves the project as it was");
        assert!(execute(&mut s, "export.settings.set", &json!({"sets": []})).is_err(), "one entry per texture set");

        execute(&mut s, "export.settings.set", &json!({"settings": ExportSettings::default()})).unwrap();
        assert!(s.doc.project.export.is_some());
        execute(&mut s, "edit.undo", &json!({})).unwrap();
        assert!(s.doc.project.export.is_none());
    }

    #[test]
    fn saved_settings_survive_a_project_round_trip() {
        let mut s = session();
        let mut export = TextureSetExport::default();
        export.overrides.size = Some(512);
        let settings = ExportSettings { dir: "/tmp/out".into(), ..Default::default() };
        execute(&mut s, "export.settings.set", &json!({"settings": settings, "sets": [export.clone(), TextureSetExport::default()]})).unwrap();

        let text = serde_json::to_string(&s.doc.project).unwrap();
        let back: texturecraft_core::Project = serde_json::from_str(&text).unwrap();
        assert_eq!(back.export.unwrap().dir, "/tmp/out");
        assert_eq!(back.texture_sets[0].export, export);
    }
}
