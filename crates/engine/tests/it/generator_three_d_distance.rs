//! The generators with only their own controls (3D Distance, 3D Linear Gradient, UV Border Distance, Grayscale Conversion, Dripping Rust) through `effect.add.generator` and `effect.set`: their controls and the controls they lack

use serde_json::{Value, json};
use texturecraft_engine::core::{
    BorderDistanceParameters, DistanceParameters, DrippingRustParameters, EffectKind, Generator, GeneratorKind, GrayscaleConversion, KindControls, LayerId,
    LinearGradientParameters, Source, UvRandomColorParameters,
};
use texturecraft_engine::{Session, execute};

fn session() -> Session {
    let mut session = texturecraft_engine::sample_session(128, 64).unwrap();
    execute(&mut session, "layer.add.fill", &Value::Null).unwrap();
    execute(&mut session, "mask.add.black", &Value::Null).unwrap();
    session
}

fn add(session: &mut Session, params: Value) -> u64 {
    execute(session, "effect.add.generator", &params).unwrap()["effect"].as_u64().unwrap()
}

fn generator(session: &Session, effect: u64) -> Generator {
    match &session.doc.project.effect(LayerId(effect)).unwrap().1.kind {
        EffectKind::Generator(generator) => generator.clone(),
        other => panic!("not a generator: {other:?}"),
    }
}

#[test]
fn three_d_distance_takes_its_controls_by_name_and_label() {
    let mut session = session();
    for name in ["3dDistance", "3D Distance", "ThreeDDistance"] {
        let effect = add(&mut session, json!({"generator": name, "distanceX": 0.2, "distanceRadius": 3.0, "distanceOffset": 0.7, "invert": true}));

        let made = generator(&session, effect);
        assert_eq!(made.kind, GeneratorKind::ThreeDDistance, "{name}");
        let expected = DistanceParameters { point: [0.2, 0.5, 0.5], radius: 1.5, offset: 0.7, ..DistanceParameters::default() };
        assert_eq!(made.distance_controls(), Some(expected), "{name}: the radius stops at 1.5");
        assert!(made.invert && !made.on_earlier_model());
    }
}

#[test]
fn three_d_distance_refuses_the_earlier_and_global_controls_and_others_refuse_its_own() {
    let mut session = session();
    let effect = add(&mut session, json!({"generator": "3dDistance"}));
    for key in ["level", "softness", "noise", "offset", "blur", "balance", "contrast"] {
        assert!(execute(&mut session, "effect.set", &json!({"effect": effect, key: 0.3})).is_err(), "{key}");
    }

    let curvature = add(&mut session, json!({"generator": "curvature"}));
    assert!(execute(&mut session, "effect.set", &json!({"effect": curvature, "distanceRadius": 0.5})).is_err());
    assert_eq!(generator(&session, curvature).controls, None);
}

#[test]
fn swapping_onto_three_d_distance_drops_the_earlier_offset() {
    let mut session = session();
    let effect = add(&mut session, json!({"generator": "curvature", "level": 0.5, "offset": 0.4}));

    execute(&mut session, "effect.set", &json!({"effect": effect, "generator": "3dDistance"})).unwrap();
    let swapped = generator(&session, effect);
    assert_eq!(swapped.offset, 0.0);
    assert_eq!(swapped.distance_controls(), Some(DistanceParameters::default()));
}

#[test]
fn three_d_linear_gradient_takes_its_points_balance_and_contrast() {
    let mut session = session();
    let params =
        json!({"generator": "3D Linear Gradient", "gradientStart": [0.0, 0.5, 0.5], "gradientEnd": [1.0, 0.5, 2.0], "gradientBalance": 0.25, "invert": true});
    let effect = add(&mut session, params);

    let made = generator(&session, effect);
    assert_eq!(made.kind, GeneratorKind::ThreeDLinearGradient);
    let expected = LinearGradientParameters { start: [0.0, 0.5, 0.5], end: [1.0, 0.5, 1.0], balance: 0.25, contrast: 0.0 };
    assert_eq!(made.linear_gradient_controls(), Some(expected), "points stay within the bounds");
    assert!(made.invert && !made.on_earlier_model());
}

#[test]
fn each_3d_kind_refuses_the_others_controls() {
    let mut session = session();
    let gradient = add(&mut session, json!({"generator": "3dLinearGradient"}));
    let distance = add(&mut session, json!({"generator": "3dDistance"}));

    assert!(execute(&mut session, "effect.set", &json!({"effect": gradient, "distanceRadius": 0.5})).is_err());
    assert!(execute(&mut session, "effect.set", &json!({"effect": distance, "gradientBalance": 0.5})).is_err());
    assert!(execute(&mut session, "effect.set", &json!({"effect": gradient, "level": 0.5})).is_err());
}

