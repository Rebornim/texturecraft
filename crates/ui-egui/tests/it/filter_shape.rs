//! The shape filters in Properties: Threshold's Mode switch, Mirror's mode-dependent rows, Mask Outline's, Bevel's, Bevel Smooth's, Directional Distance's and Warp's rows

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};
use texturecraft_engine::core::{
    BevelBlur, BevelCorner, BevelDirection, EffectKind, Filter, LayerId, MirrorCorner, MirrorMode, OutlinePosition, ThresholdMode, WarpBlending, WarpMode,
    WarpSourceMode,
};
use texturecraft_engine::{StackTarget, execute};
use texturecraft_ui_egui::TextureCraftApp;

/// The app with library filter `filter` in a fill layer's mask, selected so Properties shows it
pub(crate) fn app_with(filter: &str) -> (Harness<'static, TextureCraftApp>, LayerId) {
    let mut session = texturecraft_engine::sample_session(256, 64).unwrap();
    let layer = execute(&mut session, "layer.add.fill", &Value::Null).unwrap();
    execute(&mut session, "mask.add.white", &json!({"layer": layer["layer"]})).unwrap();
    let effect = execute(&mut session, "effect.add.filter", &json!({"target": "mask", "filter": filter})).unwrap();
    let effect = LayerId(effect["effect"].as_u64().unwrap());
    execute(&mut session, "layer.select", &json!({"layer": layer["layer"]})).unwrap();
    session.target = StackTarget::Effect(effect);

    let mut harness = Harness::builder().with_size(egui::vec2(1600.0, 1400.0)).build_eframe(|cc| TextureCraftApp::new(cc, session));
    harness.state_mut().ui.prefs.language = "en".into();
    harness.run_steps(4);
    (harness, effect)
}

/// A press and release of the primary button at `pos`, as the pointer does it
pub(crate) fn click_at(harness: &mut Harness<'_, TextureCraftApp>, pos: egui::Pos2) {
    let modifiers = egui::Modifiers::NONE;
    harness.event(egui::Event::PointerMoved(pos));
    harness.event(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: true, modifiers });
    harness.run_steps(1);
    harness.event(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: false, modifiers });
    harness.run_steps(3);
}

fn filter_of(harness: &Harness<'_, TextureCraftApp>, effect: LayerId) -> Filter {
    match &harness.state().session.doc.project.effect(effect).unwrap().1.kind {
        EffectKind::Filter(filter) => filter.clone(),
        other => panic!("not a filter: {other:?}"),
    }
}

/// Threshold's slider is named Threshold and its ≤ segment sets Lower or equal
#[test]
fn threshold_mode_switch_sets_the_comparison() {
    let (mut harness, effect) = app_with("threshold");
    let level = harness.get_by_role_and_label(Role::Slider, "Threshold").rect();
    click_at(&mut harness, egui::pos2(level.right() - 1.0, level.center().y));
    let lower = harness.get_by_role_and_label(Role::RadioButton, "≤").rect();
    click_at(&mut harness, lower.center());
    let Filter::Threshold { level, mode } = filter_of(&harness, effect) else { panic!() };
    assert!(level > 0.95, "{level}");
    assert_eq!(mode, ThresholdMode::LowerOrEqual);
}

