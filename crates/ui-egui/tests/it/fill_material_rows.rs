//! A fill's Material section: the channel strip turns channels on and off, each slot's row lists the sources, ⟲ and × restore and clear, the ramp inverts, a linked material maps its channels and picks its resolution

use egui::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::json;
use texturecraft_engine::core::{Channel, CookResolution, LayerId, LayerKind, LinkParam, MaterialLink, Procedural, ProceduralKind, Source};
use texturecraft_ui_egui::TextureCraftApp;
use texturecraft_ui_egui::screen_pick::{PickOutcome, ScreenPick};

/// Tall enough that Properties shows the whole fill and an open source list without scrolling
const WINDOW_SIZE: egui::Vec2 = egui::vec2(1600.0, 1600.0);

/// Small texture and bake sizes keep the sample session fast; the panel doesn't depend on them
const TEXTURE_RESOLUTION: u32 = 256;
const BAKE_RESOLUTION: u32 = 64;

/// Frames for the first layout and fonts to settle
const STARTUP_FRAMES: usize = 4;

/// Frames for a click or selection change to reach the panel
const SETTLE_FRAMES: usize = 3;

/// Frames for a scroll to bring a control into view
const SCROLL_FRAMES: usize = 25;

/// The app with a new fill layer (`settings` given to layer.add.fill) selected and its Material tab clicked, and the layer's id
fn app_with_fill(settings: serde_json::Value) -> (Harness<'static, TextureCraftApp>, LayerId) {
    let mut session = texturecraft_engine::sample_session(TEXTURE_RESOLUTION, BAKE_RESOLUTION).unwrap();
    let added = texturecraft_engine::execute(&mut session, "layer.add.fill", &settings).unwrap();
    let layer = LayerId(added["layer"].as_u64().unwrap());
    let mut harness = Harness::builder().with_size(WINDOW_SIZE).build_eframe(|cc| TextureCraftApp::new(cc, session));
    // The labels below are English; a first run would speak the system's language
    harness.state_mut().ui.prefs.language = "en".into();
    harness.run_steps(STARTUP_FRAMES);
    harness.get_by_role_and_label(Role::Tab, "Material").click();
    harness.run_steps(SETTLE_FRAMES);
    (harness, layer)
}

fn source(harness: &Harness<'_, TextureCraftApp>, layer: LayerId, channel: Channel) -> Source {
    let layer = harness.state().session.doc.project.layer(layer).unwrap();
    match &layer.kind {
        texturecraft_engine::core::LayerKind::Fill(fill) => fill.source(channel),
        other => panic!("not a fill: {other:?}"),
    }
}

fn click(harness: &mut Harness<'_, TextureCraftApp>, role: Role, label: &str) {
    // A link without a package fails to re-cook with a toast, which must not take the click
    harness.state_mut().toasts.clear();
    harness.get_by_role_and_label(role, label).click();
    harness.run_steps(SETTLE_FRAMES);
}

/// Scrolls the `role` named `label` into view (the scroll animates for about twenty frames), then clicks it
fn click_below(harness: &mut Harness<'_, TextureCraftApp>, role: Role, label: &str) {
    harness.get_by_role_and_label(role, label).scroll_to_me();
    harness.run_steps(SCROLL_FRAMES);
    click(harness, role, label);
}

/// Clicks the `role` named `label` that sits in the row of the slot named `slot` (bands above have their own ⟲)
fn click_in_slot(harness: &mut Harness<'_, TextureCraftApp>, slot: &str, role: Role, label: &str) {
    let row = harness.get_by_role_and_label(Role::ComboBox, slot).rect();
    let node = harness.get_all_by_role_and_label(role, label).find(|node| row.contains(node.rect().center())).unwrap();
    node.click();
    harness.run_steps(SETTLE_FRAMES);
}

