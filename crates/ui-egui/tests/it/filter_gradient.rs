//! The gradient filters in Properties: the ramp's markers, Color Quantity, Gradient Curve's Preset list and Gradient Dynamic's orientation

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};
use texturecraft_engine::core::{CurvePreset, EffectKind, Filter, LayerId};
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

/// A press and release of the primary button at `pos`, as the pointer does it
fn click_at(harness: &mut Harness<'_, TextureCraftApp>, pos: egui::Pos2) {
    let modifiers = egui::Modifiers::NONE;
    harness.event(egui::Event::PointerMoved(pos));
    harness.event(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: true, modifiers });
    harness.run_steps(1);
    harness.event(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: false, modifiers });
}

fn filter_of(harness: &Harness<'_, TextureCraftApp>, effect: LayerId) -> Filter {
    match &harness.state().session.doc.project.effect(effect).unwrap().1.kind {
        EffectKind::Filter(filter) => filter.clone(),
        other => panic!("not a filter: {other:?}"),
    }
}

/// A press at `from`, a drag to `to` in steps and a release, as the pointer does it
fn drag(harness: &mut Harness<'_, TextureCraftApp>, from: egui::Pos2, to: egui::Pos2) {
    let modifiers = egui::Modifiers::NONE;
    harness.event(egui::Event::PointerMoved(from));
    harness.event(egui::Event::PointerButton { pos: from, button: egui::PointerButton::Primary, pressed: true, modifiers });
    harness.run_steps(1);
    for step in 1..=4 {
        harness.event(egui::Event::PointerMoved(from + (to - from) * (step as f32 / 4.0)));
        harness.run_steps(1);
    }
    harness.event(egui::Event::PointerButton { pos: to, button: egui::PointerButton::Primary, pressed: false, modifiers });
    harness.run_steps(2);
}

/// Dragging Color 2's marker along the ramp moves its place, and one undo puts it back
#[test]
fn a_gradient_marker_drags_its_color_along_the_ramp() {
    let (mut harness, effect) = app_with("gradientMap");
    let marker = harness.get_all_by_role_and_label(Role::Slider, "Color 2").next().unwrap().rect();
    drag(&mut harness, marker.center(), marker.center() + egui::vec2(60.0, 0.0));
    let Filter::GradientMap { positions, .. } = filter_of(&harness, effect) else { panic!() };
    let moved = positions.get(1).copied().unwrap();
    assert!(moved > 0.6, "dragged right: {positions:?}");
    execute(&mut harness.state_mut().session, "edit.undo", &Value::Null).unwrap();
    let Filter::GradientMap { positions, .. } = filter_of(&harness, effect) else { panic!() };
    assert_eq!(positions.get(1), Some(&0.5), "one undo step for the drag");
}

/// Color Quantity at its right end shows ten color rows
#[test]
fn color_quantity_adds_color_rows() {
    let (mut harness, effect) = app_with("gradientMap");
    assert!(harness.query_by_label("Color 4").is_none());
    let track = harness.get_by_role_and_label(Role::Slider, "Color Quantity").rect();
    click_at(&mut harness, egui::pos2(track.right() - 1.0, track.center().y));
    harness.run_steps(3);
    assert_eq!(filter_of(&harness, effect).ramp_colors().len(), 10);
    assert!(harness.query_all_by_label("Color 10").next().is_some(), "the tenth color has its row");
}

/// Gradient Curve's Preset list picks a curve
#[test]
fn gradient_curve_preset_list_picks_a_curve() {
    let (mut harness, effect) = app_with("gradientCurve");
    harness.get_by_role_and_label(Role::ComboBox, "Round").click();
    harness.run_steps(3);
    harness.get_by_label("Bump").click();
    harness.run_steps(3);
    assert!(matches!(filter_of(&harness, effect), Filter::GradientCurve { preset: CurvePreset::Bump, .. }), "{:?}", filter_of(&harness, effect));
}

/// Gradient Dynamic's orientation turns vertical: a boxed switch at the panel's width, Vertical one click away
#[test]
fn gradient_dynamic_orientation_turns_vertical() {
    let (mut harness, effect) = app_with("gradientDynamic");
    assert!(harness.query_by_role_and_label(Role::ComboBox, "Horizontal").is_none(), "both choices fit side by side");
    harness.get_by_label("Vertical").click();
    harness.run_steps(3);
    assert!(matches!(filter_of(&harness, effect), Filter::GradientDynamic { vertical: true, .. }), "{:?}", filter_of(&harness, effect));
}

#[test]
fn gradient_dynamic_source_card_opens_the_color_picker() {
    let (mut harness, _) = app_with("gradientDynamic");
    assert!(harness.query_by_role_and_label(Role::TitleBar, "Gradient Source").is_none(), "no picker yet");
    // Through the pointer, on the card's name: the card's whole row picks its source
    let card = harness.get_by_role_and_label(Role::ComboBox, "Gradient Source").rect();
    click_at(&mut harness, egui::pos2(card.center().x, card.center().y));
    harness.run_steps(4);
    assert!(harness.query_by_role_and_label(Role::TitleBar, "Gradient Source").is_some(), "the card opens the colour picker of its bar");
}
