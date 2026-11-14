//! The Filter effect card in Properties: channel pills, Intensity's Divider chip, the card holding the parameters with its ⟲ beside × and Attributes as its last row

use crate::filter_support::filter_of;
use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::{Value, json};
use texturecraft_engine::core::{Channel, Filter, LayerId, SourceWarp};
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
    let restore = harness.get_by_role_and_label(Role::Button, "Restore defaults");
    assert!(restore.accesskit_node().is_disabled(), "inert at the defaults");
    let clear = harness.get_by_role_and_label(Role::Button, "Clear").rect();
    assert!((restore.rect().center().y - clear.center().y).abs() < 0.5 && restore.rect().right() < clear.left(), "⟲ beside × on the card");

    harness.state_mut().run("effect.set", json!({"effect": effect.0, "radius": 40.0}));
    harness.run_steps(3);
    harness.get_by_role_and_label(Role::Button, "Restore defaults").click();
    harness.run_steps(3);
    assert_eq!(filter_of(&harness, effect), defaults);

    harness.get_by_role_and_label(Role::Button, "Clear").click();
    harness.run_steps(3);
    assert_eq!(filter_of(&harness, effect), Filter::Empty);
    assert!(harness.query_by_label("Attributes").is_none(), "an empty filter has no settings");
    assert!(harness.query_by_role_and_label(Role::ComboBox, "Filter").is_some(), "the card reads Filter over None");
}

/// The filter's card holds its rows: no Parameters band, every row and the Attributes row inside the card's edges, Attributes last
#[test]
fn the_filter_card_holds_its_parameters() {
    let (harness, _) = app_with("slopeBlur");
    assert!(harness.query_by_label("Parameters").is_none(), "no Parameters band");
    let card = harness.get_by_role_and_label(Role::ComboBox, "Blur Slope").rect();
    let attributes = harness.get_by_role_and_label(Role::Button, "Attributes").rect();
    let sub_group = harness.get_by_role_and_label(Role::Button, "Source Parameters").rect();

    // A row inside the card keeps the card's bleed on both sides; a band would span the panel
    assert_eq!(attributes.left(), card.left() + 6.0, "{attributes:?} {card:?}");
    assert_eq!(sub_group.left(), attributes.left(), "a sub-group is a caption in the card, not a card of its own");
    for slider in harness.query_all_by_role(Role::Slider) {
        assert!(slider.rect().bottom() <= attributes.top(), "Attributes is the card's last row: {:?}", slider.rect());
    }
}

/// Air between the bottom of slider `last` and the top of the Attributes row under it, with library filter `filter` shown
fn air_under(filter: &str, last: &str) -> f32 {
    let (harness, _) = app_with(filter);
    let slider = harness.get_by_role_and_label(Role::Slider, last).rect();
    harness.get_by_role_and_label(Role::Button, "Attributes").rect().top() - slider.bottom()
}

#[test]
fn a_generic_filter_ends_its_parameters_like_a_dedicated_one() {
    // Sharpen's rows come from the library's parameters, Mask Outline's from its own rows; both end on a slider
    assert_eq!(air_under("sharpen", "Sharpen Intensity"), air_under("maskOutline", "Curve Shape"));
}

#[test]
fn replace_colors_two_colors_are_colour_bars_with_their_eyedroppers() {
    let (mut harness, effect) = app_with("replaceColor");
    assert_eq!(harness.get_all_by_role_and_label(Role::Button, "Pick color").count(), 2, "an eyedropper beside each color");

    harness.get_by_role_and_label(Role::Button, "Replacement color").click();
    harness.run_steps(3);
    harness.get_by_role_and_label(Role::TextInput, "Hex color").click();
    harness.run_steps(3);
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    harness.event(egui::Event::Text("FF0000".into()));
    harness.key_press(egui::Key::Enter);
    harness.run_steps(3);
    let Filter::ReplaceColor { to, .. } = filter_of(&harness, effect) else { panic!("a Replace Color filter") };
    assert_eq!(to, [1.0, 0.0, 0.0], "the bar's picker sets the replacement color");
}