/// Mirror shows Axis X Offset and Invert Axis X, swaps them for Axis Y's, and a corner mirror shows only its Corner Type
#[test]
fn mirror_rows_follow_the_mode() {
    let (mut harness, effect) = app_with("mirror");
    let invert = harness.get_by_role_and_label(Role::CheckBox, "Invert Axis X").rect();
    click_at(&mut harness, invert.center());
    let Filter::Mirror(mirror) = filter_of(&harness, effect) else { panic!() };
    assert_eq!(mirror.invert, [true, false]);

    let mode = harness.get_by_role_and_label(Role::ComboBox, "Mirror Axis X").rect();
    click_at(&mut harness, mode.center());
    let axis_y = harness.get_by_label("Mirror Axis Y").rect();
    click_at(&mut harness, axis_y.center());
    assert!(harness.query_by_label("Axis Y Offset").is_some() && harness.query_by_label("Axis X Offset").is_none());

    let mode = harness.get_by_role_and_label(Role::ComboBox, "Mirror Axis Y").rect();
    click_at(&mut harness, mode.center());
    let corner = harness.get_by_label("Mirror Corner").rect();
    click_at(&mut harness, corner.center());
    let corner_type = harness.get_by_role_and_label(Role::ComboBox, "Top Left").rect();
    click_at(&mut harness, corner_type.center());
    let bottom_right = harness.get_by_label("Bottom Right").rect();
    click_at(&mut harness, bottom_right.center());
    let Filter::Mirror(mirror) = filter_of(&harness, effect) else { panic!() };
    assert_eq!((mirror.mode, mirror.corner), (MirrorMode::Corner, MirrorCorner::BottomRight));
    assert!(harness.query_by_label("Axis Y Offset").is_none());
}

/// Mask Outline's Both segment, Start From Black toggle, Width and the signed Curve Shape each set their own setting
#[test]
fn mask_outline_rows_set_the_band() {
    let (mut harness, effect) = app_with("maskOutline");
    // A boxed switch where its words fit the panel's width, the same choices as a dropdown where they don't
    if let Some(both) = harness.query_by_role_and_label(Role::RadioButton, "Both").map(|node| node.rect()) {
        click_at(&mut harness, both.center());
    } else {
        let list = harness.get_by_role_and_label(Role::ComboBox, "Outside").rect();
        click_at(&mut harness, list.center());
        let both = harness.get_by_label("Both").rect();
        click_at(&mut harness, both.center());
    }
    let from_black = harness.get_by_role_and_label(Role::CheckBox, "Start From Black").rect();
    click_at(&mut harness, from_black.center());
    let width = harness.get_by_role_and_label(Role::Slider, "Width").rect();
    click_at(&mut harness, egui::pos2(width.right() - 1.0, width.center().y));
    let curve = harness.get_by_role_and_label(Role::Slider, "Curve Shape").rect();
    click_at(&mut harness, egui::pos2(curve.left() + 1.0, curve.center().y));
    let Filter::MaskOutline(outline) = filter_of(&harness, effect) else { panic!() };
    assert_eq!((outline.position, outline.from_black), (OutlinePosition::Both, false));
    assert!(outline.width > 0.95 && outline.curve < -0.95, "{outline:?}");
    assert_eq!((outline.threshold, outline.blur), (1.0, 0.0), "the others stay");
}

/// Pick `option` of a setting shown as a boxed switch where its words fit, or as a dropdown showing `current` where they don't
pub(crate) fn pick(harness: &mut Harness<'_, TextureCraftApp>, current: &str, option: &str) {
    if let Some(segment) = harness.query_by_role_and_label(Role::RadioButton, option).map(|node| node.rect()) {
        click_at(harness, segment.center());
    } else {
        let list = harness.get_by_role_and_label(Role::ComboBox, current).rect();
        click_at(harness, list.center());
        let choice = harness.get_by_label(option).rect();
        click_at(harness, choice.center());
    }
}

/// Bevel's Bevel Wrap, signed Distance, Corner Type, Smoothing and Blur Mode each set their own setting
#[test]
fn bevel_rows_set_the_slope() {
    let (mut harness, effect) = app_with("bevel");
    let wrap = harness.get_by_role_and_label(Role::CheckBox, "Bevel Wrap").rect();
    click_at(&mut harness, wrap.center());
    let distance = harness.get_by_role_and_label(Role::Slider, "Distance").rect();
    click_at(&mut harness, egui::pos2(distance.right() - 1.0, distance.center().y));
    pick(&mut harness, "Round", "Angular");
    let smoothing = harness.get_by_role_and_label(Role::Slider, "Smoothing").rect();
    click_at(&mut harness, egui::pos2(smoothing.right() - 1.0, smoothing.center().y));
    pick(&mut harness, "Non-Uniform", "Uniform");
    let Filter::DistanceBevel(bevel) = filter_of(&harness, effect) else { panic!() };
    assert_eq!((bevel.wrap, bevel.corner, bevel.blur), (true, BevelCorner::Angular, BevelBlur::Uniform));
    assert!(bevel.distance > 0.95 && bevel.smoothing > 4.9, "{bevel:?}");
}

