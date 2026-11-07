//! The levels editor in Properties: the handles over the histogram and the output gradient, the
//! values typed in after a double-click and the Reset, Auto and Invert buttons edit the effect
//! through `effect.set`.

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::{Value, json};
use texturecraft_engine::core::{Channel, EffectKind, LayerId, Levels};
use texturecraft_engine::{Session, execute};
use texturecraft_ui_egui::TextureCraftApp;

/// A fill layer with a clouds mask under a levels effect added by `command` with `params`,
/// that effect selected, and its id
fn harness(command: &str, params: Value) -> (Harness<'static, TextureCraftApp>, LayerId) {
    let mut session = texturecraft_engine::sample_session(256, 64).unwrap();
    for (id, params) in [
        ("layer.add.fill", json!({"color": [0.8, 0.3, 0.1]})),
        ("mask.add.black", Value::Null),
        ("effect.add.fill", json!({"target": "mask", "procedural": "clouds"})),
        (command, params),
    ] {
        execute(&mut session, id, &params).unwrap();
    }
    let effect = levels_effect(&session);
    open(session, effect)
}

fn open(mut session: Session, effect: LayerId) -> (Harness<'static, TextureCraftApp>, LayerId) {
    let (layer, _) = session.doc.project.effect(effect).unwrap();
    execute(&mut session, "layer.select", &json!({"layer": layer.0, "target": effect.0})).unwrap();
    let mut h = Harness::builder().with_size(egui::vec2(1600.0, 1400.0)).build_eframe(|cc| TextureCraftApp::new(cc, session));
    h.run_steps(4);
    (h, effect)
}

/// The newest levels effect in the project
fn levels_effect(session: &Session) -> LayerId {
    let layers = session.doc.project.layers.values();
    let effects = layers.flat_map(|l| l.content_effects.iter().chain(l.mask.iter().flat_map(|m| m.effects.iter())));
    effects.filter(|e| matches!(e.kind, EffectKind::Levels(_))).map(|e| e.id).max_by_key(|id| id.0).unwrap()
}

fn levels(h: &Harness<'_, TextureCraftApp>, effect: LayerId) -> Levels {
    match &h.state().session.doc.project.effect(effect).unwrap().1.kind {
        EffectKind::Levels(l) => *l,
        other => panic!("a levels effect, found {other:?}"),
    }
}

fn undo_steps(h: &Harness<'_, TextureCraftApp>) -> usize {
    h.state().session.history_labels().len()
}

fn handle(h: &Harness<'_, TextureCraftApp>, label: &str) -> egui::Pos2 {
    h.get_by_role_and_label(Role::Slider, label).rect().center()
}

/// The button labelled `label` (an effect in the stack can share its name)
fn button<'a>(h: &'a Harness<'_, TextureCraftApp>, label: &'a str) -> egui_kittest::Node<'a> {
    h.get_by_role_and_label(Role::Button, label)
}

/// Presses at `from`, moves in a few steps to `to` (one frame each, like a hand) and releases
fn drag(h: &mut Harness<'_, TextureCraftApp>, from: egui::Pos2, to: egui::Pos2) {
    h.drag_at(from);
    h.run_steps(2);
    for step in 1..=4 {
        h.hover_at(from + (to - from) * (step as f32 / 4.0));
        h.run_steps(1);
    }
    h.drop_at(to);
    h.run_steps(3);
}

fn set_levels(h: &mut Harness<'_, TextureCraftApp>, effect: LayerId, params: Value) {
    let mut params = params;
    params["effect"] = json!(effect.0);
    execute(&mut h.state_mut().session, "effect.set", &params).unwrap();
    h.run_steps(3);
}

#[test]
fn dragging_each_handle_edits_its_value_in_one_undo_step() {
    let (mut h, effect) = harness("effect.add.levels", json!({"target": "mask"}));
    for (label, offset_x) in [("Input low", 80.0), ("Input high", -80.0), ("Gamma", 30.0), ("Output low", 60.0), ("Output high", -60.0)] {
        let before = levels(&h, effect);
        let steps = undo_steps(&h);
        let at = handle(&h, label);
        drag(&mut h, at, at + egui::vec2(offset_x, 0.0));
        let after = levels(&h, effect);
        let changed: Vec<&str> = [
            ("Input low", before.in_low != after.in_low),
            ("Input high", before.in_high != after.in_high),
            ("Gamma", before.gamma != after.gamma),
            ("Output low", before.out_low != after.out_low),
            ("Output high", before.out_high != after.out_high),
        ]
        .into_iter()
        .filter_map(|(name, changed)| changed.then_some(name))
        .collect();
        assert_eq!(changed, [label], "dragging {label} changes only it: {before:?} -> {after:?}");
        assert_eq!(undo_steps(&h), steps + 1, "dragging {label} is one undo step");
    }

    let dragged = levels(&h, effect);
    assert!(dragged.in_low > 0.0 && dragged.in_high < 1.0 && dragged.out_low > 0.0 && dragged.out_high < 1.0, "{dragged:?}");
    assert!(dragged.gamma < 1.0, "gamma right of the middle darkens: {}", dragged.gamma);
}

