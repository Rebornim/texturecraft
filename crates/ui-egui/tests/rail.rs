//! The rail opens its panels in a popup beside their icons, one at a time; dragging the popup's title docks or floats the panel

use egui::{Event, Modifiers, PointerButton, Pos2, Role, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use texturecraft_ui_egui::TextureCraftApp;
use texturecraft_ui_egui::docking::Panel;

const WINDOW_SIZE: egui::Vec2 = egui::vec2(1600.0, 960.0);

/// The rail is the last 38 points of the window; the popup's own buttons sit left of it
const RAIL_LEFT_EDGE: f32 = WINDOW_SIZE.x - 38.0;

// Small texture and bake sizes keep the sample session fast; the rail doesn't depend on them
const TEXTURE_RESOLUTION: u32 = 256;
const BAKE_RESOLUTION: u32 = 64;

/// Frames for the first layout and fonts to settle
const STARTUP_FRAMES: usize = 4;

/// Frames for a click to reach the rail, the popup and the dock
const SETTLE_FRAMES: usize = 3;

/// A setting only Shader Settings shows, and one only Display Settings shows
const SHADER_SETTING: &str = "Index of refraction";
const DISPLAY_SETTING: &str = "Environment opacity";

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

fn click_rail(harness: &mut Harness<'_, TextureCraftApp>, name: &str) {
    rail_icon(harness, name).expect("the panel's icon on the rail").click();
    harness.run_steps(SETTLE_FRAMES);
}

/// Drag with the primary button from `from` to `to` in a few moves
fn drag(harness: &mut Harness<'_, TextureCraftApp>, from: Pos2, to: Pos2) {
    harness.hover_at(from);
    harness.run_steps(1);
    harness.event(Event::PointerButton { pos: from, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    harness.run_steps(2);
    for step in 1..=6 {
        harness.hover_at(from + (to - from) * (step as f32 / 6.0));
        harness.run_steps(2);
    }
    harness.event(Event::PointerButton { pos: to, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    harness.run_steps(SETTLE_FRAMES);
}

/// The popup's title: the panel's name left of the rail
fn popup_title(harness: &Harness<'_, TextureCraftApp>, name: &str) -> egui::Rect {
    harness.get_all_by_label(name).map(|node| node.rect()).find(|rect| rect.max.x < RAIL_LEFT_EDGE).expect("the popup's title")
}

#[test]
fn the_rail_shows_one_panel_at_a_time() {
    let mut harness = app();

    click_rail(&mut harness, "Shader Settings");
    assert_eq!(harness.state().ui.rail_panel, Some(Panel::Shader));
    assert!(harness.query_by_label(SHADER_SETTING).is_some());

    click_rail(&mut harness, "Display Settings");
    assert_eq!(harness.state().ui.rail_panel, Some(Panel::Display));
    assert!(harness.query_by_label(SHADER_SETTING).is_none(), "opening Display Settings closed Shader Settings");
    assert!(harness.query_by_label(DISPLAY_SETTING).is_some());
    assert!(!harness.state().ui.dock.is_open(Panel::Shader), "the rail's panels are not dock windows");

    click_rail(&mut harness, "Display Settings");
    assert_eq!(harness.state().ui.rail_panel, None, "its own icon closes it");
}

#[test]
fn the_popup_lines_up_with_its_icon() {
    let mut harness = app();

    click_rail(&mut harness, "History");
    let icon = rail_icon(&harness, "History").expect("the History icon").rect();
    let title = popup_title(&harness, "History");
    assert!(title.top() >= icon.top() && title.top() <= icon.bottom(), "the title {title:?} is not level with the icon {icon:?}");
}

#[test]
fn the_close_button_a_press_outside_and_escape_close_it() {
    let mut harness = app();

    click_rail(&mut harness, "Display Settings");
    harness.get_by_role_and_label(Role::Button, "Close").click();
    harness.run_steps(SETTLE_FRAMES);
    assert_eq!(harness.state().ui.rail_panel, None, "Close");

    click_rail(&mut harness, "Log");
    harness.get_by_label("Assets").click();
    harness.run_steps(SETTLE_FRAMES);
    assert_eq!(harness.state().ui.rail_panel, None, "a press outside");

    click_rail(&mut harness, "Log");
    harness.key_press(egui::Key::Escape);
    harness.run_steps(SETTLE_FRAMES);
    assert_eq!(harness.state().ui.rail_panel, None, "Escape");
}

#[test]
fn dragging_the_title_out_floats_the_panel_and_frees_the_rail() {
    let mut harness = app();

    click_rail(&mut harness, "History");
    let title = popup_title(&harness, "History").center();
    drag(&mut harness, title, title - vec2(500.0, -100.0));
    assert_eq!(harness.state().ui.rail_panel, None);
    assert!(harness.state().ui.dock.is_visible(Panel::History), "History floats on its own");
    assert!(rail_icon(&harness, "History").is_none(), "a panel in the dock has no rail icon");

    click_rail(&mut harness, "Shader Settings");
    assert_eq!(harness.state().ui.rail_panel, Some(Panel::Shader));
    assert!(harness.state().ui.dock.is_visible(Panel::History), "History stays open beside the rail's panel");
}

#[test]
fn dragging_the_title_onto_a_tab_strip_docks_the_panel_there() {
    let mut harness = app();

    click_rail(&mut harness, "Log");
    let title = popup_title(&harness, "Log").center();
    let properties_tab = harness.get_by_label("Properties - Paint").rect().center();
    drag(&mut harness, title, properties_tab);

    let layout = &harness.state().ui.dock.workspace().layout;
    assert!(layout.contains(&Panel::Log) && !layout.floating.iter().any(|window| window.contains(&Panel::Log)), "the Log is docked");
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
