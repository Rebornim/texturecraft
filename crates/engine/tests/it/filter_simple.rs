//! The simple adjustment filters: Clamp, Smoothstep, Invert's switch, Height Adjust, Histogram Scan's Invert Position,
//! Histogram Shift's Position, Sharpen, Posterize, Pixelate, FXAA (Anti-aliasing), the blurs, Highpass and Grayscale Conversion

use serde_json::json;
use texturecraft_engine::core::{Channel, ConversionMode, EffectKind, Filter, GrayscaleConversion, HighpassSpace, LayerId, lum};
use texturecraft_engine::{Session, execute};

use crate::filter_shape::{SIZE, add_filter, roughness, session};

fn filter_of(s: &Session, effect: LayerId) -> Filter {
    match &s.doc.project.effect(effect).unwrap().1.kind {
        EffectKind::Filter(filter) => filter.clone(),
        other => panic!("not a filter: {other:?}"),
    }
}

/// A session with one roughness `value` fill
fn fill(value: f32) -> Session {
    let mut s = session();
    execute(&mut s, "layer.add.fill", &json!({"color": [0.5, 0.5, 0.5], "roughness": value})).unwrap();
    s
}

const TEXEL: (u32, u32) = (SIZE / 4, SIZE / 2);

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 2e-3
}

/// The library defaults leave the values as they are (Posterize steps them, Invert inverts)
#[test]
fn new_simple_filters_start_where_the_reference_does() {
    let cases = [
        ("clamp", Filter::Clamp { min: 0.0, max: 1.0 }),
        ("smoothstep", Filter::Smoothstep { min: 0.0, max: 1.0 }),
        ("heightAdjust", Filter::HeightAdjust { invert: false, offset: 0.0, multiply: 1.0 }),
        ("histogramScan", Filter::HistogramScan { position: 0.5, contrast: 0.0, invert_position: false }),
        ("histogramShift", Filter::HistogramShift { offset: 0.0 }),
        ("posterize", Filter::Posterize { levels: 8.0, alpha: false }),
        ("invert", Filter::Invert),
    ];
    for (id, defaults) in cases {
        let mut s = fill(0.25);
        let effect = add_filter(&mut s, json!({"filter": id}));
        assert_eq!(filter_of(&s, effect), defaults, "{id}");
    }
    let mut s = fill(0.25);
    add_filter(&mut s, json!({"filter": "clamp"}));
    add_filter(&mut s, json!({"filter": "heightAdjust"}));
    add_filter(&mut s, json!({"filter": "histogramScan"}));
    add_filter(&mut s, json!({"filter": "histogramShift"}));
    assert!(close(roughness(&mut s, TEXEL), 0.25), "Clamp, Height Adjust, Histogram Scan and Shift at their defaults change nothing");
}

/// Smoothstep eases between its bounds; Clamp keeps within them
#[test]
fn smoothstep_eases_between_minimum_and_maximum() {
    let mut s = fill(0.25);
    let effect = add_filter(&mut s, json!({"filter": "smoothstep"}));
    assert!(close(roughness(&mut s, TEXEL), 0.15625), "0.25 eased: 3t² − 2t³");
    execute(&mut s, "effect.set", &json!({"effect": effect.0, "min": 0.25, "max": 0.75})).unwrap();
    assert!(close(roughness(&mut s, TEXEL), 0.0), "at the minimum");
    execute(&mut s, "effect.set", &json!({"effect": effect.0, "min": 0.0, "max": 0.2})).unwrap();
    assert!(close(roughness(&mut s, TEXEL), 1.0), "past the maximum");
    execute(&mut s, "effect.set", &json!({"effect": effect.0, "min": 0.25, "max": 0.25})).unwrap();
    assert!(close(roughness(&mut s, TEXEL), 1.0), "equal bounds are a step, the bound itself white");
}

/// Invert's switch turns it off and on in one undo step each; restore defaults turns it back on
#[test]
fn invert_switches_off_and_on() {
    let mut s = fill(0.25);
    let effect = add_filter(&mut s, json!({"filter": "invert"}));
    assert!(close(roughness(&mut s, TEXEL), 0.75));
    execute(&mut s, "effect.set", &json!({"effect": effect.0, "invert": false})).unwrap();
    assert_eq!(filter_of(&s, effect), Filter::InvertOff);
    assert!(close(roughness(&mut s, TEXEL), 0.25), "off changes nothing");
    assert_eq!(filter_of(&s, effect).label(), "Invert", "still named Invert");
    assert!(execute(&mut s, "effect.set", &json!({"effect": effect.0, "invert": "yes"})).is_err());
    execute(&mut s, "edit.undo", &json!({})).unwrap();
    assert_eq!(filter_of(&s, effect), Filter::Invert, "one undo step");
    execute(&mut s, "effect.set", &json!({"effect": effect.0, "invert": false})).unwrap();
    execute(&mut s, "effect.filter.restoreDefaults", &json!({"effect": effect.0})).unwrap();
    assert_eq!(filter_of(&s, effect), Filter::Invert);
    // An old project's Invert still reads as before
    let old: Filter = serde_json::from_value(json!("Invert")).unwrap();
    assert_eq!(old, Filter::Invert);
}

