//! Every docked panel group sits under one header bar: its name at the left, a close × at the right

use egui::{Event, Modifiers, PointerButton, Pos2, Role};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use texturecraft_ui_egui::TextureCraftApp;
use texturecraft_ui_egui::docking::Panel;

use crate::{BAKE_RESOLUTION, SETTLE_FRAMES, STARTUP_FRAMES, TEXTURE_RESOLUTION};

const WINDOW_SIZE: egui::Vec2 = egui::vec2(1600.0, 960.0);

fn app() -> Harness<'static, TextureCraftApp> {
    let session = texturecraft_engine::sample_session(TEXTURE_RESOLUTION, BAKE_RESOLUTION).unwrap();
    let mut harness = Harness::builder().with_size(WINDOW_SIZE).build_eframe(|cc| TextureCraftApp::new(cc, session));

    // The labels below are English; a first run would speak the system's language
    harness.state_mut().ui.prefs.language = "en".into();
    harness.run_steps(STARTUP_FRAMES);

    harness
}

/// The rect of the tab whose name starts with `name`
fn tab_rect(harness: &Harness<'_, TextureCraftApp>, name: &str) -> egui::Rect {
    harness
        .get_all_by_role(Role::Tab)
        .find(|tab| tab.accesskit_node().label().is_some_and(|label| label.starts_with(name)))
        .map(|tab| tab.rect())
        .unwrap_or_else(|| panic!("a {name} tab"))
}

/// The first close button right of `tab` on its header bar
fn header_close(harness: &Harness<'_, TextureCraftApp>, tab: egui::Rect) -> egui::Rect {
    harness
        .get_all_by_role(Role::Button)
        .filter(|button| button.accesskit_node().label().as_deref() == Some("Close Panel"))
        .map(|button| button.rect())
        .filter(|close| close.min.x > tab.max.x && tab.y_range().contains(close.center().y))
        .min_by(|a, b| a.min.x.total_cmp(&b.min.x))
        .expect("a close button right of the tab")
}

fn click_at(harness: &mut Harness<'_, TextureCraftApp>, position: Pos2) {
    harness.hover_at(position);
    harness.event(Event::PointerButton { pos: position, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    harness.run_steps(1);
    harness.event(Event::PointerButton { pos: position, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    harness.run_steps(SETTLE_FRAMES);
}

#[test]
fn every_docked_panel_header_ends_in_its_own_close_button() {
    let harness = app();
    let names = ["Assets", "Texture Sets", "Layers", "Properties - "];
    let tabs: Vec<egui::Rect> = names.iter().map(|name| tab_rect(&harness, name)).collect();
    for (name, tab) in names.iter().zip(&tabs) {
        let close = header_close(&harness, *tab);

        // No other group's tab between a tab and its close button: the button is on its own bar
        let between = tabs.iter().any(|other| other.y_range().contains(close.center().y) && other.min.x > tab.max.x && other.max.x < close.min.x);
        assert!(!between, "{name} has no close button on its own header");
    }
}

#[test]
fn the_header_close_button_closes_its_panel() {
    let mut harness = app();
    assert!(harness.state().ui.dock.is_visible(Panel::Properties));
    let close = header_close(&harness, tab_rect(&harness, "Properties - "));
    click_at(&mut harness, close.center());

    assert!(!harness.state().ui.dock.is_open(Panel::Properties));
    assert!(harness.state().ui.dock.is_visible(Panel::Layers), "only the panel under the bar closes");
}

/// With an effect selected the Properties header names its type, not the effect's own name (every filter reads "Filter")
#[test]
fn the_properties_header_names_the_selected_effects_type() {
    let mut harness = app();
    let session = &mut harness.state_mut().session;
    let layer = texturecraft_engine::execute(session, "layer.add.fill", &serde_json::Value::Null).unwrap()["layer"].as_u64().unwrap();
    let blur = texturecraft_engine::execute(session, "effect.add.filter", &serde_json::json!({"layer": layer, "target": "content", "filter": "blur"})).unwrap()
        ["effect"]
        .as_u64()
        .unwrap();
    texturecraft_engine::execute(session, "effect.set", &serde_json::json!({"effect": blur, "name": "Soften edges"})).unwrap();
    texturecraft_engine::execute(session, "layer.select", &serde_json::json!({"layer": layer, "target": blur})).unwrap();
    harness.run_steps(SETTLE_FRAMES);

    tab_rect(&harness, "Properties - Filter");
    let level = texturecraft_engine::execute(&mut harness.state_mut().session, "effect.add.levels", &serde_json::json!({"layer": layer, "target": "content"}))
        .unwrap()["effect"]
        .as_u64()
        .unwrap();
    texturecraft_engine::execute(&mut harness.state_mut().session, "layer.select", &serde_json::json!({"layer": layer, "target": level})).unwrap();
    harness.run_steps(SETTLE_FRAMES);
    tab_rect(&harness, "Properties - Levels");
}