#[test]
fn handles_stop_at_their_neighbours() {
    let (mut h, effect) = harness("effect.add.levels", json!({"target": "mask", "inLow": 0.2, "inHigh": 0.6, "outLow": 0.3, "outHigh": 0.7}));
    let far_right = egui::vec2(1000.0, 0.0);
    for label in ["Input low", "Gamma", "Output low"] {
        let at = handle(&h, label);
        drag(&mut h, at, at + far_right);
    }
    let l = levels(&h, effect);
    assert!((l.in_low - 0.59).abs() < 1e-4, "input low stops below input high: {l:?}");
    assert_eq!(l.gamma, 0.01, "gamma stops at its lowest value");
    let gap = handle(&h, "Input high").x - handle(&h, "Gamma").x;
    assert!((0.0..1.0).contains(&gap), "the gamma handle reaches input high: {gap} pt apart");
    assert!((l.out_low - 0.69).abs() < 1e-4, "output low stops below output high: {l:?}");
}

#[test]
fn output_handles_keep_their_side_once_inverted() {
    let (mut h, effect) = harness("effect.add.levels", json!({"target": "mask", "outLow": 0.2, "outHigh": 0.8}));
    button(&h, "Invert").click();
    h.run_steps(3);
    let inverted = levels(&h, effect);
    assert_eq!((inverted.out_low, inverted.out_high), (0.8, 0.2));

    let at = handle(&h, "Output high");
    drag(&mut h, at, at + egui::vec2(1000.0, 0.0));
    let l = levels(&h, effect);
    assert!((l.out_high - 0.79).abs() < 1e-4, "output high stops below output low: {l:?}");
}

#[test]
fn reset_and_invert_are_one_undo_step_each() {
    let (mut h, effect) = harness("effect.add.levels", json!({"target": "mask"}));
    assert!(button(&h, "Reset").accesskit_node().is_disabled(), "nothing to reset at the defaults");
    set_levels(&mut h, effect, json!({"inLow": 0.1, "inHigh": 0.9, "gamma": 2.0, "outLow": 0.2, "outHigh": 0.7}));

    let steps = undo_steps(&h);
    button(&h, "Invert").click();
    h.run_steps(3);
    assert_eq!(levels(&h, effect), Levels { in_low: 0.1, in_high: 0.9, gamma: 2.0, out_low: 0.7, out_high: 0.2 });
    assert_eq!(undo_steps(&h), steps + 1);

    button(&h, "Reset").click();
    h.run_steps(3);
    assert_eq!(levels(&h, effect), Levels::default());
    assert_eq!(undo_steps(&h), steps + 2);
}

#[test]
fn auto_fits_the_input_range_to_the_histogram() {
    let (mut h, effect) = harness("effect.add.levels", json!({"target": "mask", "gamma": 1.5}));
    let counts = h.state().session.effect_histogram(effect, Channel::BaseColor, 64).unwrap();
    let total: u32 = counts.iter().sum();
    let holds = |count: &u32| f64::from(*count) > f64::from(total) * 0.001;
    let low = counts.iter().position(holds).unwrap() as f32 / 64.0;
    let high = (counts.iter().rposition(holds).unwrap() + 1) as f32 / 64.0;
    assert!(low > 0.0 || high < 1.0, "the clouds don't fill the whole range: {low}..{high}");

    button(&h, "Auto").click();
    h.run_steps(3);
    let l = levels(&h, effect);
    assert_eq!((l.in_low, l.in_high), (low, high));
    assert_eq!(l.gamma, 1.5, "Auto leaves gamma");
}

/// Double-clicks `label`'s handle, then types `text` into the box that opens with the value selected
fn type_value(h: &mut Harness<'_, TextureCraftApp>, label: &str, text: &str) {
    let at = handle(h, label);
    for _ in 0..2 {
        h.event(egui::Event::PointerMoved(at));
        for pressed in [true, false] {
            h.event(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE });
        }
        h.run_steps(1);
    }
    h.run_steps(1);
    for c in text.chars() {
        h.event(egui::Event::Text(c.to_string()));
        h.run_steps(1);
    }
}

#[test]
fn a_value_typed_after_double_clicking_a_handle_applies() {
    let (mut h, effect) = harness("effect.add.levels", json!({"target": "mask"}));
    let steps = undo_steps(&h);
    type_value(&mut h, "Gamma", "2.5");
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    assert_eq!(levels(&h, effect).gamma, 2.5);
    assert_eq!(undo_steps(&h), steps + 1);
}

#[test]
fn escape_drops_a_typed_value() {
    let (mut h, effect) = harness("effect.add.levels", json!({"target": "mask"}));
    let steps = undo_steps(&h);
    type_value(&mut h, "Output low", "0.4");
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    assert_eq!(levels(&h, effect), Levels::default());
    assert_eq!(undo_steps(&h), steps);
    assert!(h.query_by_value("0.4").is_none(), "the box closed");
}

#[test]
fn a_typed_value_keeps_input_low_below_input_high() {
    let (mut h, effect) = harness("effect.add.levels", json!({"target": "mask", "inHigh": 0.5}));
    type_value(&mut h, "Input low", "0.9");
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    assert!((levels(&h, effect).in_low - 0.49).abs() < 1e-4, "{:?}", levels(&h, effect));
}

#[test]
fn the_levels_filter_shows_the_same_editor() {
    let (h, _) = harness("effect.add.filter", json!({"target": "mask", "filter": "levels"}));
    for label in ["Input low", "Gamma", "Input high", "Output low", "Output high"] {
        h.get_by_role_and_label(Role::Slider, label);
    }
    for button in ["Reset", "Auto", "Invert"] {
        h.get_by_role_and_label(Role::Button, button);
    }
}
