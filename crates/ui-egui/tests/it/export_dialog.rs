//! The Export textures dialog: it fits the app window, Esc closes it, and its notes say what each setting writes

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable};
use texturecraft_engine::core::NormalFormat;
use texturecraft_ui_egui::TextureCraftApp;

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
    assert_eq!(harness.state().ui.export.skip.len(), MANY_SETS, "every set is left out");
    harness.get_by_label("Select all").click();
    harness.run_steps(2);
    assert!(harness.state().ui.export.skip.is_empty(), "every set is exported");
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
    let app = harness.state_mut();
    app.ui.export.preset = preset.into();
    app.ui.export.normal_format = chosen.into();
    app.session.doc.project.normal_format = project;
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
