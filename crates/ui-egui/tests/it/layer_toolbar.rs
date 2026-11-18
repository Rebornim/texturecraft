//! The Layers panel's top row: Add effect greys what can't go where the selection is, Add mask adds a mask with its effect, and
//! Bitmap adds nothing until a texture is picked

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::{Value, json};
use texturecraft_engine::core::{Channel, EffectKind, LayerId, Source};
use texturecraft_engine::paint::alphas;
use texturecraft_engine::{StackTarget, execute};
use texturecraft_ui_egui::TextureCraftApp;

use crate::{BAKE_RESOLUTION, SETTLE_FRAMES, STARTUP_FRAMES, TEXTURE_RESOLUTION, click_at, click_widget};

/// Tall enough for the toolbar, the layer rows and their menus at once
const WINDOW_SIZE: egui::Vec2 = egui::vec2(1600.0, 1200.0);

/// Frames past the tooltip delay at the harness's step
const TOOLTIP_DELAY_FRAMES: usize = 60;

/// Slack, in points, for a list row that starts on the list's own bottom edge
const EDGE_TOLERANCE: f32 = 1.0;

/// The app with a selected fill layer, its content the target
fn app() -> (Harness<'static, TextureCraftApp>, LayerId) {
    let mut session = texturecraft_engine::sample_session(TEXTURE_RESOLUTION, BAKE_RESOLUTION).unwrap();
    let layer = LayerId(execute(&mut session, "layer.add.fill", &Value::Null).unwrap()["layer"].as_u64().unwrap());
    execute(&mut session, "layer.select", &json!({"layer": layer.0})).unwrap();
    let mut harness = Harness::builder().with_size(WINDOW_SIZE).build_eframe(|cc| TextureCraftApp::new(cc, session));
    harness.state_mut().ui.prefs.language = "en".into();
    harness.run_steps(STARTUP_FRAMES);
    (harness, layer)
}

/// Opens the row's menu `menu` and clicks its item `item`
fn pick(harness: &mut Harness<'_, TextureCraftApp>, menu: &str, item: &str) {
    click_widget(harness, Role::Button, menu);
    click_widget(harness, Role::Button, item);
}

fn mask_kinds(harness: &Harness<'_, TextureCraftApp>, layer: LayerId) -> Vec<EffectKind> {
    let project = &harness.state().session.doc.project;
    project.layer(layer).unwrap().mask.as_ref().map(|mask| mask.effects.iter().map(|effect| effect.kind.clone()).collect()).unwrap_or_default()
}

#[test]
fn add_effect_greys_mask_only_effects_on_the_content_and_adds_them_in_the_mask() {
    let (mut harness, layer) = app();
    let steps = harness.state().session.history_labels().len();
    for item in ["Compare mask", "Color selection", "Generator"] {
        pick(&mut harness, "Add effect", item);
        assert_eq!(harness.state().session.history_labels().len(), steps, "{item} is greyed on the layer content, not sent to the mask");
        harness.key_press(egui::Key::Escape);
        harness.run_steps(SETTLE_FRAMES);
    }
    assert!(harness.state().session.doc.project.layer(layer).unwrap().mask.is_none());

    harness.state_mut().run("mask.add.white", json!({"layer": layer.0}));
    harness.run_steps(SETTLE_FRAMES);
    pick(&mut harness, "Add effect", "Color selection");
    let kinds = mask_kinds(&harness, layer);
    assert!(matches!(kinds.as_slice(), [EffectKind::ColorSelection(_)]), "{kinds:?}");
    let mask = harness.state().session.doc.project.layer(layer).unwrap().mask.as_ref().unwrap();
    let added = mask.effects.iter().find(|effect| matches!(effect.kind, EffectKind::ColorSelection(_))).unwrap().id;
    assert_eq!(harness.state().session.target, StackTarget::Effect(added), "the new colour selection is selected");
}

