//! A fill's Symmetry section: its switch, Type, Axis, Flip copy and the radial rows each change the fill's symmetry

use egui::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;
use texturecraft_engine::core::{FillSymmetry, LayerId, LayerKind, SymmetryKind};
use texturecraft_ui_egui::TextureCraftApp;

/// Tall enough that Properties shows the whole fill without scrolling
const WINDOW_SIZE: egui::Vec2 = egui::vec2(1600.0, 1400.0);

/// Small texture and bake sizes keep the sample session fast; the panel doesn't depend on them
const TEXTURE_RESOLUTION: u32 = 256;
const BAKE_RESOLUTION: u32 = 64;

/// Frames for the first layout and fonts to settle
const STARTUP_FRAMES: usize = 4;

/// Frames for a click or selection change to reach the panel
const SETTLE_FRAMES: usize = 3;

/// The app with a new fill layer (projected as `projection`) selected, its Symmetry tab clicked, and its layer id
fn app_with_fill(projection: &str) -> (Harness<'static, TextureCraftApp>, u64) {
    let mut session = texturecraft_engine::sample_session(TEXTURE_RESOLUTION, BAKE_RESOLUTION).unwrap();
    let added = texturecraft_engine::execute(&mut session, "layer.add.fill", &json!({})).unwrap();
    let layer = added["layer"].as_u64().unwrap();
    texturecraft_engine::execute(&mut session, "layer.fill.projection", &json!({"layer": layer, "projection": projection})).unwrap();

    let mut harness = Harness::builder().with_size(WINDOW_SIZE).build_eframe(|cc| TextureCraftApp::new(cc, session));
    // The labels below are English; a first run would speak the system's language
    harness.state_mut().ui.prefs.language = "en".into();
    harness.run_steps(STARTUP_FRAMES);
    harness.get_by_role_and_label(Role::Tab, "Symmetry").click();
    harness.run_steps(SETTLE_FRAMES);
    (harness, layer)
}

/// The fill layer's symmetry as the document holds it
fn symmetry(harness: &Harness<'_, TextureCraftApp>, layer: u64) -> FillSymmetry {
    match &harness.state().session.doc.project.layer(LayerId(layer)).unwrap().kind {
        LayerKind::Fill(fill) => fill.sampling.symmetry,
        other => panic!("not a fill: {other:?}"),
    }
}

fn click(harness: &mut Harness<'_, TextureCraftApp>, role: Role, label: &str) {
    harness.get_by_role_and_label(role, label).click();
    harness.run_steps(SETTLE_FRAMES);
}

#[test]
fn the_symmetry_switch_shows_its_rows_and_each_row_changes_the_symmetry() {
    let (mut harness, layer) = app_with_fill("planar");
    assert!(harness.query_by_label("Mirror symmetry").is_none(), "off, the section is the caption alone");

    click(&mut harness, Role::CheckBox, "Symmetry");
    assert!(symmetry(&harness, layer).on);
    assert!(harness.query_by_label("Axis position X").is_some(), "a mirror across X moves along X");
    assert!(harness.query_by_label("Count").is_none(), "a mirror has no radial rows");

    click(&mut harness, Role::RadioButton, "Y");
    assert_eq!(symmetry(&harness, layer).axis, 1);
    click(&mut harness, Role::CheckBox, "Flip copy U");
    assert_eq!(symmetry(&harness, layer).flip, [true, false]);

    click(&mut harness, Role::ComboBox, "Mirror symmetry");
    click(&mut harness, Role::ListBoxOption, "Radial symmetry");
    assert_eq!(symmetry(&harness, layer).kind, SymmetryKind::Radial);
    for label in ["Count", "Angle span", "Axis position X", "Axis position Z"] {
        assert!(harness.query_by_label(label).is_some(), "radial round Y: {label}");
    }
    assert!(harness.query_by_label("Axis position Y").is_none(), "the turning axis does not move along itself");

    click(&mut harness, Role::CheckBox, "Symmetry");
    assert!(!symmetry(&harness, layer).on);
    assert_eq!(symmetry(&harness, layer).kind, SymmetryKind::Radial, "switched off, the settings stay");
}

#[test]
fn a_uv_fill_says_why_in_place_of_the_symmetry_switch() {
    let (mut harness, layer) = app_with_fill("uv");
    assert!(harness.query_by_role_and_label(Role::CheckBox, "Symmetry").is_none(), "no switch to click");
    harness.get_by_label("Not available with UV projection").click();
    harness.run_steps(SETTLE_FRAMES);
    assert!(!symmetry(&harness, layer).on, "a UV projection has no symmetry to switch on");
    assert!(harness.query_by_label("Type").is_none(), "its settings stay hidden, not greyed");
}
