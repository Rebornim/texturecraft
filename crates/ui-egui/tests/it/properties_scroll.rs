//! The docked Properties list keeps its scroll place per layer: selecting another layer and coming back lands where it was; it scrolls
//! until its last section's top reaches the top of the list

use egui::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::json;
use texturecraft_engine::execute;
use texturecraft_ui_egui::TextureCraftApp;

use crate::{BAKE_RESOLUTION, REDRAW_FRAMES, STARTUP_FRAMES, TEXTURE_RESOLUTION};

/// Short enough that a fill's Properties scrolls
const WINDOW_SIZE: egui::Vec2 = egui::vec2(1600.0, 1000.0);

/// Tall enough that a fill's three sections fit in the list without scrolling
const TALL_WINDOW: egui::Vec2 = egui::vec2(1600.0, 2400.0);

/// Short enough that a fill's MATERIAL section alone is taller than the list
const SHORT_WINDOW: egui::Vec2 = egui::vec2(1600.0, 600.0);

/// Wheel steps, in points, and how many reach the end of any fill's list
const WHEEL_NOTCH: f32 = 120.0;
const WHEEL_NOTCHES_TO_END: usize = 40;

/// Frames for the wheel's smoothed scroll to settle
const WHEEL_SETTLE_FRAMES: usize = 30;

/// A row's height and the air under the last one: the most a list scrolled to its rows' end shows below them
const ROW_AND_AIR: f32 = 40.0;

/// From the tab strip down into the list, where the wheel is turned
const INTO_LIST: f32 = 100.0;

/// The app with two UV fills ("Fill A", "Fill B") and a folder ("Folder", a short panel), Fill A selected
fn harness() -> Harness<'static, TextureCraftApp> {
    harness_sized(WINDOW_SIZE)
}

/// [`harness`] in a window of `size`
fn harness_sized(size: egui::Vec2) -> Harness<'static, TextureCraftApp> {
    let mut session = texturecraft_engine::sample_session(TEXTURE_RESOLUTION, BAKE_RESOLUTION).unwrap();
    execute(&mut session, "layer.add.group", &json!({"name": "Folder"})).unwrap();
    for name in ["Fill B", "Fill A"] {
        let layer = execute(&mut session, "layer.add.fill", &json!({})).unwrap()["layer"].as_u64().unwrap();
        execute(&mut session, "layer.rename", &json!({"layer": layer, "name": name})).unwrap();
        execute(&mut session, "layer.fill.projection", &json!({"layer": layer, "projection": "uv"})).unwrap();
    }

    let mut harness = Harness::builder().with_size(size).build_eframe(|cc| TextureCraftApp::new(cc, session));

    // The labels below are English; a first run would speak the system's language
    harness.state_mut().ui.prefs.language = "en".into();
    harness.run_steps(STARTUP_FRAMES);
    harness
}

/// A press and release at `at`, as the pointer does
fn click_at(harness: &mut Harness<'_, TextureCraftApp>, at: egui::Pos2) {
    harness.drag_at(at);
    harness.drop_at(at);
    harness.run_steps(REDRAW_FRAMES);
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

/// Turns the wheel down over the docked list until it can scroll no further
fn wheel_to_end(harness: &mut Harness<'_, TextureCraftApp>) {
    let strip = harness.get_by_role_and_label(Role::Tab, "Fill").rect();
    harness.hover_at(strip.center_bottom() + egui::vec2(0.0, INTO_LIST));
    for _ in 0..WHEEL_NOTCHES_TO_END {
        harness.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, -WHEEL_NOTCH),
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Wheel,
            modifiers: egui::Modifiers::NONE,
        });
        harness.step();
    }
    harness.run_steps(WHEEL_SETTLE_FRAMES);
}

/// Top of the docked list's `label` combo box (a fill's first row)
fn combo_top(harness: &Harness<'_, TextureCraftApp>, label: &str) -> f32 {
    harness.get_by_role_and_label(Role::ComboBox, label).rect().top()
}

/// Bottom of a new fill's last row: the Height source card's slider
fn last_row_bottom(harness: &Harness<'_, TextureCraftApp>) -> f32 {
    harness.get_by_role_and_label(Role::Slider, "Height").rect().bottom()
}

#[test]
fn a_list_whose_sections_all_fit_still_scrolls_its_last_section_to_the_top() {
    let mut harness = harness_sized(TALL_WINDOW);
    select_row(&mut harness, "Fill A");
    let list_top = harness.get_by_role_and_label(Role::Tab, "Fill").rect().bottom();
    assert!(last_row_bottom(&harness) < TALL_WINDOW.y, "every section fits in the list");

    wheel_to_end(&mut harness);
    assert!(tab_lit(&harness, "Material"), "the wheel brought MATERIAL to the top");
    assert!(!tab_lit(&harness, "Fill"));
    assert!(combo_top(&harness, "UV projection") < list_top, "FILL scrolled out above the list");

    select_row(&mut harness, "Folder");
    select_row(&mut harness, "Fill A");
    assert!(tab_lit(&harness, "Material"), "coming back keeps the place past the rows' end");
}

#[test]
fn a_last_section_taller_than_the_list_scrolls_to_its_real_end() {
    let mut harness = harness_sized(SHORT_WINDOW);
    select_row(&mut harness, "Fill A");
    let strip = harness.get_by_role_and_label(Role::Tab, "Fill").rect();
    let bar = harness.get_all_by_role(Role::ScrollBar).map(|bar| bar.rect()).find(|bar| bar.top() >= strip.top() && bar.left() > strip.left());
    let list_bottom = bar.unwrap().bottom();
    assert!(last_row_bottom(&harness) > SHORT_WINDOW.y, "the list's rows run below the window");

    wheel_to_end(&mut harness);
    let last_bottom = last_row_bottom(&harness);
    assert!(tab_lit(&harness, "Material"));
    assert!(last_bottom <= list_bottom, "the last row came into view");
    assert!(list_bottom - last_bottom < ROW_AND_AIR, "no blank space past a section taller than the list");
}