#[test]
fn the_channel_strip_turns_a_channel_and_its_card_off_and_on() {
    let (mut harness, layer) = app_with_fill(json!({}));
    assert!(harness.query_by_role_and_label(Role::ComboBox, "Metallic").is_some(), "Metallic is on: its card shows");

    click(&mut harness, Role::CheckBox, "Metal");
    let channels = &harness.state().session.doc.project.layer(layer).unwrap().channels;
    assert!(!channels.contains_key(&Channel::Metallic), "off in the strip, the fill stops writing Metallic");
    assert!(harness.query_by_role_and_label(Role::ComboBox, "Metallic").is_none(), "a channel that is off has no card");

    click(&mut harness, Role::CheckBox, "Metal");
    assert!(harness.state().session.doc.project.layer(layer).unwrap().channels.contains_key(&Channel::Metallic));
    assert!(harness.query_by_role_and_label(Role::ComboBox, "Metallic").is_some());
}

#[test]
fn a_slot_lists_its_sources_restores_a_procedural_and_clears_back_to_uniform() {
    let (mut harness, layer) = app_with_fill(json!({}));
    click(&mut harness, Role::ComboBox, "Metallic");
    click(&mut harness, Role::ListBoxOption, ProceduralKind::Clouds.label());
    let Source::Procedural { pattern, .. } = source(&harness, layer, Channel::Metallic) else { panic!("a procedural") };
    assert_eq!(pattern.kind, ProceduralKind::Clouds);
    assert!(harness.query_by_role_and_label(Role::Slider, "Contrast").is_some(), "a procedural shows its Scale and Contrast");

    texturecraft_engine::execute(
        &mut harness.state_mut().session,
        "layer.fill.set",
        &json!({"layer": layer.0, "channel": "metallic", "procedural": "Clouds", "scale": 9.0, "seed": 7}),
    )
    .unwrap();
    harness.run_steps(SETTLE_FRAMES);
    click_in_slot(&mut harness, "Metallic", Role::Button, "Restore defaults");
    let Source::Procedural { pattern, .. } = source(&harness, layer, Channel::Metallic) else { panic!("still a procedural") };
    assert_eq!(pattern, Procedural::new(ProceduralKind::Clouds), "⟲ keeps the pattern and restores its settings");

    click_in_slot(&mut harness, "Metallic", Role::Button, "Clear");
    assert_eq!(source(&harness, layer, Channel::Metallic), Source::uniform_of(Channel::Metallic), "× goes back to a uniform value");
}

#[test]
fn a_uniform_gray_ramp_inverts_its_value() {
    let (mut harness, layer) = app_with_fill(json!({"roughness": 0.3}));
    texturecraft_engine::execute(&mut harness.state_mut().session, "layer.fill.set", &json!({"layer": layer.0, "channel": "roughness", "solo": true})).unwrap();
    harness.run_steps(SETTLE_FRAMES);
    click(&mut harness, Role::Button, "Invert");
    let Source::Uniform(value) = source(&harness, layer, Channel::Roughness) else { panic!("uniform") };
    assert!((value[0] - 0.7).abs() < 1e-5, "1 - 0.3: {value:?}");
}

#[test]
fn a_linked_material_shows_its_inputs_and_the_switch_shows_its_channels() {
    let (mut harness, layer) = app_with_fill(json!({}));
    assert!(harness.query_by_role_and_label(Role::RadioButton, "Material").is_some(), "every fill has the switch");

    let wear = LinkParam {
        id: "wear".into(),
        label: "Wear".into(),
        group: String::new(),
        value: json!({"t": "float", "v": 0.5}),
        default: json!({"t": "float", "v": 0.5}),
        min: 0.0,
        max: 1.0,
        options: Vec::new(),
    };
    let link = MaterialLink {
        name: "Painted steel".into(),
        package: String::new(),
        graph: "main".into(),
        params: vec![wear],
        presets: Vec::new(),
        inputs: Vec::new(),
        size: 256,
        resolution: None,
        mapping: Default::default(),
        outputs: Default::default(),
    };
    if let Some(LayerKind::Fill(fill)) = harness.state_mut().session.doc.project.layers.get_mut(&layer).map(|layer| &mut layer.kind) {
        fill.link = Some(Box::new(link));
    }
    harness.run_steps(SETTLE_FRAMES);
    assert!(harness.query_by_role_and_label(Role::ComboBox, "Painted steel").is_some(), "the Material side shows the whole material");
    assert!(harness.query_by_role_and_label(Role::Slider, "Wear").is_some(), "with its inputs");
    assert!(harness.query_by_role_and_label(Role::ComboBox, "Metallic").is_none(), "and no channel cards");

    click(&mut harness, Role::RadioButton, "Channels");
    assert!(harness.query_by_role_and_label(Role::ComboBox, "Metallic").is_some(), "the Channels side shows a card per channel");
    assert!(harness.query_by_role_and_label(Role::Slider, "Wear").is_none());
}

