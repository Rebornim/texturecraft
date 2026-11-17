//! The shape filters in Properties: Threshold's Mode switch, Mirror's mode-dependent rows, Mask Outline's, Bevel's, Bevel Smooth's, Directional Distance's, Warp's (its Directional mode and Angle) and Blur Slope's rows (its ÷ chip, Seed and Source Parameters' Blur)

use crate::filter_support::{WINDOW_HEIGHT, along, app_with, click_at, filter_of, lowest, pick, topmost, undo, undo_depth};
use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use texturecraft_engine::core::{
    BevelBlur, BevelCorner, BevelDirection, Filter, MirrorCorner, MirrorMode, OutlinePosition, SlopeBlending, ThresholdMode, WarpBlending, WarpMode,
    WarpSourceMode,
};
use texturecraft_ui_egui::TextureCraftApp;

/// Threshold's slider is named Threshold and its ≤ segment sets Lower or equal
#[test]
fn threshold_mode_switch_sets_the_comparison() {
    let (mut harness, effect) = app_with("threshold", "mask", WINDOW_HEIGHT);

    let level = harness.get_by_role_and_label(Role::Slider, "Threshold").rect();
    click_at(&mut harness, along(level, 1.0));
    let lower = harness.get_by_role_and_label(Role::RadioButton, "≤").rect();
    click_at(&mut harness, lower.center());

    let Filter::Threshold { level, mode } = filter_of(&harness, effect) else { panic!() };
    assert!(level > 0.95, "{level}");
    assert_eq!(mode, ThresholdMode::LowerOrEqual);
}

/// Mirror shows Axis X Offset and Invert Axis X, swaps them for Axis Y's, and a corner mirror shows only its Corner Type
#[test]
fn mirror_rows_follow_the_mode() {
    let (mut harness, effect) = app_with("mirror", "mask", WINDOW_HEIGHT);
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
    let (mut harness, effect) = app_with("maskOutline", "mask", WINDOW_HEIGHT);

    pick(&mut harness, "Outside", "Both");
    let from_black = harness.get_by_role_and_label(Role::CheckBox, "Start From Black").rect();
    click_at(&mut harness, from_black.center());
    let width = harness.get_by_role_and_label(Role::Slider, "Width").rect();
    click_at(&mut harness, along(width, 1.0));
    let curve = harness.get_by_role_and_label(Role::Slider, "Curve Shape").rect();
    click_at(&mut harness, along(curve, 0.0));

    let Filter::MaskOutline(outline) = filter_of(&harness, effect) else { panic!() };
    assert_eq!((outline.position, outline.from_black), (OutlinePosition::Both, false));
    assert!(outline.width > 0.95 && outline.curve < -0.95, "{outline:?}");
    assert_eq!((outline.threshold, outline.blur), (1.0, 0.0), "the others stay");
}

/// Bevel's Bevel Wrap, signed Distance, Corner Type, Smoothing and Blur Mode each set their own setting
#[test]
fn bevel_rows_set_the_slope() {
    let (mut harness, effect) = app_with("bevel", "mask", WINDOW_HEIGHT);

    let wrap = harness.get_by_role_and_label(Role::CheckBox, "Bevel Wrap").rect();
    click_at(&mut harness, wrap.center());
    let distance = harness.get_by_role_and_label(Role::Slider, "Distance").rect();
    click_at(&mut harness, along(distance, 1.0));
    pick(&mut harness, "Round", "Angular");
    let smoothing = harness.get_by_role_and_label(Role::Slider, "Smoothing").rect();
    click_at(&mut harness, along(smoothing, 1.0));
    pick(&mut harness, "Non-Uniform", "Uniform");

    let Filter::DistanceBevel(bevel) = filter_of(&harness, effect) else { panic!() };
    assert_eq!((bevel.wrap, bevel.corner, bevel.blur), (true, BevelCorner::Angular, BevelBlur::Uniform));
    assert!(bevel.distance > 0.95 && bevel.smoothing > 4.9, "{bevel:?}");
}

