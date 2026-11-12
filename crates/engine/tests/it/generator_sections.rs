//! The section controls through `effect.add.generator` and `effect.set`, beside the earlier level, softness and noise

use serde_json::{Value, json};
use texturecraft_engine::core::{AxisBlend, EffectKind, Generator, GeneratorKind, LayerId, SectionParameters};
use texturecraft_engine::mask_cmds::section_key_applies;
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
fn new_generators_start_on_the_reference_controls() {
    let mut session = session();
    for kind in ["curvature", "ambientOcclusion", "position", "worldSpaceNormals"] {
        let effect = add(&mut session, json!({"generator": kind}));
        let added = generator(&session, effect);
        assert_eq!(added.section, SectionParameters::defaults(added.kind), "{kind}");
    }
    assert_eq!(GeneratorKind::parse("World Space Normals"), Some(GeneratorKind::WorldSpaceNormals));
}

#[test]
fn a_section_control_moves_an_earlier_generator_onto_the_defaults_with_that_change() {
    let mut session = session();
    let effect = add(&mut session, json!({"generator": "position", "level": 0.8}));
    assert_eq!(generator(&session, effect).section, None, "level keeps it on the earlier model");

    let edit = json!({"effect": effect, "sectionBalance": 0.3, "rightLeftBlend": "Max (Lighten)", "frontBackWeight": 4.0, "sectionBrightness": -3.0});
    execute(&mut session, "effect.set", &edit).unwrap();

    let moved = generator(&session, effect);
    let mut expected = SectionParameters::defaults(GeneratorKind::Position).unwrap();
    expected.balance = 0.3;
    expected.brightness = -1.0;
    expected.axes[SectionParameters::RIGHT_TO_LEFT].blend = AxisBlend::Max;
    expected.axes[SectionParameters::FRONT_TO_BACK].weight = 1.0;
    assert_eq!(moved.section, Some(expected), "the rest at defaults, values clamped");
    assert_eq!(moved.level, Generator::new(GeneratorKind::Position).level, "the earlier model's values start over");

    execute(&mut session, "effect.set", &json!({"effect": effect, "noise": 0.5})).unwrap();
    assert_eq!(generator(&session, effect).section, None, "noise moves it back");
}

#[test]
fn keys_a_kind_lacks_or_both_models_at_once_are_refused() {
    let mut session = session();
    let curvature = add(&mut session, json!({"generator": "curvature"}));
    let occlusion = add(&mut session, json!({"generator": "ambientOcclusion"}));
    let position = add(&mut session, json!({"generator": "position"}));
    let normals = add(&mut session, json!({"generator": "worldSpaceNormals"}));
    let dirt = add(&mut session, json!({"generator": "dirt"}));

    for (effect, key) in [
        (curvature, "sectionBalance"),
        (curvature, "sectionBlur"),
        (curvature, "topBottomWeight"),
        (occlusion, "sectionBrightness"),
        (occlusion, "rightLeftInvert"),
        (position, "sectionBlur"),
        (dirt, "sectionContrast"),
    ] {
        assert!(execute(&mut session, "effect.set", &json!({"effect": effect, key: 0.5})).is_err(), "{key} on {effect}");
    }
    for (effect, key) in [(curvature, "sectionBrightness"), (occlusion, "sectionBlur"), (normals, "sectionBlur"), (normals, "frontBackWeight")] {
        execute(&mut session, "effect.set", &json!({"effect": effect, key: 0.5})).unwrap();
    }

    assert!(execute(&mut session, "effect.set", &json!({"effect": position, "level": 0.5, "sectionContrast": 0.5})).is_err());
    assert!(execute(&mut session, "effect.set", &json!({"effect": normals, "softness": 0.5})).is_err(), "World Space Normals has no earlier model");
    assert!(execute(&mut session, "effect.set", &json!({"effect": position, "topBottomBlend": "overlay"})).is_err());
    assert_eq!(generator(&session, position).section, SectionParameters::defaults(GeneratorKind::Position), "a refused edit changes nothing");

    assert!(!section_key_applies(GeneratorKind::Position, "sectionBlur"));
    assert!(section_key_applies(GeneratorKind::WorldSpaceNormals, "rightLeftBlend"));
}
