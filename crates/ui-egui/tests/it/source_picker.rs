//! A fill channel's source picker: Resources | Anchor points tabs, Procedurals / Textures / Alphas / Uniform tiles, a search, opened on the current source

use egui::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::json;
use texturecraft_engine::core::{Channel, LayerId, LayerKind, ProceduralKind, Source};
use texturecraft_ui_egui::TextureCraftApp;

/// Tall enough that Properties shows a fill's cards and an open picker without scrolling the panel
const WINDOW_SIZE: egui::Vec2 = egui::vec2(1600.0, 1600.0);
const TEXTURE_RESOLUTION: u32 = 256;
const BAKE_RESOLUTION: u32 = 64;
const STARTUP_FRAMES: usize = 4;
const SETTLE_FRAMES: usize = 3;
/// Frames for the grid's scroll to the current tile to finish its animation
const SCROLL_FRAMES: usize = 25;

/// The app with a new fill layer selected on its Material tab, the layer's id; `prepare` runs on the session first
fn app_with_fill(prepare: impl FnOnce(&mut texturecraft_engine::Session, LayerId)) -> (Harness<'static, TextureCraftApp>, LayerId) {
    let mut session = texturecraft_engine::sample_session(TEXTURE_RESOLUTION, BAKE_RESOLUTION).unwrap();
    let added = texturecraft_engine::execute(&mut session, "layer.add.fill", &json!({})).unwrap();
    let layer = LayerId(added["layer"].as_u64().unwrap());
    prepare(&mut session, layer);
    let mut harness = Harness::builder().with_size(WINDOW_SIZE).build_eframe(|cc| TextureCraftApp::new(cc, session));
    // The labels below are English; a first run would speak the system's language
    harness.state_mut().ui.prefs.language = "en".into();
    harness.run_steps(STARTUP_FRAMES);
    harness.get_by_role_and_label(Role::Tab, "Material").click();
    harness.run_steps(SETTLE_FRAMES);
    (harness, layer)
}

fn source(harness: &Harness<'_, TextureCraftApp>, layer: LayerId, channel: Channel) -> Source {
    match &harness.state().session.doc.project.layer(layer).unwrap().kind {
        LayerKind::Fill(fill) => fill.source(channel),
        other => panic!("not a fill: {other:?}"),
    }
}

/// Clicks the slot named `slot`, which opens or closes its picker
fn click_slot(harness: &mut Harness<'_, TextureCraftApp>, slot: &str) {
    harness.get_by_role_and_label(Role::ComboBox, slot).click();
    harness.run_steps(SETTLE_FRAMES);
}

fn click_tile(harness: &mut Harness<'_, TextureCraftApp>, label: &str) {
    harness.get_by_role_and_label(Role::ListBoxOption, label).click();
    harness.run_steps(SETTLE_FRAMES);
}

fn type_search(harness: &mut Harness<'_, TextureCraftApp>, text: &str) {
    harness.event(egui::Event::Text(text.into()));
    harness.run_steps(SETTLE_FRAMES);
}

#[test]
fn a_procedural_tile_sets_the_channel_and_closes_the_picker() {
    let (mut harness, layer) = app_with_fill(|_, _| {});
    click_slot(&mut harness, "Roughness");
    assert!(harness.query_by_role_and_label(Role::Tab, "Resources").is_some(), "the picker has its Resources tab");
    assert!(harness.query_by_role_and_label(Role::Tab, "Anchor points").is_some(), "and its Anchor points tab");

    click_tile(&mut harness, "Clouds");
    let Source::Procedural { pattern, .. } = source(&harness, layer, Channel::Roughness) else { panic!("a procedural") };
    assert_eq!(pattern.kind, ProceduralKind::Clouds);
    assert!(!egui::Popup::is_any_open(&harness.ctx), "a pick closes the picker");

    // The Uniform tile at the end of Resources puts the channel back to a uniform value
    click_slot(&mut harness, "Roughness");
    type_search(&mut harness, "uniform");
    click_tile(&mut harness, "Uniform value");
    assert_eq!(source(&harness, layer, Channel::Roughness), Source::uniform_of(Channel::Roughness));
}

#[test]
fn an_alpha_from_the_search_becomes_the_channels_source() {
    let (mut harness, layer) = app_with_fill(|_, _| {});
    click_slot(&mut harness, "Roughness");
    // The search field has the focus as the picker opens and searches every Resources group
    type_search(&mut harness, "crack");
    assert!(harness.query_by_role_and_label(Role::ListBoxOption, "Clouds").is_none(), "the search hides what doesn't match");
    harness.get_by_role_and_label(Role::Button, "Clear search").click();
    harness.run_steps(SETTLE_FRAMES);
    assert!(harness.query_by_role_and_label(Role::ListBoxOption, "Clouds").is_some(), "× clears the search");
    type_search(&mut harness, "crack");

    click_tile(&mut harness, "Cracks");
    let Source::Image { name, .. } = source(&harness, layer, Channel::Roughness) else { panic!("an alpha is read as an image") };
    assert_eq!(texturecraft_engine::fill_alpha::library_alpha_of(&name).map(|alpha| alpha.label.as_str()), Some("Cracks"));
}

