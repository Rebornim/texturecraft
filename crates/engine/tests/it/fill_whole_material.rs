use serde_json::{Value, json};
use texturecraft_engine::core::{Channel, LayerId, LayerKind, Source};
use texturecraft_engine::{Session, execute, geom::primitives};

fn session() -> Session {
    Session::new_project("Whole material", primitives::sphere(1.0, 16, 8), "builtin:sphere", None, 128).unwrap()
}

fn layer(v: Value) -> u64 {
    v["layer"].as_u64().unwrap()
}

fn fill(s: &Session, id: u64) -> texturecraft_engine::core::Fill {
    match &s.doc.project.layer(LayerId(id)).unwrap().kind {
        LayerKind::Fill(fill) => fill.clone(),
        _ => panic!("not a fill"),
    }
}

fn channels(s: &Session, id: u64) -> Vec<Channel> {
    s.doc.project.layer(LayerId(id)).unwrap().channels.keys().copied().collect()
}

#[test]
fn a_fill_takes_one_whole_material_names_it_and_forgets_it_once_a_channel_is_set_alone() {
    let mut s = session();
    let id = layer(execute(&mut s, "layer.add.fill", &json!({"color": [1.0, 0.0, 0.0]})).unwrap());
    execute(&mut s, "layer.fill.set", &json!({"layer": id, "channel": "opacity", "value": 0.5})).unwrap();
    let projection = fill(&s, id).projection;
    let saved: Value = serde_json::to_value(fill(&s, id)).unwrap();
    assert!(saved.get("material").is_none(), "a fill without a whole material saves as before");

    execute(&mut s, "layer.fill.material", &json!({"layer": id, "material": "copper_hammered"})).unwrap();
    let copper = fill(&s, id);
    assert_eq!(copper.material.as_deref(), Some("copper_hammered"));
    assert!(matches!(copper.source(Channel::BaseColor), Source::Procedural { .. }), "the material's base colour");
    assert_eq!(copper.source(Channel::Metallic), Source::Uniform([1.0; 3]));
    assert_eq!(channels(&s, id), [Channel::BaseColor, Channel::Metallic, Channel::Roughness, Channel::Height], "the material's channels on, opacity off");
    assert_eq!(copper.projection, projection, "the fill keeps its projection");
    assert_eq!(execute(&mut s, "project.inspect", &json!({})).unwrap().to_string().matches("\"material\":\"copper_hammered\"").count(), 1);

    let reopened = Session::from_project_bytes(&s.to_project_bytes().unwrap()).unwrap();
    assert_eq!(fill(&reopened, id).material.as_deref(), Some("copper_hammered"), "saved with the project");

    execute(&mut s, "edit.undo", &json!({})).unwrap();
    assert_eq!(fill(&s, id).material, None, "one undo step");
    assert_eq!(fill(&s, id).source(Channel::BaseColor), Source::Uniform([1.0, 0.0, 0.0]));
    execute(&mut s, "edit.redo", &json!({})).unwrap();

    execute(&mut s, "layer.fill.set", &json!({"layer": id, "channel": "metallic", "enabled": false})).unwrap();
    assert!(fill(&s, id).material.is_some(), "turning a channel off keeps the material");
    execute(&mut s, "layer.fill.set", &json!({"layer": id, "channel": "roughness", "value": 0.3})).unwrap();
    assert_eq!(fill(&s, id).material, None, "a channel set on its own is no longer the whole material");

    execute(&mut s, "layer.fill.material", &json!({"layer": id, "material": "Steel"})).unwrap();
    execute(&mut s, "layer.fill.material", &json!({"layer": id, "material": null})).unwrap();
    let cleared = fill(&s, id);
    assert_eq!((cleared.material, cleared.sources.len()), (None, 0), "null: back to the channels' defaults");

    // A content fill effect takes the material too
    let effect = execute(&mut s, "effect.add.fill", &json!({"target": "content", "roughness": 0.7})).unwrap()["effect"].as_u64().unwrap();
    execute(&mut s, "layer.fill.material", &json!({"effect": effect, "material": "gold_polished"})).unwrap();
    let inspect = execute(&mut s, "project.inspect", &json!({})).unwrap().to_string();
    assert!(inspect.contains("\"material\":\"gold_polished\""));

    // The shelf names a plain material's fill; a smart one is no whole material
    let shelf = layer(execute(&mut s, "material.apply", &json!({"material": "brass"})).unwrap());
    assert_eq!(fill(&s, shelf).material.as_deref(), Some("brass"));
    assert!(execute(&mut s, "layer.fill.material", &json!({"layer": id, "material": "steel_painted_worn"})).is_err(), "smart");
    assert!(execute(&mut s, "layer.fill.material", &json!({"layer": id, "material": "nope"})).is_err());
    assert!(execute(&mut s, "layer.fill.material", &json!({"layer": id})).is_err(), "material is required");
    assert!(execute(&mut s, "layer.fill.material", &json!({"layer": id, "material": 7})).is_err());
}
