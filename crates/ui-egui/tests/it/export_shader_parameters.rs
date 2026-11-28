//! Export textures › SETTINGS: Export shader settings writes `<project>_shaders.json`, the
//! shader's settings and every texture set of the export, with the export's last texture set

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use texturecraft_ui_egui::TextureCraftApp;

use super::temp_dir::TempDir;

#[test]
fn ticking_export_shader_settings_writes_the_json_listing_every_exported_set() {
    let session = texturecraft_engine::sample_session(256, 64).unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1600.0, 960.0))
        .with_context_setup(texturecraft_ui_egui::framework::install_headless)
        .build_eframe(|cc| TextureCraftApp::new(cc, session));
    h.state_mut().ui.prefs.language = "en".into();
    h.run_steps(4);
    let dir = TempDir::new("export-shader-parameters");
    h.state_mut().ui.export.dir = dir.display().to_string();
    h.state_mut().ui.windows.export = true;
    h.run_steps(4);

    h.get_by_role_and_label(egui::Role::CheckBox, "Export shader settings").click();
    h.run_steps(4);

    assert!(h.state().ui.export.shader_parameters);
    h.get_all_by_label("Export").last().unwrap().click();
    for _ in 0..64 {
        h.run_steps(1);
        if h.state().ui.export.job.is_none() {
            break;
        }
    }
    h.run_steps(2);

    let project = &h.state().session.doc.project;
    let file = dir.join(texturecraft_engine::shader_parameters::shader_parameters_file(&project.name));
    let json: serde_json::Value = serde_json::from_slice(&std::fs::read(&file).unwrap_or_else(|_| panic!("{} was written", file.display()))).unwrap();
    let names: Vec<&str> = project.texture_sets.iter().map(|set| set.name.as_str()).collect();

    assert_eq!(json["shaders"][0]["textureSets"], serde_json::json!(names), "every texture set of the export, written once: {json}");
    assert!(json["shaders"][0]["parameters"]["ior"].is_number());
}
