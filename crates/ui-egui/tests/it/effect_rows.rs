//! Effect rows in the layer stack: dragging one reorders its stack, and its blend mode and opacity sit at its right.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};
use texturecraft_engine::core::LayerId;
use texturecraft_engine::execute;
use texturecraft_ui_egui::TextureCraftApp;

fn id(v: Value, key: &str) -> u64 {
    v[key].as_u64().unwrap()
}

/// Drag the row labelled `from` onto the upper (`below` false) or lower half of the row `to`.
fn drag(h: &mut Harness<'_, TextureCraftApp>, from: &str, to: &str, below: bool) {
    let start = h.get_by_label(from).rect().center();
    let r = h.get_by_label(to).rect();
    let end = egui::pos2(r.center().x, if below { r.bottom() - 4.0 } else { r.top() + 4.0 });
    h.drag_at(start);
    h.run_steps(2);
    h.hover_at(end);
    h.run_steps(2);
    h.drop_at(end);
    h.run_steps(3);
}

#[test]
fn dragging_an_effect_row_reorders_its_stack() {
    let mut session = texturecraft_engine::sample_session(256, 64).unwrap();
    let layer = LayerId(id(execute(&mut session, "layer.add.fill", &Value::Null).unwrap(), "layer"));
    let mut add =
        |name: &str| id(execute(&mut session, "effect.add.filter", &json!({"target": "content", "filter": "invert", "name": name})).unwrap(), "effect");
    let (a, b, c) = (add("Effect A"), add("Effect B"), add("Effect C"));
    execute(&mut session, "layer.select", &json!({"layer": layer.0})).unwrap();
    let mut h = Harness::builder().with_size(egui::vec2(1600.0, 1400.0)).build_eframe(|cc| TextureCraftApp::new(cc, session));
    h.run_steps(4);
    let order = |h: &Harness<'_, TextureCraftApp>| h.state().session.doc.project.layers[&layer].content_effects.iter().map(|e| e.id.0).collect::<Vec<_>>();
    assert_eq!(order(&h), [a, b, c]);
    // The rows list the top first: C, B, A. A dropped on C's upper half goes to the top.
    drag(&mut h, "Effect A", "Effect C", false);
    assert_eq!(order(&h), [b, c, a]);
    // A dropped on B's lower half goes back to the bottom.
    drag(&mut h, "Effect A", "Effect B", true);
    assert_eq!(order(&h), [a, b, c]);
    // C dropped on A's upper half lands between A and B.
    drag(&mut h, "Effect C", "Effect A", false);
    assert_eq!(order(&h), [a, c, b]);
}

/// An effect row's blend mode menu and opacity field sit at its right, as on layer rows.
#[test]
fn an_effect_row_sets_its_blend_mode_and_opacity() {
    let mut session = texturecraft_engine::sample_session(256, 64).unwrap();
    let layer = LayerId(id(execute(&mut session, "layer.add.fill", &Value::Null).unwrap(), "layer"));
    let effect = id(execute(&mut session, "effect.add.filter", &json!({"target": "content", "filter": "invert", "name": "Effect A"})).unwrap(), "effect");
    execute(&mut session, "layer.select", &json!({"layer": layer.0})).unwrap();
    let mut h = Harness::builder().with_size(egui::vec2(1600.0, 1400.0)).build_eframe(|cc| TextureCraftApp::new(cc, session));
    h.run_steps(4);
    let effect_of = |h: &Harness<'_, TextureCraftApp>| h.state().session.doc.project.layers[&layer].content_effects[0].clone();
    assert_eq!(effect, effect_of(&h).id.0);

    // The mode menu is the left part of the pair at the row's right edge, the opacity field the right part
    let row = h.get_by_label("Effect A").rect();
    let mode_at = egui::pos2(row.right() - 4.0 - 96.0 + 20.0, row.center().y);
    h.drag_at(mode_at);
    h.drop_at(mode_at);
    h.run_steps(2);
    h.get_by_label("Multiply").click();
    h.run_steps(3);
    assert_eq!(effect_of(&h).blend, texturecraft_engine::core::BlendMode::Multiply);

    let opacity_at = egui::pos2(row.right() - 4.0 - 20.0, row.center().y);
    h.drag_at(opacity_at);
    h.run_steps(2);
    h.hover_at(opacity_at - egui::vec2(100.0, 0.0));
    h.run_steps(2);
    h.drop_at(opacity_at - egui::vec2(100.0, 0.0));
    h.run_steps(3);
    assert!(effect_of(&h).opacity < 0.9, "dragging the field to the left lowers the opacity: {}", effect_of(&h).opacity);
}
