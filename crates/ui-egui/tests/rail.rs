//! The rail beside the dock: one icon per panel the layout has nowhere else, each opening its panel in a flyout

use egui::{Event, Modifiers, PointerButton, Pos2, Role, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use texturecraft_ui_egui::TextureCraftApp;
use texturecraft_ui_egui::docking::Panel;

const WINDOW_SIZE: egui::Vec2 = egui::vec2(1600.0, 960.0);

// Small texture and bake sizes keep the sample session fast; the rail doesn't depend on them
const TEXTURE_RESOLUTION: u32 = 256;
const BAKE_RESOLUTION: u32 = 64;

/// Frames for the first layout and fonts to settle
const STARTUP_FRAMES: usize = 4;

/// Frames for a click to reach the dock and its flyout
const SETTLE_FRAMES: usize = 3;

/// A setting only Shader Settings shows, and one only Display Settings shows
const SHADER_SETTING: &str = "Index of refraction";
const DISPLAY_SETTING: &str = "Environment opacity";

/// A line the sample session's bake writes to the Log
const LOG_LINE: &str = "Baked texture set 0 (CPU)";

fn app() -> Harness<'static, TextureCraftApp> {
    let session = texturecraft_engine::sample_session(TEXTURE_RESOLUTION, BAKE_RESOLUTION).unwrap();
    let mut harness = Harness::builder().with_size(WINDOW_SIZE).build_eframe(|cc| TextureCraftApp::new(cc, session));
    // The labels below are English; a first run would speak the system's language
    harness.state_mut().ui.prefs.language = "en".into();
    harness.run_steps(STARTUP_FRAMES);
    harness
}

/// Drag with the primary button from `from` to `to` in a few moves
fn drag(harness: &mut Harness<'_, TextureCraftApp>, from: Pos2, to: Pos2) {
    harness.hover_at(from);
    harness.run_steps(1);
    harness.event(Event::PointerButton { pos: from, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    harness.run_steps(2);
    for step in 1..=4 {
        harness.hover_at(from + (to - from) * (step as f32 / 4.0));
        harness.run_steps(1);
    }
    harness.event(Event::PointerButton { pos: to, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    harness.run_steps(SETTLE_FRAMES);
}

fn icon_rect(harness: &Harness<'_, TextureCraftApp>, panel: &str) -> egui::Rect {
    harness.get_by_role_and_label(Role::Button, panel).rect()
}

fn click_icon(harness: &mut Harness<'_, TextureCraftApp>, panel: &str) {
    harness.get_by_role_and_label(Role::Button, panel).click();
    harness.run_steps(SETTLE_FRAMES);
}

#[test]
fn the_rail_shows_one_panel_at_a_time() {
    let mut harness = app();
    assert!(harness.query_by_label(SHADER_SETTING).is_none(), "the rail starts closed");

    click_icon(&mut harness, "Shader Settings");
    assert!(harness.query_by_label(SHADER_SETTING).is_some());

    click_icon(&mut harness, "Display Settings");
    assert!(harness.query_by_label(DISPLAY_SETTING).is_some());
    assert!(harness.query_by_label(SHADER_SETTING).is_none(), "opening Display Settings closed Shader Settings");

    click_icon(&mut harness, "Display Settings");
    assert!(harness.query_by_label(DISPLAY_SETTING).is_none(), "its own icon closes it");
}

#[test]
fn a_click_outside_closes_the_flyout() {
    let mut harness = app();
    click_icon(&mut harness, "Log");
    assert!(harness.query_by_label(LOG_LINE).is_some());

    harness.get_by_label("Assets").click();
    harness.run_steps(SETTLE_FRAMES);
    assert!(harness.query_by_label(LOG_LINE).is_none(), "the Log's flyout closed");
    assert!(harness.state().ui.dock.is_on_rail(Panel::Log), "the Log waits on the rail");
}

#[test]
fn the_texture_set_settings_icon_appears_when_its_tab_closes() {
    let mut harness = app();
    assert!(harness.query_by_role_and_label(Role::Button, "Texture Set Settings").is_none(), "it starts as a tab beside Layers");

    harness.state_mut().ui.dock.close(Panel::TextureSetSettings);
    harness.run_steps(SETTLE_FRAMES);
    assert!(harness.state().ui.dock.is_on_rail(Panel::TextureSetSettings));
    assert!(harness.query_by_role_and_label(Role::Button, "Texture Set Settings").is_some());
}

#[test]
fn the_rail_does_not_move_or_widen() {
    let mut harness = app();
    let icon = icon_rect(&harness, "Display Settings");
    // How far the drags reach: well past the rail's width
    let reach = vec2(200.0, 0.0);

    for from in [icon.left_center() - vec2(1.0, 0.0), icon.left_center(), pos2_below(icon)] {
        drag(&mut harness, from, from - reach);
        assert_eq!(icon_rect(&harness, "Display Settings").x_range(), icon.x_range(), "a drag from {from:?} moved the rail");
    }
}

/// A point in the rail below its last icon
fn pos2_below(icon: egui::Rect) -> Pos2 {
    // Below the five icons the rail can hold, inside the window
    const BELOW_THE_ICONS: f32 = 400.0;
    icon.center() + vec2(0.0, BELOW_THE_ICONS)
}