#[test]
fn a_linked_material_maps_its_channels_and_picks_its_resolution() {
    let (mut harness, layer) = app_with_fill(json!({}));
    let link = MaterialLink {
        name: "Painted steel".into(),
        package: String::new(),
        graph: "main".into(),
        params: Vec::new(),
        presets: Vec::new(),
        inputs: Vec::new(),
        size: 256,
        resolution: None,
        mapping: Default::default(),
        outputs: [("baseColor", "steel_base.png"), ("roughness", "steel_rough.png")].map(|(output, image)| (output.to_owned(), image.to_owned())).into(),
    };
    if let Some(LayerKind::Fill(fill)) = harness.state_mut().session.doc.project.layers.get_mut(&layer).map(|layer| &mut layer.kind) {
        fill.link = Some(Box::new(link));
    }
    harness.run_steps(SETTLE_FRAMES);
    let link = |harness: &Harness<'_, TextureCraftApp>| match &harness.state().session.doc.project.layer(layer).unwrap().kind {
        LayerKind::Fill(fill) => fill.link.as_deref().unwrap().clone(),
        other => panic!("not a fill: {other:?}"),
    };

    // Channels mapping: the Roughness row lists the outputs; picking one makes Roughness read it
    click_below(&mut harness, Role::ComboBox, "roughness");
    click(&mut harness, Role::ListBoxOption, "baseColor");
    assert_eq!(link(&harness).output_of(Channel::Roughness), "baseColor");
    assert!(matches!(source(&harness, layer, Channel::Roughness), Source::Image { name, .. } if name == "steel_base.png"));

    // Resolution: folded until clicked; Fixed keeps the size, the list picks another, Relative goes back to a factor
    assert!(harness.query_by_role_and_label(Role::RadioButton, "Fixed").is_none(), "Resolution starts folded");
    click_below(&mut harness, Role::Button, "Resolution");
    click_below(&mut harness, Role::RadioButton, "Fixed");
    assert_eq!(link(&harness).resolution, Some(CookResolution::Fixed(TEXTURE_RESOLUTION)));
    click_below(&mut harness, Role::ComboBox, "256 × 256");
    click(&mut harness, Role::ListBoxOption, "1024 × 1024");
    assert_eq!(link(&harness).resolution, Some(CookResolution::Fixed(1024)));
    click_below(&mut harness, Role::RadioButton, "Relative");
    assert_eq!(link(&harness).resolution, Some(CookResolution::Relative(2)), "1024 is the texture set's ×4");
}

/// The fill's uniform base colour (linear)
fn base_color(harness: &Harness<'_, TextureCraftApp>, layer: LayerId) -> [f32; 3] {
    let Source::Uniform(color) = source(harness, layer, Channel::BaseColor) else { panic!("a uniform colour") };
    color
}

