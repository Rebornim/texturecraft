//! The icon rail beside the dock opens the Log panel like its other panels

use egui::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use texturecraft_ui_egui::TextureCraftApp;
use texturecraft_ui_egui::docking::Panel;

const WINDOW_SIZE: egui::Vec2 = egui::vec2(1600.0, 960.0);

// Small texture and bake sizes keep the sample session fast; the rail doesn't depend on them
const TEXTURE_RESOLUTION: u32 = 256;
const BAKE_RESOLUTION: u32 = 64;

/// The rail is the last 38 points of the window; the open Log's tab is also a "Log" button
const RAIL_LEFT_EDGE: f32 = WINDOW_SIZE.x - 38.0;

/// Frames for the first layout and fonts to settle
const STARTUP_FRAMES: usize = 4;

/// Frames for a click to reach the dock
const SETTLE_FRAMES: usize = 3;

/// Click the rail's Log button
fn click_log_button(harness: &mut Harness<'_, TextureCraftApp>) {
    // The rail's tooltip, and so its label, adds the shortcut when the Log has one
    let is_log = |label: &str| label == "Log" || label.starts_with("Log (");
    harness
        .get_all_by_role(Role::Button)
        .find(|button| button.rect().min.x >= RAIL_LEFT_EDGE && button.accesskit_node().label().is_some_and(|label| is_log(&label)))
        .expect("a Log button on the rail")
        .click();
    harness.run_steps(SETTLE_FRAMES);
}

#[test]
fn the_rail_opens_and_closes_the_log() {
    let session = texturecraft_engine::sample_session(TEXTURE_RESOLUTION, BAKE_RESOLUTION).unwrap();
    let mut harness = Harness::builder().with_size(WINDOW_SIZE).build_eframe(|cc| TextureCraftApp::new(cc, session));
    // The label below is English; a first run would speak the system's language
    harness.state_mut().ui.prefs.language = "en".into();
    harness.run_steps(STARTUP_FRAMES);
    assert!(!harness.state().ui.dock.is_visible(Panel::Log), "the Log starts closed");

    click_log_button(&mut harness);
    assert!(harness.state().ui.dock.is_visible(Panel::Log));

    click_log_button(&mut harness);
    assert!(!harness.state().ui.dock.is_visible(Panel::Log));
}