#[test]
fn the_picker_opens_on_the_current_sources_tab_scrolled_to_it() {
    let (mut harness, layer) = app_with_fill(|session, layer| {
        let stencil = json!({"layer": layer.0, "channel": "roughness", "alpha": "letter_z"});
        texturecraft_engine::execute(session, "layer.fill.set", &stencil).unwrap();
    });
    assert!(matches!(source(&harness, layer, Channel::Roughness), Source::Image { .. }));
    click_slot(&mut harness, "Roughness");
    harness.run_steps(SCROLL_FRAMES);

    // The last alpha sits far below the picker's first rows; it shows, outlined as the current
    let tile = harness.get_by_role_and_label(Role::ListBoxOption, "Stencil Z");
    assert_eq!(tile.accesskit_node().toggled(), Some(egui::accesskit::Toggled::True), "the current tile is outlined");
    let slot = harness.get_by_role_and_label(Role::ComboBox, "Roughness").rect();
    let grid_bottom = slot.max.y + 500.0;
    assert!(tile.rect().min.y > slot.max.y && tile.rect().max.y < grid_bottom, "the current tile is scrolled into view: {:?}", tile.rect());
}

#[test]
fn the_anchor_points_tab_lists_the_stacks_anchors() {
    // An anchor on the first fill, then a second fill above it reads the anchor
    let (mut harness, _) = app_with_fill(|session, layer| {
        texturecraft_engine::execute(session, "effect.add.anchor", &json!({"layer": layer.0, "name": "Edge wear"})).unwrap();
        texturecraft_engine::execute(session, "layer.add.fill", &json!({})).unwrap();
    });
    let reader = harness.state().session.selected_layer().unwrap();
    click_slot(&mut harness, "Roughness");
    harness.get_by_role_and_label(Role::Tab, "Anchor points").click();
    harness.run_steps(SETTLE_FRAMES);
    assert!(harness.query_by_role_and_label(Role::ListBoxOption, "Clouds").is_none(), "the tab shows anchors only");

    let anchor = harness.get_all_by_role(Role::ListBoxOption).find_map(|node| node.accesskit_node().label().filter(|label| label.contains("Edge wear")));
    click_tile(&mut harness, &anchor.expect("the anchor below is offered"));
    assert!(matches!(source(&harness, reader, Channel::Roughness), Source::Anchor { .. }));
}

#[test]
fn the_anchor_points_tab_lists_mask_anchor_points_by_name() {
    let (mut harness, _) = app_with_fill(|session, layer| {
        texturecraft_engine::execute(session, "effect.add.anchor", &json!({"layer": layer.0, "target": "mask", "name": "Dirt mask"})).unwrap();
        texturecraft_engine::execute(session, "layer.add.fill", &json!({})).unwrap();
    });
    let reader = harness.state().session.selected_layer().unwrap();
    click_slot(&mut harness, "Roughness");
    harness.get_by_role_and_label(Role::Tab, "Anchor points").click();
    harness.run_steps(SETTLE_FRAMES);
    let anchor = harness.get_all_by_role(Role::ListBoxOption).find_map(|node| node.accesskit_node().label().filter(|label| label.starts_with("Dirt mask")));
    click_tile(&mut harness, &anchor.expect("the mask anchor point below is offered under its own name"));
    assert!(matches!(source(&harness, reader, Channel::Roughness), Source::Anchor { .. }));
}

/// Press and release the primary button at `at` without moving
fn click_at(harness: &mut Harness<'_, TextureCraftApp>, at: egui::Pos2) {
    harness.hover_at(at);
    harness.event(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
    harness.run_steps(1);
    harness.event(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE });
    harness.run_steps(SETTLE_FRAMES);
}

#[test]
fn escape_or_a_click_outside_closes_it_and_the_wheel_still_zooms_the_view() {
    let (mut harness, layer) = app_with_fill(|_, _| {});
    click_slot(&mut harness, "Roughness");
    let distance = harness.state().session.camera.distance;
    let view = harness.state().viewport_rect.center();
    harness.hover_at(view);
    harness.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, 60.0),
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Wheel,
        modifiers: egui::Modifiers::NONE,
    });
    harness.run_steps(SETTLE_FRAMES);
    assert_ne!(harness.state().session.camera.distance, distance, "the wheel over the view zooms with the picker open");

    harness.ctx.memory_mut(|memory| memory.stop_text_input());
    harness.key_press(egui::Key::Escape);
    harness.run_steps(SETTLE_FRAMES);
    assert!(!egui::Popup::is_any_open(&harness.ctx), "Escape closes the picker");

    click_slot(&mut harness, "Roughness");
    assert!(egui::Popup::is_any_open(&harness.ctx));
    let slot = harness.get_by_role_and_label(Role::ComboBox, "Roughness").rect();
    click_at(&mut harness, egui::pos2(slot.min.x - 400.0, slot.min.y));
    assert!(!egui::Popup::is_any_open(&harness.ctx), "a click outside closes the picker");

    // Opened again from Uniform after a scroll down, it starts at the top
    click_slot(&mut harness, "Roughness");
    let slot = harness.get_by_role_and_label(Role::ComboBox, "Roughness").rect();
    let first_tile = harness.get_by_role_and_label(Role::ListBoxOption, "Clouds").rect().center();
    harness.hover_at(first_tile);
    harness.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, -400.0),
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Wheel,
        modifiers: egui::Modifiers::NONE,
    });
    harness.run_steps(SCROLL_FRAMES);
    let clouds = |harness: &Harness<'_, TextureCraftApp>| harness.get_by_role_and_label(Role::ListBoxOption, "Clouds").rect().min.y;
    assert!(clouds(&harness) < slot.max.y, "the wheel over the picker scrolls its grid");
    click_slot(&mut harness, "Roughness");
    click_slot(&mut harness, "Roughness");
    assert!(clouds(&harness) > slot.max.y, "the first tile shows again");
    assert_eq!(source(&harness, layer, Channel::Roughness), Source::uniform_of(Channel::Roughness), "nothing changed");
}
