//! A fill's Material section: the channel strip turns channels on and off, each slot's row lists the sources, ⟲ and × restore and clear, the ramp inverts, a linked material maps its channels and picks its resolution

use egui::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;
use texturecraft_engine::core::{Channel, CookResolution, LayerId, LayerKind, LinkParam, MaterialLink, Procedural, ProceduralKind, Source};
use texturecraft_ui_egui::TextureCraftApp;

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
    assert!(harness.query_by_role_and_label(Role::RadioButton, "Material").is_none(), "a fill without a material has no switch");

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