#[test]
fn the_colour_bar_opens_the_picker_which_types_restores_and_waits_for_an_eyedropper_click() {
    let (mut harness, layer) = app_with_fill(json!({"color": [0.2, 0.2, 0.2]}));
    let before = base_color(&harness, layer);
    click(&mut harness, Role::Button, "Base color");
    assert!(harness.query_by_role_and_label(Role::ComboBox, "sRGB").is_some(), "the picker shows its colour space");

    click(&mut harness, Role::TextInput, "Hex color");
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    harness.event(egui::Event::Text("FF8000".into()));
    harness.key_press(egui::Key::Enter);
    harness.run_steps(SETTLE_FRAMES);
    let typed = base_color(&harness, layer);
    let expected = [1.0, texturecraft_engine::texel::srgb_to_linear(128.0 / 255.0), 0.0];
    assert!(typed.iter().zip(expected).all(|(a, b)| (a - b).abs() < 1e-3), "the typed sRGB code is stored linear: {typed:?}");

    click(&mut harness, Role::ComboBox, "HSV");
    click(&mut harness, Role::ListBoxOption, "RGB");
    assert!(harness.query_by_role_and_label(Role::Slider, "R").is_some(), "the sliders edit R, G and B");
    assert!(harness.query_by_role_and_label(Role::ComboBox, "sRGB").is_some(), "picking from its own list keeps the picker open");

    click(&mut harness, Role::Button, "Previous color");
    let restored = base_color(&harness, layer);
    assert!(restored.iter().zip(before).all(|(a, b)| (a - b).abs() < 1e-4), "a click on the previous colour restores it: {restored:?} {before:?}");

    harness.get_all_by_role_and_label(Role::Button, "Pick color").next().unwrap().click();
    harness.run_steps(SETTLE_FRAMES);
    assert!(harness.state().picking_color.is_some(), "the eyedropper waits for a click on the model");
}