/// Bevel Smooth's Direction, Curve Offset and Bevel Wrap set their own setting; Directional Distance's Angle turns
#[test]
fn bevel_smooth_and_directional_distance_rows() {
    let (mut harness, effect) = app_with("bevelSmooth", "mask", WINDOW_HEIGHT);

    pick(&mut harness, "Out", "In/Out");
    let offset = harness.get_by_role_and_label(Role::Slider, "Curve Offset").rect();
    click_at(&mut harness, along(offset, 0.0));
    let wrap = harness.get_by_role_and_label(Role::CheckBox, "Bevel Wrap").rect();
    click_at(&mut harness, wrap.center());

    let Filter::BevelSmooth(smooth) = filter_of(&harness, effect) else { panic!() };
    assert_eq!((smooth.direction, smooth.wrap), (BevelDirection::InOut, true));
    assert!(smooth.curve_offset < -0.95 && smooth.distance == 0.1, "{smooth:?}");

    let (mut harness, effect) = app_with("directionalDistance", "mask", WINDOW_HEIGHT);
    let angle = harness.get_by_role_and_label(Role::Slider, "Angle").rect();

    click_at(&mut harness, along(angle, 0.0));

    let Filter::DirectionalDistance(trail) = filter_of(&harness, effect) else { panic!() };
    assert!(trail.angle < 5.0 && trail.distance == 0.1, "{trail:?}");
}

/// Warp shows the Angle only in the directional modes; Directions, Blending Mode, Source Mode, Source Tiling and Intensity set their own setting
#[test]
fn warp_rows_follow_the_mode() {
    let (mut harness, effect) = app_with("warp", "mask", WINDOW_HEIGHT);
    assert!(harness.query_by_label("Angle").is_none() && harness.query_by_label("Source Parameters").is_some());

    // The filter card is named Warp too: the Warp Mode list is the lower one
    let mode = lowest(&harness, Role::ComboBox, "Warp");
    click_at(&mut harness, mode.center());
    let multi = harness.get_by_label("Multi Directional Warp").rect();
    click_at(&mut harness, multi.center());

    assert!(harness.query_by_role_and_label(Role::Slider, "Angle").is_some(), "a directional mode turns by its Angle");

    let two = harness.get_by_role_and_label(Role::RadioButton, "2").rect();
    click_at(&mut harness, two.center());
    pick(&mut harness, "Average", "Chain");
    pick(&mut harness, "Default Noise", "Previous Input");
    let tiling = harness.get_by_role_and_label(Role::Slider, "Source Tiling").rect();
    click_at(&mut harness, along(tiling, 1.0));
    let intensity = harness.get_by_role_and_label(Role::Slider, "Intensity").rect();
    click_at(&mut harness, along(intensity, 1.0));

    let Filter::SourceWarp(warp) = filter_of(&harness, effect) else { panic!() };
    assert_eq!(
        (warp.mode, warp.directions, warp.blending, warp.source.mode),
        (WarpMode::MultiDirectional, 2, WarpBlending::Chain, WarpSourceMode::PreviousInput)
    );
    assert!(warp.source.tiling > 15.9 && warp.intensity > 0.95, "{warp:?}");
    assert_eq!((warp.seed, warp.divider, warp.source.balance), (0, 10, 0.5), "the others stay");
}

/// Blur Slope shows the reference's rows in order (no Slope scale); Blending Mode, Source Type and Quality set their own setting
#[test]
fn slope_blur_rows_set_the_smear() {
    let (mut harness, effect) = app_with("slopeBlur", "mask", WINDOW_HEIGHT);
    assert!(harness.query_by_label("Slope scale").is_none());
    let top = |harness: &Harness<'_, TextureCraftApp>, role: Role, label: &str| harness.get_by_role_and_label(role, label).rect().top();
    let order = [
        top(&harness, Role::Slider, "Intensity"),
        top(&harness, Role::ComboBox, "Blur"),
        top(&harness, Role::Slider, "Quality"),
        harness.get_by_label("Source Parameters").rect().top(),
        top(&harness, Role::ComboBox, "Default Noise"),
        top(&harness, Role::Slider, "Blur"),
        top(&harness, Role::Slider, "Position"),
        top(&harness, Role::Slider, "Contrast"),
        top(&harness, Role::Slider, "Source Tiling"),
    ];
    assert!(order.windows(2).all(|pair| pair[0] < pair[1]), "{order:?}");

    pick(&mut harness, "Blur", "Max");
    pick(&mut harness, "Default Noise", "Custom Noise");
    let quality = harness.get_by_role_and_label(Role::Slider, "Quality").rect();
    click_at(&mut harness, along(quality, 0.0));

    let Filter::SourceSlopeBlur(slope) = filter_of(&harness, effect) else { panic!() };
    assert_eq!((slope.blending, slope.source.mode), (SlopeBlending::Max, WarpSourceMode::CustomNoise));
    assert!(slope.quality < 0.05, "{slope:?}");
    assert_eq!((slope.seed, slope.divider, slope.source.balance), (0, 10, 0.5), "the others stay");

    // Source Type's list reads Custom Noise too: the input's slot is the lower, between it and Blur
    let source_type = topmost(&harness, Role::ComboBox, "Custom Noise");
    let slot = lowest(&harness, Role::ComboBox, "Custom Noise");
    let blur = harness.get_by_role_and_label(Role::Slider, "Blur").rect();
    assert!(source_type.bottom() <= slot.top() && slot.bottom() <= blur.top(), "the Custom Noise input under Source Type: {source_type:?} {slot:?} {blur:?}");
}

