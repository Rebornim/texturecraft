//! The Smudge panel shows which channels a smudge drags and shares their on/off flags with the Material tab

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::{Value, json};
use texturecraft_engine::core::Channel;
use texturecraft_engine::geom::primitives;
use texturecraft_engine::{Session, Tool, execute};
use texturecraft_ui_egui::TextureCraftApp;

fn harness() -> Harness<'static, TextureCraftApp> {
    let session = Session::new_project("t", primitives::sphere(1.0, 48, 24), "builtin:sphere", None, 256).unwrap();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1600.0, 960.0))
        .with_context_setup(texturecraft_ui_egui::framework::install_headless)
        .build_eframe(|context| TextureCraftApp::new(context, session));
    harness.run_steps(4);
    run(&mut harness, "tool.smudge", Value::Null);
    harness
}

fn run(harness: &mut Harness<'_, TextureCraftApp>, command: &str, params: Value) {
    execute(&mut harness.state_mut().session, command, &params).unwrap();
    harness.run_steps(4);
}

/// The Smudge panel's chip for `channel`: the top one, over the Material section's
fn smudge_chip<'a>(harness: &'a Harness<'_, TextureCraftApp>, channel: Channel) -> egui_kittest::Node<'a> {
    let chips: Vec<_> = harness.get_all_by_role_and_label(Role::Button, channel.short()).collect();
    assert_eq!(chips.len(), 2, "the Smudge panel and the Material section each draw a {} chip", channel.short());
    chips.into_iter().min_by(|a, b| a.rect().top().total_cmp(&b.rect().top())).unwrap()
}

fn chip_is_on(harness: &Harness<'_, TextureCraftApp>, channel: Channel) -> bool {
    smudge_chip(harness, channel).accesskit_node().toggled() == Some(egui::accesskit::Toggled::True)
}

/// Channels whose chip is on, in the order of `Channel::ALL`
fn chips_on(harness: &Harness<'_, TextureCraftApp>) -> Vec<Channel> {
    Channel::ALL
        .iter()
        .copied()
        .filter(|channel| harness.query_all_by_role_and_label(Role::Button, channel.short()).next().is_some() && chip_is_on(harness, *channel))
        .collect()
}

fn enabled(harness: &Harness<'_, TextureCraftApp>) -> Vec<Channel> {
    let tool = &harness.state().session.tool;
    Channel::ALL.iter().copied().filter(|channel| tool.enabled.contains(channel)).collect()
}

#[test]
fn smudge_panel_shows_the_channels_the_material_has_on() {
    let mut harness = harness();
    assert_eq!(harness.state().session.tool.tool, Tool::Smudge);
    assert!(harness.query_by_label("Channels dragged").is_some());
    assert_eq!(chips_on(&harness), enabled(&harness));
    for channel in [Channel::BaseColor, Channel::Metallic, Channel::Roughness, Channel::Normal, Channel::Height] {
        assert!(chip_is_on(&harness, channel), "{channel:?} is on by default");
    }

    run(&mut harness, "tool.material", json!({"channel": "roughness", "enabled": false}));

    assert_eq!(chips_on(&harness), enabled(&harness));
    assert!(!chip_is_on(&harness, Channel::Roughness));
}

#[test]
fn clicking_a_channel_in_the_smudge_panel_turns_it_off_and_on_for_the_material() {
    let mut harness = harness();

    smudge_chip(&harness, Channel::Metallic).click();
    harness.run_steps(4);
    assert!(!harness.state().session.tool.enabled.contains(&Channel::Metallic));
    assert!(!chip_is_on(&harness, Channel::Metallic));

    smudge_chip(&harness, Channel::Metallic).click();
    harness.run_steps(4);
    assert!(harness.state().session.tool.enabled.contains(&Channel::Metallic));
    assert!(chip_is_on(&harness, Channel::Metallic));
}

#[test]
fn material_tab_and_smudge_panel_stay_in_sync() {
    let mut harness = harness();
    // Both the Smudge panel and the Material section draw a chip per channel
    let chips = harness.get_all_by_role_and_label(Role::Button, Channel::Height.short()).count();
    assert_eq!(chips, 2, "Smudge panel and Material tab");

    let first = harness.get_all_by_role_and_label(Role::Button, Channel::Height.short()).next().unwrap();
    first.click();
    harness.run_steps(4);

    assert!(!harness.state().session.tool.enabled.contains(&Channel::Height));
    for chip in harness.get_all_by_role_and_label(Role::Button, Channel::Height.short()) {
        assert_ne!(chip.accesskit_node().toggled(), Some(egui::accesskit::Toggled::True), "both chips show height off");
    }
}