#[test]
fn uv_border_distance_takes_its_controls_and_refuses_the_others() {
    let mut session = session();
    let effect = add(&mut session, json!({"generator": "UV Border Distance", "borderDistance": 0.3, "borderSmoothness": 2.0, "borderContrast": 0.4}));

    let expected = BorderDistanceParameters { distance: 0.3, smoothness: 1.0, contrast: 0.4, ..BorderDistanceParameters::default() };
    assert_eq!(generator(&session, effect).border_distance_controls(), Some(expected));
    assert!(execute(&mut session, "effect.set", &json!({"effect": effect, "distanceRadius": 0.5})).is_err());
    assert!(execute(&mut session, "effect.set", &json!({"effect": effect, "softness": 0.5})).is_err());
    let curvature = add(&mut session, json!({"generator": "curvature"}));
    assert!(execute(&mut session, "effect.set", &json!({"effect": curvature, "borderDistance": 0.5})).is_err());
}

#[test]
fn grayscale_conversion_takes_its_controls_and_its_image() {
    let mut session = session();
    let image = image::RgbaImage::from_pixel(4, 4, image::Rgba([200, 50, 50, 255]));
    let mut bytes = Vec::new();
    image.write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png).unwrap();
    let name = session.import_resource_bytes("color.png", bytes).unwrap();
    let effect = add(&mut session, json!({"generator": "Grayscale Conversion", "grayscaleType": "red", "grayscaleTile": 3.4, "grayscaleSafeRotation": false}));

    execute(&mut session, "effect.set", &json!({"effect": effect, "input": "image", "inputImage": name})).unwrap();
    let grayscale = generator(&session, effect).grayscale_controls().unwrap();
    assert_eq!((grayscale.grayscale, grayscale.tile, grayscale.safe_rotation), (GrayscaleConversion::Red, 3.0, false), "the tile is a whole count");
    assert_eq!(grayscale.source, Some(Source::image(name.clone())));
    execute(&mut session, "file.removeUnused", &Value::Null).unwrap();
    assert!(session.doc.resources.contains_key(&name), "the image is still read");

    let curvature = add(&mut session, json!({"generator": "curvature"}));
    assert!(execute(&mut session, "effect.set", &json!({"effect": curvature, "input": "image", "inputImage": name})).is_err());
    assert!(execute(&mut session, "effect.set", &json!({"effect": curvature, "grayscaleTile": 2})).is_err());
}

#[test]
fn dripping_rust_takes_its_controls_its_seed_and_its_axis() {
    let mut session = session();
    let params = json!({"generator": "Dripping Rust", "rustSpreading": 0.7, "dripsIntensity": 0.4, "dripsSamples": 99, "dripsAxis": "X", "seed": 3});
    let effect = add(&mut session, params);

    let made = generator(&session, effect);
    let expected = DrippingRustParameters { spreading: 0.7, drips_intensity: 0.4, drips_samples: 32, axis: 0, ..DrippingRustParameters::default() };
    assert_eq!(made.kind_controls(), Some(KindControls::DrippingRust(expected)), "samples stop at 32");
    assert_eq!(made.seed, 3);
    assert!(execute(&mut session, "effect.set", &json!({"effect": effect, "dripsAxis": "w"})).is_err());
    assert!(execute(&mut session, "effect.set", &json!({"effect": effect, "level": 0.2})).is_err());
    let curvature = add(&mut session, json!({"generator": "curvature"}));
    assert!(execute(&mut session, "effect.set", &json!({"effect": curvature, "rustSpreading": 0.2})).is_err());
}

#[test]
fn uv_random_color_takes_its_mode_and_gradient() {
    let mut session = session();
    let params = json!({"generator": "UV Random Color", "colorSourceMode": "customGradient", "gradientColors": [[2.0, 0.5, 0.0], "#000000"]});
    let effect = add(&mut session, params);

    let expected = UvRandomColorParameters { custom_gradient: true, gradient: vec![[1.0, 0.5, 0.0], [0.0; 3]] };
    assert_eq!(generator(&session, effect).kind_controls(), Some(KindControls::UvRandomColor(expected)), "colors stay within 0..1");
    assert!(execute(&mut session, "effect.set", &json!({"effect": effect, "gradientColors": []})).is_err());
    assert!(execute(&mut session, "effect.set", &json!({"effect": effect, "colorSourceMode": "fancy"})).is_err());
}