/// Bevel Smooth's Direction, Curve Offset and Bevel Wrap set their own setting; Directional Distance's Angle turns
#[test]
fn bevel_smooth_and_directional_distance_rows() {
    let (mut harness, effect) = app_with("bevelSmooth");
    pick(&mut harness, "Out", "In/Out");
    let offset = harness.get_by_role_and_label(Role::Slider, "Curve Offset").rect();
    click_at(&mut harness, egui::pos2(offset.left() + 1.0, offset.center().y));
    let wrap = harness.get_by_role_and_label(Role::CheckBox, "Bevel Wrap").rect();
    click_at(&mut harness, wrap.center());
    let Filter::BevelSmooth(smooth) = filter_of(&harness, effect) else { panic!() };
    assert_eq!((smooth.direction, smooth.wrap), (BevelDirection::InOut, true));
    assert!(smooth.curve_offset < -0.95 && smooth.distance == 0.1, "{smooth:?}");

    let (mut harness, effect) = app_with("directionalDistance");
    let angle = harness.get_by_role_and_label(Role::Slider, "Angle").rect();
    click_at(&mut harness, egui::pos2(angle.left() + 1.0, angle.center().y));
    let Filter::DirectionalDistance(trail) = filter_of(&harness, effect) else { panic!() };
    assert!(trail.angle < 5.0 && trail.distance == 0.1, "{trail:?}");
}

/// Warp shows the Angle only in the directional modes; Directions, Blending Mode, Source Mode, Source Tiling and Intensity set their own setting
#[test]
fn warp_rows_follow_the_mode() {
    let (mut harness, effect) = app_with("warp");
    assert!(harness.query_by_label("Angle").is_none() && harness.query_by_label("Source Parameters").is_some());
    // The filter card is named Warp too: the Warp Mode list is the lower one
    let mode = harness.query_all_by_role_and_label(Role::ComboBox, "Warp").map(|node| node.rect()).max_by(|a, b| a.top().total_cmp(&b.top())).unwrap();
    click_at(&mut harness, mode.center());
    let multi = harness.get_by_label("Multi Directional Warp").rect();
    click_at(&mut harness, multi.center());
    assert!(harness.query_by_role_and_label(Role::Slider, "Angle").is_some(), "a directional mode turns by its Angle");

    let two = harness.get_by_role_and_label(Role::RadioButton, "2").rect();
    click_at(&mut harness, two.center());
    pick(&mut harness, "Average", "Chain");
    pick(&mut harness, "Default Noise", "Previous Input");
    let tiling = harness.get_by_role_and_label(Role::Slider, "Source Tiling").rect();
    click_at(&mut harness, egui::pos2(tiling.right() - 1.0, tiling.center().y));
    let intensity = harness.get_by_role_and_label(Role::Slider, "Intensity").rect();
    click_at(&mut harness, egui::pos2(intensity.right() - 1.0, intensity.center().y));

    let Filter::SourceWarp(warp) = filter_of(&harness, effect) else { panic!() };
    assert_eq!(
        (warp.mode, warp.directions, warp.blending, warp.source.mode),
        (WarpMode::MultiDirectional, 2, WarpBlending::Chain, WarpSourceMode::PreviousInput)
    );
    assert!(warp.source.tiling > 15.9 && warp.intensity > 0.95, "{warp:?}");
    assert_eq!((warp.seed, warp.divider, warp.source.balance), (1, 10, 0.5), "the others stay");
}
