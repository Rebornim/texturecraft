//! The simple adjustment filters in Properties: Invert's switch, Height Adjust's Invert above its sliders, Histogram Scan's
//! Invert Position and Posterize's Apply to Alpha under theirs, Pixelate and FXAA (Anti-aliasing)'s switches, and the reference's names on
//! Clamp, Smoothstep, Histogram Shift, Sharpen, Blur and Blur Directional; Highpass's and Grayscale Conversion's rows

use egui::accesskit::Role;
use egui_kittest::kittest::{NodeT, Queryable};
use texturecraft_engine::core::{ConversionMode, Filter, HighpassSpace};

use crate::filter_shape::{app_with, click_at, pick};
use crate::filter_support::filter_of;

/// Invert's only row is its switch: a click turns it off, another back on
#[test]
fn invert_switch_turns_it_off_and_on() {
    let (mut harness, effect) = app_with("invert");
    let switch = harness.get_by_role_and_label(Role::CheckBox, "Invert").rect();
    click_at(&mut harness, switch.center());
    assert_eq!(filter_of(&harness, effect), Filter::InvertOff);
    let switch = harness.get_by_role_and_label(Role::CheckBox, "Invert").rect();
    click_at(&mut harness, switch.center());
    assert_eq!(filter_of(&harness, effect), Filter::Invert);
}

/// Height Adjust: Invert above Offset and Multiply; the switch flips it and Multiply's track sets it
#[test]
fn height_adjust_rows() {
    let (mut harness, effect) = app_with("heightAdjust");
    let invert = harness.get_by_role_and_label(Role::CheckBox, "Invert").rect();
    let offset = harness.get_by_role_and_label(Role::Slider, "Offset").rect();
    let multiply = harness.get_by_role_and_label(Role::Slider, "Multiply").rect();
    assert!(invert.center().y < offset.center().y && offset.center().y < multiply.center().y, "Invert, Offset, Multiply");
    click_at(&mut harness, invert.center());
    click_at(&mut harness, egui::pos2(multiply.right() - 1.0, multiply.center().y));
    let Filter::HeightAdjust { invert, offset, multiply } = filter_of(&harness, effect) else { panic!() };
    assert!(invert && offset == 0.0 && multiply > 9.5, "{invert} {offset} {multiply}");
}

/// Histogram Scan's Invert Position sits under Position and Contrast; Posterize's Apply to Alpha under Colors Amount
#[test]
fn switches_under_the_sliders() {
    let (mut harness, effect) = app_with("histogramScan");
    let contrast = harness.get_by_role_and_label(Role::Slider, "Contrast").rect();
    let invert = harness.get_by_role_and_label(Role::CheckBox, "Invert Position").rect();
    assert!(invert.center().y > contrast.center().y);
    click_at(&mut harness, invert.center());
    assert!(matches!(filter_of(&harness, effect), Filter::HistogramScan { invert_position: true, .. }));

    let (mut harness, effect) = app_with("posterize");
    let amount = harness.get_by_role_and_label(Role::Slider, "Colors Amount").rect();
    let alpha = harness.get_by_role_and_label(Role::CheckBox, "Apply to Alpha").rect();
    assert!(alpha.center().y > amount.center().y);
    click_at(&mut harness, alpha.center());
    assert_eq!(filter_of(&harness, effect), Filter::Posterize { levels: 8.0, alpha: true });
}

/// Clamp's Min and Max, Smoothstep's Minimum and Maximum, Histogram Shift's Position (its left end shifts by a whole turn back) and Sharpen Intensity
#[test]
fn reference_names_on_the_sliders() {
    for (filter, labels) in [("clamp", &["Min", "Max"][..]), ("smoothstep", &["Minimum", "Maximum"]), ("sharpen", &["Sharpen Intensity"])] {
        let (harness, _) = app_with(filter);
        for label in labels {
            assert!(harness.query_by_role_and_label(Role::Slider, label).is_some(), "{filter}: {label}");
        }
    }
    let (mut harness, effect) = app_with("histogramShift");
    let position = harness.get_by_role_and_label(Role::Slider, "Position").rect();
    click_at(&mut harness, egui::pos2(position.center().x, position.center().y));
    let Filter::HistogramShift { offset } = filter_of(&harness, effect) else { panic!() };
    assert!((offset + 0.5).abs() < 0.05, "the middle is a half turn back: {offset}");
}

