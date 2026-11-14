//! The docked Properties list keeps its scroll place per layer: selecting another layer and coming back lands where it was

use egui::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::json;
use texturecraft_engine::execute;
use texturecraft_ui_egui::TextureCraftApp;

/// Short enough that a fill's Properties scrolls
const WINDOW_SIZE: egui::Vec2 = egui::vec2(1600.0, 1000.0);

/// Small texture and bake sizes keep the sample session fast; the panel doesn't depend on them
const TEXTURE_RESOLUTION: u32 = 256;
const BAKE_RESOLUTION: u32 = 64;

/// Frames for the first layout and fonts to settle
const STARTUP_FRAMES: usize = 4;

/// Frames for a click, a selection change or a jump to reach the panel
const SETTLE_FRAMES: usize = 4;

/// The app with two UV fills ("Fill A", "Fill B") and a folder ("Folder", a short panel), Fill A selected
fn harness() -> Harness<'static, TextureCraftApp> {
    let mut session = texturecraft_engine::sample_session(TEXTURE_RESOLUTION, BAKE_RESOLUTION).unwrap();
    execute(&mut session, "layer.add.group", &json!({"name": "Folder"})).unwrap();
    for name in ["Fill B", "Fill A"] {
        let layer = execute(&mut session, "layer.add.fill", &json!({})).unwrap()["layer"].as_u64().unwrap();
        execute(&mut session, "layer.rename", &json!({"layer": layer, "name": name})).unwrap();
        execute(&mut session, "layer.fill.projection", &json!({"layer": layer, "projection": "uv"})).unwrap();
    }

    let mut harness = Harness::builder().with_size(WINDOW_SIZE).build_eframe(|cc| TextureCraftApp::new(cc, session));

    // The labels below are English; a first run would speak the system's language
    harness.state_mut().ui.prefs.language = "en".into();
    harness.run_steps(STARTUP_FRAMES);
    harness
}

/// A press and release at `at`, as the pointer does
fn click_at(harness: &mut Harness<'_, TextureCraftApp>, at: egui::Pos2) {
    harness.drag_at(at);
    harness.drop_at(at);
    harness.run_steps(SETTLE_FRAMES);
}

/// Selects the Layers panel row labelled `name` by clicking it
fn select_row(harness: &mut Harness<'_, TextureCraftApp>, name: &str) {
    let at = harness.get_by_label(name).rect().center();
    click_at(harness, at);
}

/// Whether the docked jump tab `name` is the lit one, i.e. its section is in view
fn tab_lit(harness: &Harness<'_, TextureCraftApp>, name: &str) -> bool {
    harness.get_by_role_and_label(Role::Tab, name).accesskit_node().toggled() == Some(egui::accesskit::Toggled::True)
}

#[test]
fn coming_back_to_a_layer_lands_where_its_properties_were_scrolled() {
    let mut harness = harness();
    select_row(&mut harness, "Fill A");
    assert!(tab_lit(&harness, "Fill"), "a fill never shown starts at the top");

    let material = harness.get_by_role_and_label(Role::Tab, "Material").rect().center();
    click_at(&mut harness, material);
    assert!(tab_lit(&harness, "Material"), "the jump scrolled down to MATERIAL");

    select_row(&mut harness, "Folder");
    assert!(harness.query_by_role_and_label(Role::Tab, "Material").is_none(), "a folder shows no fill tabs");

    select_row(&mut harness, "Fill A");
    assert!(tab_lit(&harness, "Material"), "Fill A is back where it was scrolled");
    assert!(!tab_lit(&harness, "Fill"));

    select_row(&mut harness, "Fill B");
    assert!(tab_lit(&harness, "Fill"), "another fill keeps its own place: the top");
}
