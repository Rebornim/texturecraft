//! The Filter effect card in Properties: channel pills, Intensity's Divider chip, Restore defaults on Parameters and × emptying the filter

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};
use texturecraft_engine::core::{Channel, EffectKind, Filter, LayerId, SourceWarp};
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

fn filter_of(harness: &Harness<'_, TextureCraftApp>, effect: LayerId) -> Filter {
    match &harness.state().session.doc.project.effect(effect).unwrap().1.kind {
        EffectKind::Filter(filter) => filter.clone(),
        other => panic!("not a filter: {other:?}"),
    }
}

#[test]
fn a_pill_switches_its_channel_and_the_chip_picks_the_divider() {
    let (mut harness, effect) = app_with("warp");
    harness.get_by_role_and_label(Role::CheckBox, "Rough").click();
    harness.run_steps(3);
    assert!(harness.state().session.doc.project.effect(effect).unwrap().1.off_channels.contains(&Channel::Roughness));

    harness.get_by_role_and_label(Role::ComboBox, "÷10").click();
    harness.run_steps(3);
    assert!(egui::Popup::is_any_open(&harness.ctx), "the chip opens its list");
    harness.get_by_role_and_label(Role::ListBoxOption, "÷100").click();
    harness.run_steps(3);
    assert!(matches!(filter_of(&harness, effect), Filter::SourceWarp(SourceWarp { divider: 100, .. })), "{:?}", filter_of(&harness, effect));
    assert!(!egui::Popup::is_any_open(&harness.ctx), "picking closes the list");
}

#[test]
fn restore_defaults_and_clear_on_the_card() {
    let (mut harness, effect) = app_with("blur");
    let defaults = filter_of(&harness, effect);
    harness.state_mut().run("effect.set", json!({"effect": effect.0, "radius": 40.0}));
    harness.run_steps(3);
    harness.get_by_role_and_label(Role::Button, "Restore defaults").click();
    harness.run_steps(3);
    assert_eq!(filter_of(&harness, effect), defaults);

    harness.get_by_role_and_label(Role::Button, "Clear").click();
    harness.run_steps(3);
    assert_eq!(filter_of(&harness, effect), Filter::Empty);
    assert!(harness.query_by_label("Parameters").is_none(), "an empty filter has no parameters");
    assert!(harness.query_by_role_and_label(Role::ComboBox, "Filter").is_some(), "the card reads Filter over None");
}

/// The rect of the band button named `title`
fn band_rect(harness: &Harness<'_, TextureCraftApp>, title: &str) -> egui::Rect {
    harness.get_by_role_and_label(Role::Button, title).rect()
}

#[test]
fn the_filter_bands_span_the_panel_like_the_fills() {
    let (mut harness, effect) = app_with("blur");
    let attributes = band_rect(&harness, "Attributes");

    // The fill under the filter lays its bands out on the same sheet edges
    harness.state_mut().session.target = StackTarget::Content;
    harness.run_steps(3);
    let filtering = band_rect(&harness, "Filtering and edges");
    // The fill's rows overflow and take a scroll bar, so only the left edges must be equal
    assert_eq!(attributes.left(), filtering.left(), "a filter's bands start at the panel edge like a fill's");
    assert!(attributes.right() >= filtering.right(), "and reach its right edge: {attributes:?} {filtering:?}");

    harness.state_mut().session.target = StackTarget::Effect(effect);
    harness.run_steps(3);
    assert_eq!(band_rect(&harness, "Attributes").left(), filtering.left());
}