/// Height Adjust flips, scales and raises around mid gray
#[test]
fn height_adjust_flips_scales_and_raises() {
    let mut s = fill(0.25);
    let effect = add_filter(&mut s, json!({"filter": "heightAdjust"}));
    execute(&mut s, "effect.set", &json!({"effect": effect.0, "multiply": 2.0})).unwrap();
    assert!(close(roughness(&mut s, TEXEL), 0.0), "(0.25 − 0.5) × 2 + 0.5");
    execute(&mut s, "effect.set", &json!({"effect": effect.0, "invert": true, "offset": 0.1})).unwrap();
    assert!(close(roughness(&mut s, TEXEL), 1.0), "flipped to 0.75, (0.75 − 0.5) × 2 + 0.5 + 0.1 clamped");
    execute(&mut s, "effect.set", &json!({"effect": effect.0, "multiply": 1.0, "offset": -0.2})).unwrap();
    assert!(close(roughness(&mut s, TEXEL), 0.55));
    execute(&mut s, "effect.set", &json!({"effect": effect.0, "multiply": 50.0})).unwrap();
    assert_eq!(filter_of(&s, effect), Filter::HeightAdjust { invert: true, offset: -0.2, multiply: 10.0 }, "Multiply caps at 10");
}

/// Histogram Scan's Invert Position reads the position from the other end; old saves have none
#[test]
fn histogram_scan_inverts_its_position() {
    let mut s = fill(0.25);
    let effect = add_filter(&mut s, json!({"filter": "histogramScan"}));
    execute(&mut s, "effect.set", &json!({"effect": effect.0, "position": 0.2, "contrast": 1.0})).unwrap();
    assert!(close(roughness(&mut s, TEXEL), 1.0), "0.25 is above 0.2");
    execute(&mut s, "effect.set", &json!({"effect": effect.0, "invertPosition": true})).unwrap();
    assert!(close(roughness(&mut s, TEXEL), 0.0), "0.25 is below 1 − 0.2");
    let old: Filter = serde_json::from_value(json!({"HistogramScan": {"position": 0.5, "contrast": 0.5}})).unwrap();
    assert_eq!(old, Filter::HistogramScan { position: 0.5, contrast: 0.5, invert_position: false });
    assert_eq!(serde_json::to_value(&old).unwrap(), json!({"HistogramScan": {"position": 0.5, "contrast": 0.5}}), "saved as before");
}

/// Histogram Shift's Position: 1 shifts nothing, lower shifts back by 1 − position; offsets saved before read as themselves
#[test]
fn histogram_shift_takes_a_position() {
    let mut s = fill(0.25);
    let effect = add_filter(&mut s, json!({"filter": "histogramShift"}));
    execute(&mut s, "effect.set", &json!({"effect": effect.0, "position": 0.5})).unwrap();
    assert_eq!(filter_of(&s, effect), Filter::HistogramShift { offset: -0.5 });
    assert!(close(roughness(&mut s, TEXEL), 0.75), "0.25 − 0.5 wraps to 0.75");
    assert_eq!(Filter::shift_position(-0.5), 0.5);
    assert_eq!(Filter::shift_position(0.25), 0.25, "an old positive offset shows as itself");
    assert_eq!(Filter::shift_position(0.0), 1.0);
}

/// Posterize steps to its Colors Amount and takes Apply to Alpha; old saves read without it and save as before
#[test]
fn posterize_takes_apply_to_alpha() {
    let mut s = fill(0.3);
    let effect = add_filter(&mut s, json!({"filter": "posterize"}));
    execute(&mut s, "effect.set", &json!({"effect": effect.0, "levels": 2, "applyToAlpha": true})).unwrap();
    assert_eq!(filter_of(&s, effect), Filter::Posterize { levels: 2.0, alpha: true });
    assert!(close(roughness(&mut s, TEXEL), 0.0));
    assert!(execute(&mut s, "effect.set", &json!({"effect": effect.0, "applyToAlpha": 1})).is_err());
    let old: Filter = serde_json::from_value(json!({"Posterize": {"levels": 4.0}})).unwrap();
    assert_eq!(old, Filter::Posterize { levels: 4.0, alpha: false });
    assert_eq!(serde_json::to_value(&old).unwrap(), json!({"Posterize": {"levels": 4.0}}), "saved as before");
}

