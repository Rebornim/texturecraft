//! The instance in Properties: INSTANCE OF holds the card of the layer it draws, a click on it selects that layer; a source that is gone shows a missing layer that takes no click

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::json;
use texturecraft_engine::core::{LayerId, LayerKind};
use texturecraft_engine::execute;
use texturecraft_ui_egui::TextureCraftApp;

/// No layer has this id
const GONE: LayerId = LayerId(999_999);

/// A fill named Steel and an instance of it (its source swapped for a layer that is gone when `dangling`), the instance selected: (harness, instance, source)
fn instance_harness(dangling: bool) -> (Harness<'static, TextureCraftApp>, LayerId, LayerId) {
    let mut session = texturecraft_engine::sample_session(256, 64).unwrap();
    let source = LayerId(execute(&mut session, "layer.add.fill", &json!({})).unwrap()["layer"].as_u64().unwrap());
    execute(&mut session, "layer.rename", &json!({"layer": source.0, "name": "Steel"})).unwrap();
    let instance = LayerId(execute(&mut session, "layer.instance", &json!({"layer": source.0})).unwrap()["layer"].as_u64().unwrap());
    if dangling {
        session.doc.project.layers.get_mut(&instance).unwrap().kind = LayerKind::Instance { source: GONE };
    }
    execute(&mut session, "layer.select", &json!({"layer": instance.0, "target": "content"})).unwrap();
    let mut harness = Harness::builder().with_size(egui::vec2(1600.0, 1400.0)).with_step_dt(1.0 / 60.0).build_eframe(|cc| TextureCraftApp::new(cc, session));
    harness.state_mut().ui.prefs.language = "en".into();
    harness.run_steps(4);
    (harness, instance, source)
}

fn set_name(harness: &Harness<'_, TextureCraftApp>) -> String {
    harness.state().session.doc.project.texture_sets[0].name.clone()
}

#[test]
fn the_source_card_names_the_source_and_its_texture_set_and_a_click_selects_it() {
    let (mut harness, instance, source) = instance_harness(false);
    assert_eq!(harness.state().session.selected_layer(), Some(instance));
    let card = format!("Steel {}", set_name(&harness));
    harness.get_by_role_and_label(Role::Button, &card).click();
    harness.run_steps(3);
    assert_eq!(harness.state().session.selected_layer(), Some(source), "the click goes to the source");
}

#[test]
fn a_source_that_is_gone_shows_a_missing_layer_that_takes_no_click() {
    let (mut harness, instance, _) = instance_harness(true);
    let card = format!("Missing layer {}", set_name(&harness));
    let missing = harness.get_by_role_and_label(Role::Button, &card);
    assert!(missing.accesskit_node().is_disabled(), "a missing source can't be gone to");
    // A real press and release on the card
    let at = missing.rect().center();
    harness.drag_at(at);
    harness.run_steps(1);
    harness.drop_at(at);
    harness.run_steps(3);
    assert_eq!(harness.state().session.selected_layer(), Some(instance));
}