/// Pixelate: Pixels Amount (32) with Apply To Alpha under it; FXAA (Anti-aliasing): only its Affect Alpha Channel switch
#[test]
fn pixelate_and_fxaa_rows() {
    let (mut harness, effect) = app_with("pixelate");
    let amount = harness.get_by_role_and_label(Role::Slider, "Pixels Amount");
    assert_eq!(amount.accesskit_node().numeric_value().map(f64::round), Some(32.0));
    let amount = amount.rect();
    let alpha = harness.get_by_role_and_label(Role::CheckBox, "Apply To Alpha").rect();
    assert!(alpha.center().y > amount.center().y);
    click_at(&mut harness, alpha.center());
    assert!(matches!(filter_of(&harness, effect), Filter::Mosaic { alpha: false, .. }));
    click_at(&mut harness, egui::pos2(amount.right() - 1.0, amount.center().y));
    let Filter::Mosaic { size, .. } = filter_of(&harness, effect) else { panic!() };
    assert!((Filter::pixels_amount(size) - 128.0).abs() < 0.5, "the right end is 128 across: {size}");

    let (mut harness, effect) = app_with("fxaa");
    let affect = harness.get_by_role_and_label(Role::CheckBox, "Affect Alpha Channel").rect();
    click_at(&mut harness, affect.center());
    assert_eq!(filter_of(&harness, effect), Filter::Fxaa { alpha: true });
}

/// Blur Intensity and Blur Directional's Intensity and Angle (its dial) take the reference's names
#[test]
fn blur_rows_take_the_reference_names() {
    let (harness, _) = app_with("blur");
    let intensity = harness.get_by_role_and_label(Role::Slider, "Blur Intensity");
    assert_eq!(intensity.accesskit_node().numeric_value().map(|v| (v * 100.0).round()), Some(30.0));
    let (mut harness, effect) = app_with("blurDirectional");
    let intensity = harness.get_by_role_and_label(Role::Slider, "Intensity").rect();
    assert!(harness.query_by_role_and_label(Role::Slider, "Angle").is_some());
    click_at(&mut harness, egui::pos2(intensity.right() - 1.0, intensity.center().y));
    assert!(matches!(filter_of(&harness, effect), Filter::DirectionalBlur { radius, .. } if radius > 250.0));
}

/// Highpass: Color Space, Radius and Apply To Alpha, each setting its own value
#[test]
fn highpass_rows() {
    let (mut harness, effect) = app_with("highPass");
    let radius = harness.get_by_role_and_label(Role::Slider, "Radius");
    assert_eq!(radius.accesskit_node().numeric_value().map(|v| (v * 100.0).round()), Some(300.0));
    let radius = radius.rect();
    let space = harness.get_by_role_and_label(Role::ComboBox, "Working space / Mask").rect();
    let alpha = harness.get_by_role_and_label(Role::CheckBox, "Apply To Alpha").rect();
    assert!(space.center().y < radius.center().y && radius.center().y < alpha.center().y, "the reference's order");
    pick(&mut harness, "Working space / Mask", "sRGB (from ACEScg)");
    click_at(&mut harness, alpha.center());
    assert_eq!(filter_of(&harness, effect), Filter::HighPass { radius: 48.0, space: HighpassSpace::SrgbFromAcescg, alpha: true });
    click_at(&mut harness, egui::pos2(radius.right() - 1.0, radius.center().y));
    assert!(matches!(filter_of(&harness, effect), Filter::HighPass { radius, .. } if radius > 250.0), "Radius 16 = 256 texels at 2K");
}

/// Grayscale Conversion: Channel Input (Custom Input shows its slot right under it), Invert, Balance, Contrast, Mode and the weights in Channels Weights only
#[test]
fn grayscale_conversion_rows() {
    let (mut harness, effect) = app_with("grayscale");
    let invert = harness.get_by_role_and_label(Role::CheckBox, "Invert").rect().center().y;
    let sliders = ["Balance", "Contrast", "Red", "Green", "Blue", "Alpha"].map(|label| harness.get_by_role_and_label(Role::Slider, label).rect().center().y);
    let rows: Vec<f32> = std::iter::once(invert).chain(sliders).collect();
    assert!(rows.windows(2).all(|pair| pair[0] < pair[1]), "the reference's order: {rows:?}");
    assert!(harness.query_by_role_and_label(Role::ComboBox, "Custom Input").is_none(), "no slot for the current channel");
    pick(&mut harness, "Current Channel", "Custom Input");
    let Filter::GrayscaleConversion(conversion) = filter_of(&harness, effect) else { panic!() };
    assert!(conversion.custom_input);

    // The lowest: Channel Input shows the same name where its words don't fit a switch
    let slot = harness.query_all_by_role_and_label(Role::ComboBox, "Custom Input").map(|node| node.rect()).max_by(|a, b| a.top().total_cmp(&b.top())).unwrap();
    let invert = harness.get_by_role_and_label(Role::CheckBox, "Invert").rect();
    assert!(slot.center().y < invert.center().y, "its slot right under Channel Input");
    click_at(&mut harness, invert.center());
    pick(&mut harness, "Channels Weights", "Max");
    let Filter::GrayscaleConversion(conversion) = filter_of(&harness, effect) else { panic!() };
    assert!(conversion.invert && conversion.mode == ConversionMode::Max);
    assert!(harness.query_by_label("Red").is_none(), "no weights outside Channels Weights");
}
