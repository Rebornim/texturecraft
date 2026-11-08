//! Floating panels first open beside the icon rail, and reopen where they were closed

use egui_kittest::Harness;
use texturecraft_ui_egui::TextureCraftApp;
use texturecraft_ui_egui::docking::Panel;

const WINDOW_SIZE: egui::Vec2 = egui::vec2(1600.0, 960.0);

/// The icon rail's width, right of the dock area
const RAIL_WIDTH: f32 = 38.0;

/// How close the panel's right edge must come to the rail; the dock area keeps a small margin
const RAIL_TOLERANCE: f32 = 16.0;

// Small texture and bake sizes keep the sample session fast; the dock doesn't depend on them
const TEXTURE_RESOLUTION: u32 = 256;
const BAKE_RESOLUTION: u32 = 64;

/// Frames for the first layout and fonts to settle
const STARTUP_FRAMES: usize = 4;

/// Frames for a change to reach the dock
const SETTLE_FRAMES: usize = 3;

fn app() -> Harness<'static, TextureCraftApp> {
    let session = texturecraft_engine::sample_session(TEXTURE_RESOLUTION, BAKE_RESOLUTION).unwrap();
    let mut harness = Harness::builder().with_size(WINDOW_SIZE).build_eframe(|cc| TextureCraftApp::new(cc, session));
    harness.run_steps(STARTUP_FRAMES);
    harness
}

/// The floating window holding `panel`: `[x, y, width, height]`
fn floating_rect(harness: &Harness<'_, TextureCraftApp>, panel: Panel) -> [f32; 4] {
    let layout = &harness.state().ui.dock.workspace().layout;
    layout.floating.iter().find(|window| window.panels().any(|p| *p == panel)).expect("the panel floats").rect
}

#[test]
fn a_floating_panel_first_opens_beside_the_rail() {
    let mut harness = app();
    harness.state_mut().ui.dock.focus(Panel::Shader);
    harness.run_steps(SETTLE_FRAMES);

    let [x, _, width, _] = floating_rect(&harness, Panel::Shader);
    let rail_left = WINDOW_SIZE.x - RAIL_WIDTH;
    assert!((rail_left - (x + width)).abs() <= RAIL_TOLERANCE, "the panel ends at {} and the rail starts at {rail_left}", x + width);
}

#[test]
fn a_panel_reopens_where_it_was_closed() {
    let mut harness = app();
    harness.state_mut().ui.dock.focus(Panel::History);
    harness.run_steps(SETTLE_FRAMES);
    let opened = floating_rect(&harness, Panel::History);

    harness.state_mut().ui.dock.close(Panel::History);
    harness.run_steps(SETTLE_FRAMES);
    harness.state_mut().ui.dock.focus(Panel::History);
    harness.run_steps(SETTLE_FRAMES);

    assert_eq!(floating_rect(&harness, Panel::History), opened);
}
