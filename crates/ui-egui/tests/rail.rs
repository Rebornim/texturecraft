//! The icon rail opens its panels attached to it, one at a time; a panel in the dock leaves the rail

use egui::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use texturecraft_ui_egui::TextureCraftApp;
use texturecraft_ui_egui::docking::Panel;

const WINDOW_SIZE: egui::Vec2 = egui::vec2(1600.0, 960.0);

/// The rail is the last 38 points of the window; an open panel's own buttons sit left of it
const RAIL_LEFT_EDGE: f32 = WINDOW_SIZE.x - 38.0;

// Small texture and bake sizes keep the sample session fast; the rail doesn't depend on them
const TEXTURE_RESOLUTION: u32 = 256;
const BAKE_RESOLUTION: u32 = 64;

/// Frames for the first layout and fonts to settle
const STARTUP_FRAMES: usize = 4;

/// Frames for a click to reach the rail and the dock
const SETTLE_FRAMES: usize = 3;

fn app() -> Harness<'static, TextureCraftApp> {
    let session = texturecraft_engine::sample_session(TEXTURE_RESOLUTION, BAKE_RESOLUTION).unwrap();
    let mut harness = Harness::builder().with_size(WINDOW_SIZE).build_eframe(|cc| TextureCraftApp::new(cc, session));
    // The labels below are English; a first run would speak the system's language
    harness.state_mut().ui.prefs.language = "en".into();
    harness.run_steps(STARTUP_FRAMES);
    harness
}

/// The rail icon of the panel named `name`, if the rail shows one
fn rail_icon<'h>(harness: &'h Harness<'_, TextureCraftApp>, name: &str) -> Option<egui_kittest::Node<'h>> {
    // A rail icon's label is its tooltip, which adds the shortcut when the panel has one
    let is_panel = |label: &str| label == name || label.starts_with(&format!("{name} ("));
    harness
        .get_all_by_role(Role::Button)
        .find(|button| button.rect().min.x >= RAIL_LEFT_EDGE && button.accesskit_node().label().is_some_and(|label| is_panel(&label)))
}

/// Click the rail icon of the panel named `name`
fn click_rail(harness: &mut Harness<'_, TextureCraftApp>, name: &str) {
    rail_icon(harness, name).expect("the panel's icon on the rail").click();
    harness.run_steps(SETTLE_FRAMES);
}

fn click_button(harness: &mut Harness<'_, TextureCraftApp>, label: &str) {
    harness.get_by_role_and_label(Role::Button, label).click();
    harness.run_steps(SETTLE_FRAMES);
}

#[test]
fn the_rail_shows_one_panel_at_a_time() {
    let mut harness = app();

    click_rail(&mut harness, "Shader Settings");
    assert_eq!(harness.state().ui.rail_panel, Some(Panel::Shader));
    assert!(harness.query_by_label("Index of refraction").is_some(), "Shader Settings is shown");

    click_rail(&mut harness, "Display Settings");
    assert_eq!(harness.state().ui.rail_panel, Some(Panel::Display));
    assert!(harness.query_by_label("Index of refraction").is_none(), "Shader Settings closed");
    assert!(!harness.state().ui.dock.is_open(Panel::Shader), "the rail's panels are not dock windows");
}

#[test]
fn the_open_panels_icon_closes_it() {
    let mut harness = app();

    click_rail(&mut harness, "Log");
    assert_eq!(harness.state().ui.rail_panel, Some(Panel::Log));

    click_rail(&mut harness, "Log");
    assert_eq!(harness.state().ui.rail_panel, None);
    assert!(!harness.state().ui.dock.is_open(Panel::Log));
}

#[test]
fn an_undocked_panel_frees_the_rail() {
    let mut harness = app();

    click_rail(&mut harness, "History");
    click_button(&mut harness, "Undock");
    assert_eq!(harness.state().ui.rail_panel, None);
    assert!(harness.state().ui.dock.is_visible(Panel::History), "History floats on its own");
    assert!(rail_icon(&harness, "History").is_none(), "a panel in the dock has no rail icon");

    click_rail(&mut harness, "Shader Settings");
    assert_eq!(harness.state().ui.rail_panel, Some(Panel::Shader));
    assert!(harness.state().ui.dock.is_visible(Panel::History), "History stays open beside the rail's panel");
}

#[test]
fn the_close_button_closes_the_rails_panel() {
    let mut harness = app();

    click_rail(&mut harness, "Display Settings");
    click_button(&mut harness, "Close");
    assert_eq!(harness.state().ui.rail_panel, None);
}

#[test]
fn a_panel_closed_in_the_dock_returns_to_the_rail() {
    let mut harness = app();
    assert!(rail_icon(&harness, "Texture Set Settings").is_none(), "Texture Set Settings starts in the dock, beside Layers");

    harness.state_mut().ui.dock.close(Panel::TextureSetSettings);
    harness.run_steps(SETTLE_FRAMES);
    click_rail(&mut harness, "Texture Set Settings");
    assert_eq!(harness.state().ui.rail_panel, Some(Panel::TextureSetSettings));
}
