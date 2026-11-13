//! The compare mask in Properties: channel, the two sources with the operator between them, the constant, tolerance and hardness rows

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;
use texturecraft_engine::core::{Channel, CompareOp, CompareSource, EffectKind, LayerId};
use texturecraft_engine::execute;
use texturecraft_ui_egui::TextureCraftApp;

/// A fill over a fill, a compare mask on the top one, selected: (harness, compare effect)
fn compare_harness() -> (Harness<'static, TextureCraftApp>, LayerId) {
    let mut session = texturecraft_engine::sample_session(256, 64).unwrap();
    execute(&mut session, "layer.add.fill", &json!({"roughness": 0.2})).unwrap();
    let layer = execute(&mut session, "layer.add.fill", &json!({"roughness": 0.6})).unwrap()["layer"].clone();
    let effect = LayerId(execute(&mut session, "effect.add.compare", &json!({"layer": layer})).unwrap()["effect"].as_u64().unwrap());
    execute(&mut session, "layer.select", &json!({"layer": layer, "target": effect.0})).unwrap();
    let mut harness = Harness::builder().with_size(egui::vec2(1600.0, 1400.0)).with_step_dt(1.0 / 60.0).build_eframe(|cc| TextureCraftApp::new(cc, session));
    harness.run_steps(4);
    (harness, effect)
}

/// (left, op, right, channel, constant, tolerance, hardness) of compare mask `effect`
fn compare(harness: &Harness<'_, TextureCraftApp>, effect: LayerId) -> (CompareSource, CompareOp, CompareSource, Channel, f32, f32, f32) {
    match harness.state().session.doc.project.effect(effect).unwrap().1.kind {
        EffectKind::Compare { left, op, right, channel, value, tolerance, hardness } => (left, op, right, channel, value, tolerance, hardness),
        ref other => panic!("a compare mask, found {other:?}"),
    }
}

fn undo_steps(harness: &Harness<'_, TextureCraftApp>) -> usize {
    harness.state().session.history_labels().len()
}

fn has_slider(harness: &Harness<'_, TextureCraftApp>, label: &str) -> bool {
    harness.query_by_role_and_label(Role::Slider, label).is_some()
}

/// Opens the list of the source showing `name` and picks `option`
fn pick_source(harness: &mut Harness<'_, TextureCraftApp>, name: &str, option: &str) {
    harness.get_by_role_and_label(Role::ComboBox, name).click();
    harness.run_steps(3);
    harness.get_by_role_and_label(Role::ListBoxOption, option).click();
    harness.run_steps(4);
}

#[test]
fn the_rows_show_and_edit_the_comparison_one_undo_step_each() {
    let (mut harness, effect) = compare_harness();
    assert!(harness.query_by_label("COMPARE MASK").is_some() || harness.query_by_label("Compare mask").is_some(), "the section caption");
    assert_eq!(compare(&harness, effect).0, CompareSource::ThisLayer);
    assert!(harness.query_by_role_and_label(Role::ComboBox, "Layers below").is_some(), "the right source");
    assert!(has_slider(&harness, "Hardness") && !has_slider(&harness, "Constant") && !has_slider(&harness, "Tolerance"));

    let steps = undo_steps(&harness);
    pick_source(&mut harness, "Layers below", "Constant");
    assert_eq!(compare(&harness, effect).2, CompareSource::Constant);
    assert_eq!(undo_steps(&harness), steps + 1, "one undo step");
    assert!(has_slider(&harness, "Constant"), "a constant side shows its value");

    pick_source(&mut harness, "This layer", "This mask");
    assert_eq!(compare(&harness, effect).0, CompareSource::ThisMask);

    harness.get_by(|node| node.role() == Role::RadioButton && node.label().as_deref() == Some("≈")).click();
    harness.run_steps(4);
    assert_eq!(compare(&harness, effect).1, CompareOp::WithinTolerance);
    assert!(has_slider(&harness, "Tolerance"), "within tolerance shows its tolerance");

    pick_source(&mut harness, "Height", "Roughness");
    assert_eq!(compare(&harness, effect).3, Channel::Roughness);
    assert_eq!(undo_steps(&harness), steps + 4);
}

#[test]
fn a_source_list_leaves_the_view_its_wheel_and_closes_on_escape_or_a_click_outside() {
    let (mut harness, effect) = compare_harness();
    let open_list = |harness: &mut Harness<'_, TextureCraftApp>| {
        harness.get_by_role_and_label(Role::ComboBox, "Layers below").click();
        harness.run_steps(3);
        assert!(egui::Popup::is_any_open(&harness.ctx), "the source list opens");
    };
    open_list(&mut harness);

    let distance = harness.state().session.camera.distance;
    harness.hover_at(harness.state().viewport_rect.center());
    harness.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, 60.0),
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Wheel,
        modifiers: egui::Modifiers::NONE,
    });
    harness.run_steps(4);
    assert_ne!(harness.state().session.camera.distance, distance, "the wheel over the view zooms with the list open");

    harness.key_press(egui::Key::Escape);
    harness.run_steps(3);
    assert!(!egui::Popup::is_any_open(&harness.ctx), "Escape closes the list");

    open_list(&mut harness);
    let combo = harness.get_by_role_and_label(Role::ComboBox, "Layers below").rect();
    let outside = egui::pos2(combo.min.x - 400.0, combo.min.y);
    harness.hover_at(outside);
    for pressed in [true, false] {
        harness.event(egui::Event::PointerButton { pos: outside, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE });
        harness.run_steps(2);
    }
    assert!(!egui::Popup::is_any_open(&harness.ctx), "a click outside closes the list");
    assert_eq!(compare(&harness, effect).2, CompareSource::LayersBelow, "nothing changed");
}
