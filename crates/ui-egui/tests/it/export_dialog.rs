//! The Export textures dialog: it fits the app window, Esc closes it, and its notes say what each setting writes

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable};
use texturecraft_engine::core::NormalFormat;
use texturecraft_ui_egui::TextureCraftApp;
use texturecraft_ui_egui::export_draft::ExportDraft;

const MANY_SETS: usize = 80;
const LAPTOP: egui::Vec2 = egui::vec2(1280.0, 720.0);

/// The sample project with `set_count` texture sets in an app window of `size`, Export textures open
fn harness(set_count: usize, size: egui::Vec2) -> Harness<'static, TextureCraftApp> {
    let mut session = texturecraft_engine::sample_session(256, 64).unwrap();
    let template = session.doc.project.texture_sets[0].clone();
    while session.doc.project.texture_sets.len() < set_count {
        let mut set = template.clone();
        set.name = format!("Set {}", session.doc.project.texture_sets.len() + 1);
        session.doc.project.texture_sets.push(set);
    }
    let mut harness = Harness::builder().with_size(size).build_eframe(|cc| TextureCraftApp::new(cc, session));
    harness.state_mut().ui.windows.export = true;
    harness.run_steps(3);
    harness
}

/// The open window's unsaved edits
fn draft<'a>(harness: &'a mut Harness<'static, TextureCraftApp>) -> &'a mut ExportDraft {
    harness.state_mut().ui.export.settings.as_mut().unwrap()
}

fn exported_sets(harness: &mut Harness<'static, TextureCraftApp>) -> usize {
    draft(harness).sets.iter().filter(|set| !set.skip).count()
}

#[test]
fn many_texture_sets_keep_the_dialog_inside_the_window() {
    for size in [LAPTOP, egui::vec2(1280.0, 480.0)] {
        let harness = harness(MANY_SETS, size);
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
        let dialog = harness.get_all_by_role(Role::Window).find(|node| node.accesskit_node().label().as_deref() == Some("Export textures")).unwrap();
        assert!(screen.contains_rect(dialog.rect()), "the dialog {:?} leaves the {size:?} window", dialog.rect());
        for label in ["Export", "Cancel", "Reset all"] {
            let button = harness.get_all_by_label(label).find(|node| node.accesskit_node().role() == Role::Button).unwrap();
            assert!(dialog.rect().contains_rect(button.rect()), "{label} {:?} is outside the dialog {:?}", button.rect(), dialog.rect());
        }
    }
}

#[test]
fn the_set_list_selects_all_or_none() {
    let mut harness = harness(MANY_SETS, LAPTOP);
    harness.get_by_label("Select none").click();
    harness.run_steps(2);
    assert_eq!(exported_sets(&mut harness), 0, "every set is left out");
    harness.get_by_label("Select all").click();
    harness.run_steps(2);
    assert_eq!(exported_sets(&mut harness), MANY_SETS, "every set is exported");
}

#[test]
fn escape_closes_the_dialog() {
    let mut harness = harness(2, LAPTOP);
    harness.key_press(egui::Key::Escape);
    harness.run_steps(2);
    assert!(!harness.state().ui.windows.export);
}

/// The value shown in the dialog's Normal map format combo
fn normal_format_shown(harness: &mut Harness<'static, TextureCraftApp>, preset: &str, project: NormalFormat, chosen: &str) -> String {
    let draft = draft(harness);
    draft.settings.parameters.preset = preset.into();
    draft.settings.normal_format = chosen.into();
    harness.state_mut().session.doc.project.normal_format = project;
    harness.run_steps(2);
    let row = harness.get_by_label("Normal map format").rect().center().y;
    let combo = harness.get_all_by_role(Role::ComboBox).find(|node| (node.rect().center().y - row).abs() < 4.0).unwrap();
    combo.value().unwrap_or_default()
}

#[test]
fn the_normal_format_follows_the_preset_visibly() {
    let mut harness = harness(2, LAPTOP);
    let unreal = "Unreal Engine (Packed)";
    let document = "Document channels + Normal + AO";
    assert_eq!(normal_format_shown(&mut harness, unreal, NormalFormat::OpenGl, ""), "From template / project (DirectX)");
    assert_eq!(normal_format_shown(&mut harness, document, NormalFormat::OpenGl, ""), "From template / project (OpenGL)");
    assert_eq!(normal_format_shown(&mut harness, document, NormalFormat::DirectX, ""), "From template / project (DirectX)");
    assert_eq!(normal_format_shown(&mut harness, unreal, NormalFormat::OpenGl, "openGl"), "OpenGL", "the override wins");
}