/// Pixelate takes Pixels Amount (cells across the UV tile) and Apply To Alpha; Mosaics saved before load and save as before
#[test]
fn pixelate_takes_pixels_amount_and_apply_to_alpha() {
    let mut s = fill(0.3);
    let effect = add_filter(&mut s, json!({"filter": "pixelate"}));
    assert_eq!(filter_of(&s, effect), Filter::Mosaic { size: 64.0, alpha: true }, "32 pixels across, coverage stepped");
    execute(&mut s, "effect.set", &json!({"effect": effect.0, "pixelsAmount": 7.6, "applyToAlpha": false})).unwrap();
    let Filter::Mosaic { size, alpha } = filter_of(&s, effect) else { panic!() };
    assert!(close(Filter::pixels_amount(size), 8.0) && !alpha, "a whole Pixels Amount: {size}");
    assert!(close(roughness(&mut s, TEXEL), 0.3), "a uniform fill stays");
    assert!(execute(&mut s, "effect.set", &json!({"effect": effect.0, "applyToAlpha": "no"})).is_err());
    let inspect = execute(&mut s, "project.inspect", &json!({})).unwrap().to_string();
    assert!(inspect.contains("Pixelate"), "named after the reference workflow");
    let old: Filter = serde_json::from_value(json!({"Mosaic": {"size": 32.0}})).unwrap();
    assert_eq!(old, Filter::Mosaic { size: 32.0, alpha: true });
    assert_eq!(serde_json::to_value(&old).unwrap(), json!({"Mosaic": {"size": 32.0}}), "saved as before");
}

/// FXAA (Anti-aliasing) takes Affect Alpha Channel (off for a new one) and leaves flat values as they are
#[test]
fn fxaa_takes_affect_alpha_channel() {
    let mut s = fill(0.3);
    let effect = add_filter(&mut s, json!({"filter": "FXAA (Anti-aliasing)"}));
    assert_eq!(filter_of(&s, effect), Filter::Fxaa { alpha: false });
    execute(&mut s, "effect.set", &json!({"effect": effect.0, "affectAlphaChannel": true})).unwrap();
    assert_eq!(filter_of(&s, effect), Filter::Fxaa { alpha: true });
    assert!(close(roughness(&mut s, TEXEL), 0.3), "nothing to smooth on a flat fill");
    assert!(execute(&mut s, "effect.set", &json!({"effect": effect.0, "affectAlphaChannel": 2})).is_err());
    execute(&mut s, "edit.undo", &json!({})).unwrap();
    assert_eq!(filter_of(&s, effect), Filter::Fxaa { alpha: false }, "one undo step");
}

/// Blur and Blur Directional take Intensity in the reference workflow's unit (16 texels at 2K each); radius still works
#[test]
fn blurs_take_intensity() {
    let mut s = fill(0.3);
    let blur = add_filter(&mut s, json!({"filter": "blur"}));
    assert_eq!(filter_of(&s, blur), Filter::Blur { radius: 4.8, wrap: false }, "a new Blur starts at Blur Intensity 0.30");
    execute(&mut s, "effect.set", &json!({"effect": blur.0, "intensity": 2.0})).unwrap();
    assert_eq!(filter_of(&s, blur), Filter::Blur { radius: 32.0, wrap: false });
    execute(&mut s, "effect.set", &json!({"effect": blur.0, "intensity": 1000.0})).unwrap();
    assert_eq!(filter_of(&s, blur), Filter::Blur { radius: 256.0, wrap: false }, "capped as radius is");
    let directional = add_filter(&mut s, json!({"filter": "blurDirectional"}));
    execute(&mut s, "effect.set", &json!({"effect": directional.0, "intensity": 0.5, "angle": 90})).unwrap();
    assert_eq!(filter_of(&s, directional), Filter::DirectionalBlur { radius: 8.0, angle: 90.0 });
    execute(&mut s, "effect.set", &json!({"effect": directional.0, "radius": 4.0})).unwrap();
    assert_eq!(filter_of(&s, directional), Filter::DirectionalBlur { radius: 4.0, angle: 90.0 }, "radius in texels at 2K still sets it");
}

