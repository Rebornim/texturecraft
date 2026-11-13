//! A library alpha as a fill channel's source through `layer.fill.set {alpha}`: its picture becomes one image resource the channel reads

use serde_json::{Value, json};
use texturecraft_engine::core::{Channel, LayerId, LayerKind, Source};
use texturecraft_engine::fill_alpha::{library_alpha_of, resource_name};
use texturecraft_engine::geom::primitives;
use texturecraft_engine::paint::alphas;
use texturecraft_engine::{Session, execute};

fn fill() -> (Session, LayerId) {
    let mut s = Session::new_project("t", primitives::sphere(1.0, 16, 8), "builtin:sphere", None, 128).unwrap();
    let layer = LayerId(execute(&mut s, "layer.add.fill", &Value::Null).unwrap()["layer"].as_u64().unwrap());
    (s, layer)
}

fn source(s: &Session, layer: LayerId, channel: Channel) -> Source {
    match &s.doc.project.layer(layer).unwrap().kind {
        LayerKind::Fill(fill) => fill.source(channel),
        other => panic!("not a fill: {other:?}"),
    }
}

#[test]
fn an_alpha_becomes_the_channels_image_once_and_one_undo_takes_it_back() {
    let (mut s, layer) = fill();
    let scratches = alphas::find("scratches").unwrap();
    let name = resource_name(scratches);
    execute(&mut s, "layer.fill.set", &json!({"layer": layer.0, "channel": "roughness", "alpha": "Scratches"})).unwrap();

    assert_eq!(source(&s, layer, Channel::Roughness), Source::image(name.clone()));
    assert!(s.doc.resources.contains_key(&name), "the alpha's picture is a project resource, so the project saves it");
    let image = s.images.get(&name).expect("the channel's image is decoded");
    assert_eq!((image.width, image.height), (alphas::BASE as u32, alphas::BASE as u32));
    assert_eq!(library_alpha_of(&name).map(|alpha| alpha.id.as_str()), Some("scratches"), "the slot can name the alpha it came from");
    assert!(library_alpha_of("scratches.png").is_none(), "an imported image is not a library alpha");

    // The same alpha on a second channel reads the same resource
    let count = s.doc.resources.len();
    execute(&mut s, "layer.fill.set", &json!({"layer": layer.0, "channel": "metallic", "alpha": "scratches"})).unwrap();
    assert_eq!(source(&s, layer, Channel::Metallic), Source::image(name.clone()));
    assert_eq!(s.doc.resources.len(), count);

    execute(&mut s, "edit.undo", &Value::Null).unwrap();
    execute(&mut s, "edit.undo", &Value::Null).unwrap();
    assert_eq!(source(&s, layer, Channel::Roughness), Source::uniform_of(Channel::Roughness));
    assert!(!s.doc.resources.contains_key(&name), "undo takes the resource back with the source");
}

#[test]
fn an_unknown_alpha_changes_nothing() {
    let (mut s, layer) = fill();
    let before = s.doc.resources.len();
    assert!(execute(&mut s, "layer.fill.set", &json!({"layer": layer.0, "channel": "roughness", "alpha": "no such alpha"})).is_err());
    assert_eq!(source(&s, layer, Channel::Roughness), Source::uniform_of(Channel::Roughness));
    assert_eq!(s.doc.resources.len(), before);
}
