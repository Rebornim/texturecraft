//! Generator effects are named after their kind, and a name taken from the kind follows a kind change

use serde_json::{Value, json};
use texturecraft_engine::core::LayerId;
use texturecraft_engine::{Session, execute};

/// A sample session with a new generator of `kind` on its top layer's mask
fn generator(kind: &str) -> (Session, LayerId) {
    let mut session = texturecraft_engine::sample_session(256, 64).unwrap();
    execute(&mut session, "layer.add.fill", &Value::Null).unwrap();
    let added = execute(&mut session, "effect.add.generator", &json!({"generator": kind})).unwrap();

    (session, LayerId(added["effect"].as_u64().unwrap()))
}

fn name(session: &Session, effect: LayerId) -> String {
    session.doc.project.effect(effect).unwrap().1.name.clone()
}

fn set(session: &mut Session, effect: LayerId, params: Value) {
    let mut params = params;
    params["effect"] = json!(effect.0);
    execute(session, "effect.set", &params).unwrap();
}

#[test]
fn a_new_generator_takes_its_kinds_name_and_keeps_following_it() {
    let (mut session, effect) = generator("dirt");
    assert_eq!(name(&session, effect), "Dirt");

    set(&mut session, effect, json!({"generator": "curvature"}));

    assert_eq!(name(&session, effect), "Curvature");
}

#[test]
fn a_chosen_name_survives_a_kind_change() {
    let (mut session, effect) = generator("dirt");
    set(&mut session, effect, json!({"name": "Grime"}));

    set(&mut session, effect, json!({"generator": "curvature"}));
    assert_eq!(name(&session, effect), "Grime");

    set(&mut session, effect, json!({"generator": "dirt", "name": "Dust"}));
    assert_eq!(name(&session, effect), "Dust", "a name given with the change wins");
}

#[test]
fn an_older_projects_generic_generator_name_follows_its_next_kind_change() {
    let (mut session, effect) = generator("dirt");
    set(&mut session, effect, json!({"name": "Generator"}));

    set(&mut session, effect, json!({"generator": "curvature"}));

    assert_eq!(name(&session, effect), "Curvature");
}