/// Warp's Directional Warp (a list pick) shows its Angle, whose track turns it; the list pick is one undo step
#[test]
fn warp_directional_mode_and_its_angle() {
    let (mut harness, effect) = app_with("warp", "mask", WINDOW_HEIGHT);
    let depth = undo_depth(&harness);

    // The filter card is named Warp too: the Warp Mode list is the lower one
    let mode = lowest(&harness, Role::ComboBox, "Warp");
    click_at(&mut harness, mode.center());
    let directional = harness.get_by_label("Directional Warp").rect();
    click_at(&mut harness, directional.center());

    assert!(matches!(filter_of(&harness, effect), Filter::SourceWarp(warp) if warp.mode == WarpMode::Directional));
    assert_eq!(undo_depth(&harness), depth + 1, "one undo step for the pick");

    let angle = harness.get_by_role_and_label(Role::Slider, "Angle").rect();
    click_at(&mut harness, along(angle, 0.5));

    let Filter::SourceWarp(warp) = filter_of(&harness, effect) else { panic!("{:?}", filter_of(&harness, effect)) };
    assert!((warp.angle - 180.0).abs() < 0.5, "half way round: {}", warp.angle);

    undo(&mut harness);
    undo(&mut harness);

    assert!(matches!(filter_of(&harness, effect), Filter::SourceWarp(warp) if warp.mode == WarpMode::Warp), "{:?}", filter_of(&harness, effect));
}

/// Blur Slope's ÷ chip picks the divider, the dice picks a new Seed, and Source Parameters' Blur shows 0..20 over the stored 0..1
#[test]
fn slope_blur_divider_seed_and_source_blur() {
    let (mut harness, effect) = app_with("slopeBlur", "mask", WINDOW_HEIGHT);

    let chip = harness.get_by_role_and_label(Role::ComboBox, "÷10").rect();
    click_at(&mut harness, chip.center());
    let thousand = harness.get_by_role_and_label(Role::ListBoxOption, "÷1000").rect();
    click_at(&mut harness, thousand.center());
    let dice = harness.get_by_role_and_label(Role::Button, "Random").rect();
    click_at(&mut harness, dice.center());
    let blur = harness.get_by_role_and_label(Role::Slider, "Blur").rect();
    click_at(&mut harness, along(blur, 0.5));

    let Filter::SourceSlopeBlur(slope) = filter_of(&harness, effect) else { panic!("{:?}", filter_of(&harness, effect)) };
    assert_eq!(slope.divider, 1000);
    assert_ne!(slope.seed, 0, "a new seed");
    assert!((slope.source.blur - 0.5).abs() < 0.01, "stored on 0..1: {}", slope.source.blur);
    let shown = harness.get_by_role_and_label(Role::Slider, "Blur").accesskit_node().numeric_value().unwrap();
    assert!((shown - 10.0).abs() < 0.2, "shown on 0..20: {shown}");
}

/// Mirror's Display Debug switch and its axis offset track set their own setting
#[test]
fn mirror_debug_and_offset() {
    let (mut harness, effect) = app_with("mirror", "mask", WINDOW_HEIGHT);

    let debug = harness.get_by_role_and_label(Role::CheckBox, "Display Debug").rect();
    click_at(&mut harness, debug.center());
    let offset = harness.get_by_role_and_label(Role::Slider, "Axis X Offset").rect();
    click_at(&mut harness, along(offset, 1.0));

    let Filter::Mirror(mirror) = filter_of(&harness, effect) else { panic!("{:?}", filter_of(&harness, effect)) };
    assert!(mirror.debug);
    assert_eq!(mirror.offset, [1.0, 0.5], "Axis X's offset only");
}
