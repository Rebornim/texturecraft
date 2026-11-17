//! The 3D Distance generator through `effect.add.generator` and `effect.set`: its controls, the controls it lacks, and a gradient around its point

use serde_json::{Value, json};
use texturecraft_engine::core::{DistanceParameters, EffectKind, Generator, GeneratorKind, LayerId};
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
    assert_eq!(generator(&session, curvature).distance, None);
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
