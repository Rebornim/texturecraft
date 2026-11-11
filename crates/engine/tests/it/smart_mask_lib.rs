//! User smart masks: a layer's mask saved to a file and applied to a layer of another project

use serde_json::json;
use texturecraft_engine::core::{EffectKind, LayerId, Projection, Source};
use texturecraft_engine::geom::primitives;
use texturecraft_engine::{Session, execute};

fn session() -> Session {
    let mut session = Session::new_project("t", primitives::material_ball(), "builtin:material_ball", None, 256).unwrap();
    session.viewport = texturecraft_engine::geom::glam::Vec2::new(400.0, 400.0);
    session
}

fn fill_layer(session: &mut Session) -> LayerId {
    LayerId(execute(session, "layer.add.fill", &json!({"color": [0.8, 0.2, 0.1]})).unwrap()["layer"].as_u64().unwrap())
}

fn mask_view(session: &mut Session, layer: LayerId) -> Vec<u8> {
    session.update();
    session.mask_view_rgba(layer, 128).unwrap()
}

/// A procedural mask with painted pixels on top reaches another project's layer, replacing its
/// mask, and gives the same mask there.
#[test]
fn a_saved_smart_mask_applies_identically_in_another_project() {
    let dir = std::env::temp_dir().join(format!("tc-smart-mask-{}", std::process::id()));
    let path = dir.join("worn edges.tcmask");
    let mut source = session();
    let layer = fill_layer(&mut source);
    texturecraft_engine::smart_masks::apply(&mut source, layer, "edge_wear").unwrap();
    let painted = LayerId(execute(&mut source, "effect.add.paint", &json!({"layer": layer.0, "target": "mask"})).unwrap()["effect"].as_u64().unwrap());
    let image = source.doc.pixels.mask_mut(painted, (256, 256));
    for y in 40..120 {
        for x in 30..200 {
            image.set(x, y, &[65535, 65535]);
        }
    }
    let expected = mask_view(&mut source, layer);
    execute(&mut source, "mask.smart.save", &json!({"layer": layer.0, "path": path.to_str().unwrap()})).unwrap();

    let mut target = session();
    fill_layer(&mut target);
    let layer = fill_layer(&mut target);
    execute(&mut target, "mask.add.white", &json!({"layer": layer.0})).unwrap();
    execute(&mut target, "mask.smart.load", &json!({"layer": layer.0, "path": path.to_str().unwrap()})).unwrap();
    assert_eq!(mask_view(&mut target, layer), expected);
    target.doc.project.validate().unwrap();

    execute(&mut target, "edit.undo", &json!({})).unwrap();
    assert!(target.doc.project.layer(layer).unwrap().mask.as_ref().unwrap().effects.is_empty(), "undo restores the white mask");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn smart_mask_save_refuses_a_layer_without_a_mask_and_load_refuses_junk() {
    let mut session = session();
    let layer = fill_layer(&mut session);
    assert!(session.smart_mask_bytes(layer, "none").is_err());
    assert!(session.apply_smart_mask_bytes(layer, b"junk").is_err());
}

#[test]
fn smart_mask_save_refuses_a_mask_that_reads_an_anchor() {
    let mut session = session();
    let layer = fill_layer(&mut session);
    execute(&mut session, "mask.add.black", &json!({"layer": layer.0})).unwrap();
    let mask = session.doc.project.layer_mut(layer).unwrap().mask.as_mut().unwrap();
    let id = LayerId(9_999);
    mask.effects.push(texturecraft_engine::core::Effect::new(
        id,
        EffectKind::Fill {
            source: Source::Anchor { anchor: LayerId(1), channel: texturecraft_engine::core::Channel::Height },
            projection: Projection::default(),
        },
    ));
    let error = session.smart_mask_bytes(layer, "anchored").unwrap_err().to_string();
    assert!(error.contains("anchor"), "{error}");
}