#[test]
fn the_picker_is_a_window_beside_the_panel_that_its_title_drags_and_its_close_or_escape_closes() {
    let (mut harness, _) = app_with_fill(json!({"color": [0.2, 0.2, 0.2]}));
    click(&mut harness, Role::Button, "Base color");
    let bar = harness.get_by_role_and_label(Role::Button, "Base color").rect();
    let title = harness.get_by_role_and_label(Role::TitleBar, "Base color").rect();
    let space = harness.get_by_role_and_label(Role::ComboBox, "sRGB").rect();
    assert!(title.right() <= bar.left(), "the window stands beside the panel, off its rows: {title:?} {bar:?}");
    assert!(title.top() <= bar.top() + 1.0, "level with the bar, or raised to stay on screen: {title:?} {bar:?}");
    assert!(space.width() < title.width() / 3.0, "the colour space dropdown is as wide as its text: {space:?}");

    let from = title.left_center() + egui::vec2(20.0, 0.0);
    let by = egui::vec2(-120.0, 90.0);
    harness.hover_at(from);
    harness.event(egui::Event::PointerButton { pos: from, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
    harness.run_steps(1);
    for step in 1..=4 {
        harness.hover_at(from + by * (step as f32 / 4.0));
        harness.run_steps(1);
    }
    harness.event(egui::Event::PointerButton { pos: from + by, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE });
    harness.run_steps(SETTLE_FRAMES);
    let moved = harness.get_by_role_and_label(Role::TitleBar, "Base color").rect();
    assert!((moved.min - (title.min + by)).length() < 1.0, "the title bar drags the window: {moved:?} from {title:?}");
    assert!(harness.query_by_role_and_label(Role::ComboBox, "sRGB").is_some(), "a drag on the title keeps it open");

    let close = harness
        .get_all_by_role_and_label(Role::Button, "Close")
        .find(|node| moved.y_range().contains(node.rect().center().y) && node.rect().left() >= moved.right())
        .unwrap();
    close.click();
    harness.run_steps(SETTLE_FRAMES);
    assert!(harness.query_by_role_and_label(Role::TitleBar, "Base color").is_none(), "× closes it");

    click(&mut harness, Role::Button, "Base color");
    let reopened = harness.get_by_role_and_label(Role::TitleBar, "Base color").rect();
    assert!((reopened.min - moved.min).length() < 1.0, "it opens again where it was dragged: {reopened:?}");
    harness.key_press(egui::Key::Escape);
    harness.run_steps(SETTLE_FRAMES);
    assert!(harness.query_by_role_and_label(Role::TitleBar, "Base color").is_none(), "Escape closes it");
}

#[test]
fn escape_ends_an_eyedropper_pick_before_the_picker_and_an_alt_orbit_keeps_the_picker_open() {
    let (mut harness, _) = app_with_fill(json!({"color": [0.2, 0.2, 0.2]}));
    click(&mut harness, Role::Button, "Base color");
    harness.get_all_by_role_and_label(Role::Button, "Pick color").next().unwrap().click();
    harness.run_steps(SETTLE_FRAMES);
    assert!(harness.state().picking_color.is_some(), "the eyedropper waits for a click on the model");

    harness.key_press(egui::Key::Escape);
    harness.run_steps(SETTLE_FRAMES);
    assert!(harness.state().picking_color.is_none(), "Escape ends the pick");
    assert!(harness.query_by_role_and_label(Role::TitleBar, "Base color").is_some(), "the same Escape leaves the picker open");

    let view = harness.state().viewport_rect.center();
    let alt = egui::Modifiers::ALT;
    harness.event(egui::Event::ModifiersChanged(alt));
    harness.hover_at(view);
    harness.event(egui::Event::PointerButton { pos: view, button: egui::PointerButton::Primary, pressed: true, modifiers: alt });
    harness.run_steps(1);
    harness.hover_at(view + egui::vec2(40.0, 10.0));
    harness.run_steps(1);
    harness.event(egui::Event::PointerButton { pos: view + egui::vec2(40.0, 10.0), button: egui::PointerButton::Primary, pressed: false, modifiers: alt });
    harness.event(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
    harness.run_steps(SETTLE_FRAMES);
    assert!(harness.query_by_role_and_label(Role::TitleBar, "Base color").is_some(), "an Alt+drag on the model orbits and keeps the picker open");

    harness.key_press(egui::Key::Escape);
    harness.run_steps(SETTLE_FRAMES);
    assert!(harness.query_by_role_and_label(Role::TitleBar, "Base color").is_none(), "the next Escape closes the picker");
}

/// A mouse press and release of `button` at `pos`, a frame apart, as a real click arrives
fn mouse_click(harness: &mut Harness<'_, TextureCraftApp>, pos: egui::Pos2, button: egui::PointerButton) {
    harness.hover_at(pos);
    harness.run_steps(1);
    for pressed in [true, false] {
        harness.event(egui::Event::PointerButton { pos, button, pressed, modifiers: egui::Modifiers::NONE });
        harness.run_steps(1);
    }
    harness.run_steps(SETTLE_FRAMES);
}

#[test]
fn the_swatches_row_keeps_the_picker_colour_in_the_project_applies_it_on_a_click_and_removes_it_from_its_menu() {
    let (mut harness, layer) = app_with_fill(json!({"color": [0.2, 0.2, 0.2]}));
    let before = base_color(&harness, layer);
    click(&mut harness, Role::Button, "Base color");
    click(&mut harness, Role::TextInput, "Hex color");
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    harness.event(egui::Event::Text("FF8000".into()));
    harness.key_press(egui::Key::Enter);
    harness.run_steps(SETTLE_FRAMES);
    let orange = base_color(&harness, layer);

    let add = harness.get_by_role_and_label(Role::Button, "Add swatch").rect();
    let space = harness.get_by_role_and_label(Role::ComboBox, "sRGB").rect();
    assert!(add.top() > space.bottom() && (add.left() - space.left()).abs() < 1.0, "the row is under the picker, at its left edge: {add:?}");
    mouse_click(&mut harness, add.center(), egui::PointerButton::Primary);
    assert_eq!(harness.state().session.doc.project.swatches, vec![orange], "+ keeps the picker's colour in the project");
    assert!(harness.query_by_role_and_label(Role::ComboBox, "sRGB").is_some(), "the picker stays open");

    click(&mut harness, Role::Button, "Previous color");
    assert_eq!(base_color(&harness, layer), before);
    let chip = harness.get_by_role_and_label(Role::Button, "#FF8000").rect();
    let moved_add = harness.get_by_role_and_label(Role::Button, "Add swatch").rect();
    assert!((chip.left() - add.left()).abs() < 1.0 && moved_add.left() > chip.right(), "the swatch takes the + chip's place, + moves right");
    mouse_click(&mut harness, chip.center(), egui::PointerButton::Primary);
    assert_eq!(base_color(&harness, layer), orange, "a click on a swatch applies it");
    assert!(harness.query_by_role_and_label(Role::ComboBox, "sRGB").is_some(), "applying keeps the picker open");

    mouse_click(&mut harness, chip.center(), egui::PointerButton::Secondary);
    let remove = harness.get_by_role_and_label(Role::Button, "Remove swatch").rect();
    mouse_click(&mut harness, remove.center(), egui::PointerButton::Primary);
    assert!(harness.state().session.doc.project.swatches.is_empty(), "its right-click menu removes it");
    assert!(harness.query_by_role_and_label(Role::ComboBox, "sRGB").is_some(), "a pick in the menu keeps the picker open");
}

/// A primary-button drag from `from` by `by` in four moves, a frame apart, as a real drag arrives
fn mouse_drag(harness: &mut Harness<'_, TextureCraftApp>, from: egui::Pos2, by: egui::Vec2) {
    harness.hover_at(from);
    harness.run_steps(1);
    harness.event(egui::Event::PointerButton { pos: from, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
    harness.run_steps(1);
    for step in 1..=4 {
        harness.hover_at(from + by * (step as f32 / 4.0));
        harness.run_steps(1);
    }
    harness.event(egui::Event::PointerButton { pos: from + by, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE });
    harness.run_steps(SETTLE_FRAMES);
}

/// The open picker's frame: its title bar's top and sides, down to the air under the + chip
fn picker_face(harness: &Harness<'_, TextureCraftApp>) -> egui::Rect {
    // The air under the picker's last line (color_bar.rs PICKER_BOTTOM)
    const PICKER_BOTTOM: f32 = 10.0;
    let title = harness.get_by_role_and_label(Role::TitleBar, "Base color").rect();
    let add = harness.get_by_role_and_label(Role::Button, "Add swatch").rect();
    egui::Rect::from_min_max(title.min, egui::pos2(title.right() + CLOSE_ROOM, add.bottom() + PICKER_BOTTOM))
}

/// The title bar stops at the × (color_bar.rs: from the right edge to the close target's left edge)
const CLOSE_ROOM: f32 = 22.0;

#[test]
fn the_picker_is_titled_by_its_channel_resizes_from_its_edges_keeps_its_size_and_previews_nothing_on_hover() {
    let (mut harness, _) = app_with_fill(json!({"color": [0.2, 0.2, 0.2]}));
    click(&mut harness, Role::Button, "Base color");
    assert!(harness.query_by_role_and_label(Role::TitleBar, "Base color").is_some(), "the title is the channel's name alone");

    let face = picker_face(&harness);
    let field = harness.get_by_role_and_label(Role::Slider, "Saturation and value").rect();
    let hue = harness.get_by_role_and_label(Role::Slider, "Hue").rect();
    let track = harness.get_by_role_and_label(Role::Slider, "S").rect();
    let by = egui::vec2(80.0, 60.0);
    mouse_drag(&mut harness, face.right_bottom() - egui::vec2(2.0, 2.0), by);
    assert!(harness.query_by_role_and_label(Role::ComboBox, "sRGB").is_some(), "a resize keeps the picker open");
    let grown = picker_face(&harness);
    assert!((grown.min - face.min).length() < 1.0, "the bottom right corner leaves the top left where it was: {grown:?} {face:?}");
    assert!((grown.size() - (face.size() + by)).length() < 1.0, "the corner drags the size: {grown:?} from {face:?}");
    let grown_field = harness.get_by_role_and_label(Role::Slider, "Saturation and value").rect();
    let grown_hue = harness.get_by_role_and_label(Role::Slider, "Hue").rect();
    let grown_track = harness.get_by_role_and_label(Role::Slider, "S").rect();
    assert!((grown_field.size() - (field.size() + by)).length() < 1.0, "the field takes the new width and height: {grown_field:?} {field:?}");
    assert!((grown_hue.height() - (hue.height() + by.y)).abs() < 1.0 && (grown_hue.right() - hue.right() - by.x).abs() < 1.0, "the hue strip grows with it");
    assert!((grown_track.width() - (track.width() + by.x)).abs() < 1.0, "the sliders' tracks take the new width");

    mouse_drag(&mut harness, egui::pos2(grown.left() + 2.0, grown.center().y), egui::vec2(-50.0, 0.0));
    let wider = picker_face(&harness);
    assert!(
        (wider.left() - (grown.left() - 50.0)).abs() < 1.0 && (wider.right() - grown.right()).abs() < 1.0,
        "the left edge moves, the right stays: {wider:?}"
    );

    mouse_drag(&mut harness, wider.right_bottom() - egui::vec2(2.0, 2.0), egui::vec2(-1000.0, -1000.0));
    let least = picker_face(&harness);
    let least_field = harness.get_by_role_and_label(Role::Slider, "Saturation and value").rect();
    assert!(least.width() >= 200.0 && least_field.height() >= 90.0, "it stops at a usable size: {least:?} {least_field:?}");

    harness.key_press(egui::Key::Escape);
    harness.run_steps(SETTLE_FRAMES);
    click(&mut harness, Role::Button, "Base color");
    let reopened = picker_face(&harness);
    assert!((reopened.size() - least.size()).length() < 1.0, "it opens again at the size it was left: {reopened:?} {least:?}");

    let value = |harness: &Harness<'_, TextureCraftApp>| harness.get_by_role_and_label(Role::Slider, "V").accesskit_node().numeric_value();
    let shown = value(&harness);
    assert!(shown.is_some(), "the V slider says its value");
    click(&mut harness, Role::ComboBox, "sRGB");
    harness.get_by_role_and_label(Role::ListBoxOption, "Linear").hover();
    harness.run_steps(SETTLE_FRAMES);
    assert_eq!(value(&harness), shown, "a hovered colour space changes nothing");
    harness.key_press(egui::Key::Escape);
    harness.run_steps(SETTLE_FRAMES);
    click(&mut harness, Role::ComboBox, "HSV");
    harness.get_by_role_and_label(Role::ListBoxOption, "RGB").hover();
    harness.run_steps(SETTLE_FRAMES);
    assert!(harness.query_by_role_and_label(Role::Slider, "R").is_none(), "a hovered model changes nothing");
    click(&mut harness, Role::ListBoxOption, "RGB");
    assert!(harness.query_by_role_and_label(Role::Slider, "R").is_some(), "a click picks it");
}

fn whole_material(harness: &Harness<'_, TextureCraftApp>, layer: LayerId) -> Option<String> {
    match &harness.state().session.doc.project.layer(layer).unwrap().kind {
        LayerKind::Fill(fill) => fill.material.clone(),
        other => panic!("not a fill: {other:?}"),
    }
}

/// A real click (press and release a frame apart) in the middle of the `role` named `label`
fn mouse_click_on(harness: &mut Harness<'_, TextureCraftApp>, role: Role, label: &str) {
    let at = harness.get_by_role_and_label(role, label).rect().center();
    mouse_click(harness, at, egui::PointerButton::Primary);
}

#[test]
fn a_plain_fill_switches_to_one_whole_material_picks_it_from_its_card_and_clears_it() {
    let (mut harness, layer) = app_with_fill(json!({}));
    assert!(harness.query_by_role_and_label(Role::ComboBox, "Metallic").is_some(), "a plain fill opens on Channels: a card per channel");

    mouse_click_on(&mut harness, Role::RadioButton, "Material");
    assert!(harness.query_by_role_and_label(Role::ComboBox, "Metallic").is_none(), "the Material side has no channel cards");
    mouse_click_on(&mut harness, Role::ComboBox, "None");
    mouse_click_on(&mut harness, Role::ListBoxOption, "Copper Hammered");
    assert_eq!(whole_material(&harness, layer).as_deref(), Some("copper_hammered"), "the card's list sets the whole material");
    assert!(harness.query_by_role_and_label(Role::ComboBox, "Copper Hammered").is_some(), "the card names it");
    assert!(harness.query_by_role_and_label(Role::ListBoxOption, "Steel").is_none(), "the list closed on the pick");

    mouse_click_on(&mut harness, Role::RadioButton, "Channels");
    assert!(matches!(source(&harness, layer, Channel::BaseColor), Source::Procedural { .. }), "the material set every channel");
    assert!(harness.query_by_role_and_label(Role::ComboBox, "Height").is_some(), "its channels show as cards");

    mouse_click_on(&mut harness, Role::RadioButton, "Material");
    click_in_slot(&mut harness, "Copper Hammered", Role::Button, "Clear");
    assert_eq!(whole_material(&harness, layer), None, "× drops the material");
    assert_eq!(source(&harness, layer, Channel::BaseColor), Source::uniform_of(Channel::BaseColor), "and returns the channels to their defaults");
}

/// Arms the base colour bar's eyedropper and gives its pick the click outside the app that ended as `outcome`
fn pick_outside(harness: &mut Harness<'_, TextureCraftApp>, outcome: PickOutcome) {
    harness.get_all_by_role_and_label(Role::Button, "Pick color").next().unwrap().click();
    harness.run_steps(SETTLE_FRAMES);
    let bar = harness.state().picking_color.expect("the eyedropper waits for a click");
    harness.state_mut().screen_pick = Some(ScreenPick::ended(bar, outcome));
    harness.run_steps(SETTLE_FRAMES);
}

#[test]
fn a_click_outside_the_app_hands_its_screen_colour_to_the_bar_and_a_cancelled_or_impossible_one_changes_nothing() {
    let (mut harness, layer) = app_with_fill(json!({"color": [0.2, 0.2, 0.2]}));
    assert!(!harness.state().screen_picking, "tests have no real window: no system hook or grab");
    let before = base_color(&harness, layer);
    click(&mut harness, Role::Button, "Base color");

    pick_outside(&mut harness, PickOutcome::Unavailable);
    assert!(harness.state().picking_color.is_some(), "where the system cannot read the screen the pick stays on the model");
    assert_eq!(base_color(&harness, layer), before, "nothing changes until a click");
    harness.key_press(egui::Key::Escape);
    harness.run_steps(SETTLE_FRAMES);
    assert!(harness.state().picking_color.is_none() && harness.state().screen_pick.is_none(), "Escape ends the pick and its wait");

    pick_outside(&mut harness, PickOutcome::Cancelled);
    assert!(harness.state().picking_color.is_none(), "dismissing the desktop's picker ends the pick");
    assert_eq!(base_color(&harness, layer), before, "a cancelled pick changes nothing");

    let undo_before = harness.state().session.history_labels().len();
    pick_outside(&mut harness, PickOutcome::Picked([255, 128, 0]));
    let picked = base_color(&harness, layer);
    let expected = [1.0, texturecraft_engine::texel::srgb_to_linear(128.0 / 255.0), 0.0];
    assert!(picked.iter().zip(expected).all(|(a, b)| (a - b).abs() < 1e-3), "the screen's sRGB pixel is stored linear: {picked:?}");
    assert!(harness.state().picking_color.is_none() && harness.state().screen_pick.is_none(), "the click ends the pick");
    assert_eq!(harness.state().session.history_labels().len(), undo_before + 1, "one undo step");
}
