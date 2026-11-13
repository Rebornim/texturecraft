//! Filter image inputs and Attributes in Properties: Bevel Smooth's Distance Map slot under Image inputs, Warp's Custom Noise slot under its Source Mode, the read-only Attributes band

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use texturecraft_engine::core::{LayerId, ProceduralKind, Source};
use texturecraft_ui_egui::TextureCraftApp;

use crate::filter_shape::{app_with, click_at, pick};

fn input(harness: &Harness<'_, TextureCraftApp>, effect: LayerId, id: &str) -> Option<Source> {
    harness.state().session.doc.project.effect(effect).unwrap().1.inputs.get(id).cloned()
}

/// A click on the control of `role` named `label` (the lowest, where a list above shows the same name)
fn click(harness: &mut Harness<'_, TextureCraftApp>, role: Role, label: &str) {
    let at = lowest(harness, role, label).center();
    click_at(harness, at);
}

/// The lowest control of `role` named `label` (a slot below a list showing the same name)
fn lowest(harness: &Harness<'_, TextureCraftApp>, role: Role, label: &str) -> egui::Rect {
    harness.query_all_by_role_and_label(role, label).map(|node| node.rect()).max_by(|a, b| a.top().total_cmp(&b.top())).unwrap()
}

/// The Distance Map slot picks a pattern from the source picker, × puts it back to white, its ramp sets a gray
#[test]
fn the_distance_map_slot_picks_its_source() {
    let (mut harness, effect) = app_with("bevelSmooth");
    assert!(harness.query_by_label("Image inputs").is_some());
    click(&mut harness, Role::ComboBox, "Distance Map");
    assert!(harness.query_by_role_and_label(Role::Tab, "Anchor points").is_some(), "the slot opens the source picker");
    click(&mut harness, Role::ListBoxOption, "Clouds");
    let Some(Source::Procedural { pattern, low, high }) = input(&harness, effect, "distanceMap") else { panic!("a procedural") };
    assert_eq!((pattern.kind, low, high), (ProceduralKind::Clouds, [0.0; 3], [1.0; 3]));
    assert!(!egui::Popup::is_any_open(&harness.ctx), "a pick closes the picker");

    // The filter card has its own ×: the slot's is the lower one
    click(&mut harness, Role::Button, "Clear");
    assert_eq!(input(&harness, effect, "distanceMap"), None, "back to its default white");
    let ramp = harness.get_by_role_and_label(Role::Slider, "Distance Map").rect();
    click_at(&mut harness, egui::pos2(ramp.left() + 1.0, ramp.center().y));
    let Some(Source::Uniform([gray, ..])) = input(&harness, effect, "distanceMap") else { panic!("a gray") };
    assert!(gray < 0.05, "{gray}");
}

/// Warp shows its Custom Noise slot right under Source Mode only in that mode, above the source's sliders
#[test]
fn the_custom_noise_slot_follows_the_source_mode() {
    let (mut harness, effect) = app_with("warp");
    assert!(harness.query_by_role_and_label(Role::ComboBox, "Custom Noise").is_none());
    pick(&mut harness, "Default Noise", "Custom Noise");
    let slot = lowest(&harness, Role::ComboBox, "Custom Noise");
    let blur = harness.get_by_role_and_label(Role::Slider, "Source Blur").rect();
    assert!(slot.bottom() <= blur.top(), "the slot sits above Source Blur");
    click_at(&mut harness, slot.center());
    click(&mut harness, Role::ListBoxOption, "Clouds");
    assert!(matches!(input(&harness, effect, "customNoise"), Some(Source::Procedural { .. })));
    assert!(harness.query_by_label("Image inputs").is_none(), "a mode's input is not under Image inputs");
}

/// Attributes is closed at first; a click on its band shows the filter's read-only rows under Image inputs, another hides them
#[test]
fn the_attributes_band_opens_on_a_click() {
    let (mut harness, _) = app_with("bevelSmooth");
    let category = "Category: Filter";
    assert!(harness.query_by_label(category).is_none(), "closed by default");
    let band = lowest(&harness, Role::Button, "Attributes");
    assert!(band.top() > lowest(&harness, Role::Button, "Image inputs").bottom(), "under Image inputs");
    click_at(&mut harness, band.center());
    assert!(harness.query_by_label(category).is_some());
    assert!(harness.query_by_label("Description: Draws a smooth ramp from the edge of the shapes.").is_some());
    assert!(harness.query_by_label("Tags: bevel, edge, ramp").is_some());
    assert!(harness.query_by_label("Size: 256 × 256").is_some(), "the texture set's size");
    assert!(harness.get_by_label(category).rect().top() > band.bottom(), "the rows hang under the band");
    click_at(&mut harness, band.center());
    assert!(harness.query_by_label(category).is_none());
}
