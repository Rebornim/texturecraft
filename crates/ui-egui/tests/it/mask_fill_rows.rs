//! A mask fill effect's Properties: Fill | Symmetry | Grayscale tabs, the fill's projection rows, and one Grayscale slot whose picker sets its source

use egui::{Event, Modifiers, PointerButton, Pos2, Role, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;
use texturecraft_engine::core::{CookResolution, EffectKind, LayerId, ProceduralKind, Projection, Source};
use texturecraft_ui_egui::TextureCraftApp;
use texturecraft_ui_egui::move_on_model::MoveOnModel;

/// Tall enough that Properties shows every section of a mask fill without scrolling
const WINDOW_SIZE: egui::Vec2 = egui::vec2(1600.0, 1600.0);
const TEXTURE_RESOLUTION: u32 = 256;
const BAKE_RESOLUTION: u32 = 64;
const STARTUP_FRAMES: usize = 4;
const SETTLE_FRAMES: usize = 3;

/// The app with a paint layer whose mask holds one fill effect, that effect selected; the layer's and the effect's ids
fn app_with_mask_fill(projection: &str) -> (Harness<'static, TextureCraftApp>, LayerId, LayerId) {
    let mut session = texturecraft_engine::sample_session(TEXTURE_RESOLUTION, BAKE_RESOLUTION).unwrap();
    let layer = LayerId(texturecraft_engine::execute(&mut session, "layer.add.paint", &json!({})).unwrap()["layer"].as_u64().unwrap());
    let added = texturecraft_engine::execute(&mut session, "effect.add.fill", &json!({"layer": layer.0, "target": "mask", "projection": projection})).unwrap();
    let effect = LayerId(added["effect"].as_u64().unwrap());
    texturecraft_engine::execute(&mut session, "layer.select", &json!({"layer": layer.0, "target": effect.0})).unwrap();
    let mut harness = Harness::builder().with_size(WINDOW_SIZE).build_eframe(|cc| TextureCraftApp::new(cc, session));
    // The labels below are English; a first run would speak the system's language
    harness.state_mut().ui.prefs.language = "en".into();
    harness.run_steps(STARTUP_FRAMES);
    (harness, layer, effect)
}

fn mask_fill(harness: &Harness<'_, TextureCraftApp>, effect: LayerId) -> (Source, Projection) {
    match &harness.state().session.doc.project.effect(effect).unwrap().1.kind {
        EffectKind::Fill { source, projection, .. } => (source.clone(), projection.clone()),
        other => panic!("not a mask fill: {other:?}"),
    }
}

fn click(harness: &mut Harness<'_, TextureCraftApp>, role: Role, label: &str) {
    harness.get_by_role_and_label(role, label).click();
    harness.run_steps(SETTLE_FRAMES);
}

/// Scrolls the `role` named `label` into view first, then clicks it
fn click_below(harness: &mut Harness<'_, TextureCraftApp>, role: Role, label: &str) {
    harness.get_by_role_and_label(role, label).scroll_to_me();
    harness.run_steps(SETTLE_FRAMES);
    click(harness, role, label);
}

#[test]
fn a_mask_fill_has_fill_symmetry_and_grayscale_tabs_and_its_slot_picks_the_source() {
    let (mut harness, _, effect) = app_with_mask_fill("uv");
    for tab in ["Fill", "Symmetry", "Grayscale"] {
        assert!(harness.query_by_role_and_label(Role::Tab, tab).is_some(), "the {tab} tab");
    }
    assert!(harness.query_by_role_and_label(Role::Tab, "Material").is_none(), "a mask fill has no material");
    assert!(harness.query_by_label("Move on model").is_some(), "the Fill section's projection rows");

    click(&mut harness, Role::ComboBox, "Grayscale");
    assert!(harness.query_by_role_and_label(Role::Tab, "Anchor points").is_some(), "the slot opens the source picker");
    click(&mut harness, Role::ListBoxOption, "Clouds");
    let (Source::Procedural { pattern, low, high }, _) = mask_fill(&harness, effect) else { panic!("a procedural") };
    assert_eq!((pattern.kind, low, high), (ProceduralKind::Clouds, [0.0; 3], [1.0; 3]), "a gray pattern spans black to white");
    assert!(!egui::Popup::is_any_open(&harness.ctx), "a pick closes the picker");
    assert!(harness.query_by_label("Scale").is_some(), "the pattern's own rows follow the slot");

    // Resolution: the pattern is made at the texture set's size until another is picked
    let resolution = |harness: &Harness<'_, TextureCraftApp>| match mask_fill(harness, effect).0 {
        Source::Procedural { pattern, .. } => pattern.resolution,
        other => panic!("a procedural: {other:?}"),
    };
    assert!(harness.query_by_role_and_label(Role::RadioButton, "Fixed").is_none(), "Resolution starts folded");
    click_below(&mut harness, Role::Button, "Resolution");
    click_below(&mut harness, Role::RadioButton, "Fixed");
    assert_eq!(resolution(&harness), Some(CookResolution::Fixed(TEXTURE_RESOLUTION)));
    click_below(&mut harness, Role::ComboBox, "256 × 256");
    click(&mut harness, Role::ListBoxOption, "1024 × 1024");
    assert_eq!(resolution(&harness), Some(CookResolution::Fixed(1024)));
    click_below(&mut harness, Role::RadioButton, "Relative");
    assert_eq!(resolution(&harness), Some(CookResolution::Relative(2)), "1024 is the texture set's ×4");

    // The slot's × puts the mask fill back to white
    click(&mut harness, Role::Button, "Clear");
    assert_eq!(mask_fill(&harness, effect).0, Source::Uniform([1.0; 3]));
}

/// A drag as a mouse sends it: press, hold still a frame, then small moves one per frame until released; `watch` looks at the app after each move
fn mouse_drag_watching(harness: &mut Harness<'_, TextureCraftApp>, from: Pos2, to: Pos2, mut watch: impl FnMut(&Harness<'_, TextureCraftApp>)) {
    let button = |pos: Pos2, pressed: bool| Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE };
    harness.event(Event::PointerMoved(from));
    harness.run_steps(1);
    harness.event(button(from, true));
    harness.run_steps(2);
    const STEPS: usize = 40;
    for step in 1..=STEPS {
        harness.event(Event::PointerMoved(from.lerp(to, step as f32 / STEPS as f32)));
        harness.run_steps(1);
        watch(harness);
    }
    harness.event(button(to, false));
    harness.run_steps(2);
}

#[test]
fn a_mask_fills_projection_moves_on_the_model_and_resets_with_its_section() {
    let (mut harness, layer, effect) = app_with_mask_fill("uv");
    // A real drag that starts on the held button slides the mask fill's image
    let button = harness.get_by_role_and_label(Role::Button, "Move on model").rect().center();
    let mut held = None;
    mouse_drag_watching(&mut harness, button, button + vec2(-60.0, 0.0), |harness| held = held.or(harness.state().moving_on_model));
    assert_eq!(held, Some(MoveOnModel { layer, effect: Some(effect) }));
    assert_eq!(harness.state().moving_on_model, None, "letting go ends it");
    let Projection::Uv { offset, .. } = mask_fill(&harness, effect).1 else { panic!("still UV") };
    assert!(offset[0] != 0.0 && offset[1] == 0.0, "a drag across slides the image across: {offset:?}");

    click(&mut harness, Role::Button, "Fill: Reset to defaults");
    assert_eq!(mask_fill(&harness, effect).1, Projection::default(), "the Fill section's reset reaches the mask fill");
}