#[test]
fn add_mask_adds_a_height_combination_and_greys_once_the_layer_has_a_mask() {
    let (mut harness, layer) = app();
    pick(&mut harness, "Add mask", "Height combination");
    let kinds = mask_kinds(&harness, layer);
    assert!(matches!(kinds.as_slice(), [EffectKind::Compare { .. }]), "{kinds:?}");

    let steps = harness.state().session.history_labels().len();
    pick(&mut harness, "Add mask", "White");
    assert_eq!(harness.state().session.history_labels().len(), steps, "a layer with a mask adds to it through Add effect");
}

#[test]
fn bitmap_opens_the_texture_picker_and_adds_the_mask_only_once_one_is_picked() {
    let (mut harness, layer) = app();
    pick(&mut harness, "Add mask", "Bitmap");
    assert!(egui::Popup::is_any_open(&harness.ctx), "Bitmap opens the texture picker");
    assert!(harness.state().session.doc.project.layer(layer).unwrap().mask.is_none(), "no mask until a texture is picked");
    harness.key_press(egui::Key::Escape);
    harness.run_steps(SETTLE_FRAMES);
    assert!(!egui::Popup::is_any_open(&harness.ctx));
    assert!(harness.state().session.doc.project.layer(layer).unwrap().mask.is_none(), "Escape adds nothing");

    let alpha = alphas::find("round_soft").unwrap();
    pick(&mut harness, "Add mask", "Bitmap");
    let tile = harness.get_by_label(&alpha.label).rect();
    click_at(&mut harness, tile.center());
    let kinds = mask_kinds(&harness, layer);
    let name = texturecraft_engine::fill_alpha::resource_name(alpha);
    assert!(matches!(kinds.as_slice(), [EffectKind::Fill { source: Source::Image { name: read, .. }, .. }] if *read == name), "{kinds:?}");
}

#[test]
fn a_toolbar_tooltip_shows_above_its_button_off_the_layer_rows() {
    let (mut harness, _) = app();
    let button = harness.get_by_role_and_label(Role::Button, "Add effect").rect();
    harness.hover_at(button.center());

    // Past the tooltip delay
    harness.run_steps(TOOLTIP_DELAY_FRAMES);
    let tip =
        harness.get_all_by_label("Add effect").find(|node| node.accesskit_node().role() == Role::Label).map(|node| node.rect()).expect("the button's tooltip");
    assert!(tip.bottom() <= button.top(), "the tooltip sits above the toolbar: {tip:?} {button:?}");
}

/// Where the Layers panel's channel list shows `current`, if it does (the topmost list showing it: Properties sits below)
fn channel_list(harness: &Harness<'_, TextureCraftApp>, current: &str) -> Option<egui::Rect> {
    harness.query_all_by_role_and_label(Role::ComboBox, current).map(|node| node.rect()).min_by(|a, b| a.top().total_cmp(&b.top()))
}

/// Opens the Layers panel's channel list showing `current` and picks `item` in it (the nearest `item` under the list)
fn pick_channel(harness: &mut Harness<'_, TextureCraftApp>, current: &str, item: &str) {
    let list = channel_list(harness, current).unwrap();
    click_at(harness, list.center());

    let below = |rect: egui::Rect| rect.top() >= list.bottom() - EDGE_TOLERANCE && rect.left() < list.right() && rect.right() > list.left();
    let row = harness.query_all_by_label(item).map(|node| node.rect()).filter(|rect| below(*rect)).min_by(|a, b| a.top().total_cmp(&b.top())).unwrap();

    // A real press and release on the row, as a mouse would
    click_at(harness, row.center());
}

#[test]
fn the_channel_list_starts_on_material_and_picks_a_channel_or_material_again() {
    let (mut harness, _) = app();
    let start = channel_list(&harness, "Material");
    assert!(start.is_some(), "the list starts on the whole material");

    pick_channel(&mut harness, "Material", "Roughness");

    assert_eq!(channel_list(&harness, "Roughness"), start, "the list shows the channel picked");
    assert!(!harness.state().ui.layer_material_view);
    assert_eq!(harness.state().ui.layer_channel, Channel::Roughness);

    pick_channel(&mut harness, "Roughness", "Material");

    assert!(harness.state().ui.layer_material_view, "Material shows the whole material again");
    assert_eq!(harness.state().ui.layer_channel, Channel::Roughness, "the rows keep the last channel's blending");
}
