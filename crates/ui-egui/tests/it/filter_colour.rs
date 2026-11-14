//! The colour filters in Properties: HSL Perceptive's Mode switch and centred sliders, the tone cards of Color Balance and
//! Color Correct, and Blur's Wrap toggle

use crate::filter_support::{click_at, filter_of};
use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::{Value, json};
use texturecraft_engine::core::{Filter, LayerId};
use texturecraft_engine::{StackTarget, execute};
use texturecraft_ui_egui::TextureCraftApp;

/// The app with library filter `filter` on a fill layer's content, selected so Properties shows it
fn app_with(filter: &str) -> (Harness<'static, TextureCraftApp>, LayerId) {
    let mut session = texturecraft_engine::sample_session(256, 64).unwrap();
    let layer = execute(&mut session, "layer.add.fill", &Value::Null).unwrap();
    let effect = execute(&mut session, "effect.add.filter", &json!({"target": "content", "filter": filter})).unwrap();
    let effect = LayerId(effect["effect"].as_u64().unwrap());
    execute(&mut session, "layer.select", &json!({"layer": layer["layer"]})).unwrap();
    session.target = StackTarget::Effect(effect);

    let mut harness = Harness::builder().with_size(egui::vec2(1600.0, 1400.0)).build_eframe(|cc| TextureCraftApp::new(cc, session));
    harness.state_mut().ui.prefs.language = "en".into();
    harness.run_steps(4);
    (harness, effect)
}

#[test]
fn hsl_mode_switches_and_its_sliders_sit_at_the_middle() {
    let (mut harness, effect) = app_with("hsl");
    for label in ["Hue", "Saturation", "Lightness"] {
        let value = harness.get_by_role_and_label(Role::Slider, label).accesskit_node().numeric_value();
        assert_eq!(value, Some(0.5), "{label} unchanged reads 0.50");
    }
    harness.get_by_role_and_label(Role::RadioButton, "Regular").click();
    harness.run_steps(3);
    assert!(matches!(filter_of(&harness, effect), Filter::Hsl { perceptive: false, .. }), "{:?}", filter_of(&harness, effect));

    // A click at the right end of Saturation doubles it (stored 1)
    let track = harness.get_by_role_and_label(Role::Slider, "Saturation").rect();
    click_at(&mut harness, egui::pos2(track.right() - 1.0, track.center().y));
    harness.run_steps(3);
    assert!(matches!(filter_of(&harness, effect), Filter::Hsl { saturation, .. } if saturation > 0.95), "{:?}", filter_of(&harness, effect));
}

#[test]
fn tone_cards_fold_and_edit_their_own_tone() {
    let (mut harness, effect) = app_with("colorBalance");
    assert_eq!(harness.query_all_by_role_and_label(Role::Slider, "Cyan").count(), 3, "a Cyan row per tone");
    harness.get_by_role_and_label(Role::Button, "Midtones").click();
    harness.run_steps(3);
    assert_eq!(harness.query_all_by_role_and_label(Role::Slider, "Cyan").count(), 2, "Midtones folds");

    // Highlights comes first: its Yellow row's left end pushes the highlights to yellow
    let track = harness.query_all_by_role_and_label(Role::Slider, "Yellow").next().unwrap().rect();
    click_at(&mut harness, egui::pos2(track.left() + 1.0, track.center().y));
    harness.run_steps(3);
    let Filter::ColorBalance { highlights, shadows, midtones } = filter_of(&harness, effect) else { panic!() };
    assert!(highlights[2] < -0.9 && shadows == [0.0; 3] && midtones == [0.0; 3], "{highlights:?} {midtones:?} {shadows:?}");

    let (mut harness, effect) = app_with("colorCorrect");
    let track = harness.query_all_by_role_and_label(Role::Slider, "Saturation").nth(1).unwrap().rect();
    click_at(&mut harness, egui::pos2(track.left() + 1.0, track.center().y));
    harness.run_steps(3);
    let Filter::ColorCorrect { shadows, midtones, .. } = filter_of(&harness, effect) else { panic!() };
    assert!(midtones[2] < 0.05 && shadows[2] == 0.5, "the second card is Midtones: {midtones:?} {shadows:?}");
}

#[test]
fn blur_wrap_switches_the_edge_wrap() {
    let (mut harness, effect) = app_with("blur");
    harness.get_by_role_and_label(Role::CheckBox, "Blur Wrap").click();
    harness.run_steps(3);
    assert!(matches!(filter_of(&harness, effect), Filter::Blur { wrap: true, .. }));
}