#[test]
fn the_color_space_says_data_maps_stay_linear() {
    let harness = harness(2, LAPTOP);
    harness.get_by_label("Color maps only: normal, roughness, metallic and other data maps stay linear");
}

#[test]
fn save_settings_writes_the_project_and_cancel_drops_the_edits() {
    let mut harness = harness(2, LAPTOP);
    draft(&mut harness).settings.parameters.format = "tga".into();
    harness.get_by_label("Save settings").click();
    harness.run_steps(2);
    let saved = harness.state().session.doc.project.export.clone().unwrap();
    assert_eq!(saved.parameters.format, "tga");
    assert!(harness.state().ui.windows.export, "Save settings keeps the window open");

    draft(&mut harness).settings.parameters.format = "png".into();
    harness.get_by_label("Cancel").click();
    harness.run_steps(2);
    assert!(harness.state().ui.export.settings.is_none(), "the edits are dropped");
    assert_eq!(harness.state().session.doc.project.export.as_ref().unwrap().parameters.format, "tga");

    harness.state_mut().ui.windows.export = true;
    harness.run_steps(2);
    assert_eq!(draft(&mut harness).settings.parameters.format, "tga", "the window reopens on the saved settings");
}

#[test]
fn the_bottom_bar_counts_the_textures_to_export() {
    let mut harness = harness(2, LAPTOP);
    let plan = texturecraft_engine::execute(&mut harness.state_mut().session, "export.plan", &serde_json::json!({})).unwrap();
    let all = plan.as_array().unwrap().len();
    harness.get_by_label(&format!("{all} textures selected for export"));

    draft(&mut harness).sets[1].skip = true;
    harness.run_steps(2);
    let one_set = plan.as_array().unwrap().iter().filter(|file| file["ts"] == 0).count();
    harness.get_by_label(&format!("{one_set} textures selected for export"));
}

/// The pen on the row labelled `parameter` in a texture set's settings
fn pen<'a>(harness: &'a Harness<'static, TextureCraftApp>, parameter: &str) -> egui_kittest::Node<'a> {
    let row = harness.get_all_by_label(parameter).last().unwrap().rect().center().y;
    harness.get_all_by_label("Override for this texture set").find(|node| (node.rect().center().y - row).abs() < 6.0).unwrap()
}

#[test]
fn a_texture_set_overrides_a_parameter_with_its_pen() {
    let mut harness = harness(2, LAPTOP);
    harness.get_by_label("Base").click();
    harness.run_steps(2);
    assert_eq!(draft(&mut harness).selected, Some(1));
    assert!(harness.query_by_label("Based on each texture set's size").is_none(), "a set shows its own size, not the global choice");
    harness.get_by_label("256x256 (texture set size)");

    pen(&harness, "Size").click();
    harness.run_steps(2);
    assert_eq!(draft(&mut harness).sets[1].overrides.size, Some(0), "the override starts from the global value");
    assert!(harness.get_all_by_label("*").next().is_some(), "the set's row marks the override");

    pen(&harness, "Size").click();
    harness.run_steps(2);
    assert_eq!(draft(&mut harness).sets[1].overrides.size, None, "a second click goes back to the global value");
    assert!(harness.query_all_by_label("*").next().is_none(), "the marker goes with the override");
}

#[test]
fn the_section_menu_overrides_every_parameter_or_none() {
    let mut harness = harness(2, LAPTOP);
    harness.get_by_label("Base").click();
    harness.run_steps(2);
    for (item, overridden) in [("Enable override for all items", true), ("Disable override for all items", false)] {
        harness.get_by_label(egui_phosphor::regular::LIST).click();
        harness.run_steps(2);
        harness.get_by_label(item).click();
        harness.run_steps(2);
        let overrides = draft(&mut harness).sets[1].overrides.clone();
        let all = [overrides.preset.is_some(), overrides.file_type.is_some(), overrides.size.is_some(), overrides.padding.is_some()];
        assert_eq!(all, [overridden; 4], "{item}");
    }
}