/// Highpass takes Radius in Blur Intensity's unit (a new one 3), Color Space and Apply To Alpha; a Highpass saved before loads and saves as before
#[test]
fn highpass_takes_radius_color_space_and_apply_to_alpha() {
    let mut s = fill(0.3);
    let effect = add_filter(&mut s, json!({"filter": "Highpass"}));
    assert_eq!(filter_of(&s, effect), Filter::HighPass { radius: 48.0, space: HighpassSpace::Working, alpha: false });
    assert!(close(roughness(&mut s, TEXEL), 0.5), "a flat fill has no detail: mid gray");
    execute(&mut s, "effect.set", &json!({"effect": effect.0, "intensity": 0.5, "colorSpace": "srgbFromAcescg", "applyToAlpha": true})).unwrap();
    assert_eq!(filter_of(&s, effect), Filter::HighPass { radius: 8.0, space: HighpassSpace::SrgbFromAcescg, alpha: true });
    assert!(execute(&mut s, "effect.set", &json!({"effect": effect.0, "colorSpace": "rec2020"})).is_err());
    assert!(execute(&mut s, "effect.set", &json!({"effect": effect.0, "applyToAlpha": "yes"})).is_err());
    execute(&mut s, "edit.undo", &json!({})).unwrap();
    assert_eq!(filter_of(&s, effect), Filter::HighPass { radius: 48.0, space: HighpassSpace::Working, alpha: false }, "one undo step");
    let old: Filter = serde_json::from_value(json!({"HighPass": {"radius": 8.0}})).unwrap();
    assert_eq!(old, Filter::HighPass { radius: 8.0, space: HighpassSpace::Working, alpha: false });
    assert_eq!(serde_json::to_value(&old).unwrap(), json!({"HighPass": {"radius": 8.0}}), "saved as before");
}

/// The composited base color at the test texel
fn base_color(s: &mut Session) -> [f32; 3] {
    s.update();
    s.caches[0].composite.value(Channel::BaseColor, TEXEL.0, TEXEL.1)
}

/// A new Grayscale Conversion weighs a third of each component; a Grayscale saved before keeps its luminance, also after its first edit
#[test]
fn grayscale_conversion_takes_its_settings_and_keeps_saved_grayscales() {
    let mut s = session();
    execute(&mut s, "layer.add.fill", &json!({"color": [0.8, 0.2, 0.1]})).unwrap();
    let effect = add_filter(&mut s, json!({"filter": "Grayscale Conversion"}));
    assert_eq!(filter_of(&s, effect), Filter::GrayscaleConversion(GrayscaleConversion::LIBRARY));
    s.doc.project.effect_mut(effect).unwrap().kind = EffectKind::Filter(Filter::Grayscale);
    let saved = base_color(&mut s);
    assert!(saved.iter().all(|v| close(*v, saved[0])) && close(saved[0], lum([0.8, 0.2, 0.1])), "{saved:?}");

    execute(&mut s, "effect.set", &json!({"effect": effect.0, "balance": 0.5})).unwrap();
    assert_eq!(filter_of(&s, effect), Filter::GrayscaleConversion(GrayscaleConversion::of_luminance()), "the luminance weights");
    assert!(close(base_color(&mut s)[0], saved[0]), "looks the same after its first edit");

    execute(&mut s, "effect.set", &json!({"effect": effect.0, "red": 1.0, "green": 0.0, "blue": 0.0, "invert": true})).unwrap();
    assert!(close(base_color(&mut s)[0], 1.0 - 0.8), "Red only, inverted");
    execute(&mut s, "effect.set", &json!({"effect": effect.0, "mode": "min", "invert": false})).unwrap();
    assert!(close(base_color(&mut s)[0], 0.1), "the darkest component");
    execute(&mut s, "effect.set", &json!({"effect": effect.0, "channelInput": "customInput", "mode": "max"})).unwrap();
    assert!(close(base_color(&mut s)[0], 0.5), "the Custom Input reads mid gray until set");
    let Filter::GrayscaleConversion(conversion) = filter_of(&s, effect) else { panic!("converted") };
    assert!(conversion.custom_input && conversion.mode == ConversionMode::Max);
    assert!(execute(&mut s, "effect.set", &json!({"effect": effect.0, "channelInput": "image"})).is_err());
    assert!(execute(&mut s, "effect.set", &json!({"effect": effect.0, "mode": "luma"})).is_err(), "Luma is the gradients' mode");
    let inspect = execute(&mut s, "project.inspect", &json!({})).unwrap().to_string();
    assert!(inspect.contains("Grayscale Conversion"));
    assert_eq!(serde_json::to_value(Filter::Grayscale).unwrap(), json!("Grayscale"), "saved as before");
}
