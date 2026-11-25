//! Flattening a folder: a new fill layer above it reads one image per channel and its mask is baked, so the stack looks the same with the folder hidden

use serde_json::{Value, json};
use texturecraft_engine::core::{LayerId, LayerKind};
use texturecraft_engine::geom::glam::Vec2;
use texturecraft_engine::geom::primitives;
use texturecraft_engine::{Session, StackTarget, execute};

/// The test project's texture size
const RESOLUTION: u32 = 256;

/// Largest difference a flattened texel may show (16-bit images, straight values recovered from premultiplied ones)
const TOLERANCE: f32 = 4e-3;

fn session() -> Session {
    let mut s = Session::new_project("t", primitives::sphere(1.0, 48, 24), "builtin:sphere", None, RESOLUTION).unwrap();
    s.viewport = Vec2::new(400.0, 400.0);
    s.camera.yaw = 0.0;
    s.camera.pitch = 0.0;
    s.update();
    s
}

fn id(reply: Value) -> u64 {
    reply["layer"].as_u64().unwrap()
}

fn stroke(s: &mut Session, size: u32, points: [[u32; 2]; 2]) {
    execute(s, "tool.brush", &json!({"size": size, "hardness": 40})).unwrap();
    execute(s, "tool.stroke", &json!({ "points": points })).unwrap();
}

/// Every channel's composited value at every texel
fn composite(s: &mut Session) -> Vec<[f32; 3]> {
    s.update();
    let channels = s.doc.project.texture_set(0).unwrap().channel_list();
    let cache = &s.caches[0];
    channels.iter().flat_map(|c| (0..RESOLUTION).flat_map(move |y| (0..RESOLUTION).map(move |x| cache.composite.value(*c, x, y)))).collect()
}

fn largest_difference(a: &[[f32; 3]], b: &[[f32; 3]]) -> f32 {
    a.iter().zip(b).flat_map(|(a, b)| a.iter().zip(b).map(|(a, b)| (a - b).abs())).fold(0.0, f32::max)
}

#[test]
fn a_flattened_folder_looks_the_same_as_the_folder_and_undoes_in_one_step() {
    let mut s = session();
    id(execute(&mut s, "layer.add.fill", &json!({"color": [0.0, 0.0, 1.0], "roughness": 0.2, "height": -0.3})).unwrap());
    let below = composite(&mut s);

    // A folder over it: a masked red fill and a soft roughness stroke, with a stroke in the folder's own mask
    let folder = id(execute(&mut s, "layer.add.group", &Value::Null).unwrap());
    id(execute(&mut s, "layer.add.fill", &json!({"color": [1.0, 0.2, 0.0], "roughness": 0.9, "height": 0.4})).unwrap());
    s.target = StackTarget::Mask;
    stroke(&mut s, 30, [[170, 200], [230, 200]]);
    id(execute(&mut s, "layer.add.paint", &Value::Null).unwrap());
    execute(&mut s, "tool.material", &json!({"channel": "roughness", "value": 0.6})).unwrap();
    stroke(&mut s, 20, [[200, 160], [200, 240]]);
    execute(&mut s, "layer.select", &json!({"layer": folder})).unwrap();
    s.target = StackTarget::Mask;
    stroke(&mut s, 40, [[160, 215], [240, 215]]);
    let before = composite(&mut s);

    assert!(largest_difference(&before, &below) > 0.1, "the folder shows over the layer below");

    let flat = id(execute(&mut s, "layer.flatten", &json!({"layer": folder})).unwrap());
    let after = composite(&mut s);

    assert!(matches!(s.doc.project.layer(LayerId(flat)).unwrap().kind, LayerKind::Fill(_)), "the folder becomes a fill layer");
    assert!(!s.doc.project.layer(LayerId(folder)).unwrap().visible, "the folder is hidden under it");
    assert!(s.doc.project.layer(LayerId(flat)).unwrap().mask.is_some(), "the folder's mask is baked into the new layer");
    assert!(largest_difference(&after, &before) < TOLERANCE, "{}", largest_difference(&after, &before));

    let mut reopened = Session::from_project_bytes(&s.to_project_bytes().unwrap()).unwrap();

    assert!(largest_difference(&composite(&mut reopened), &before) < TOLERANCE, "the baked images keep their coverage when reopened");

    execute(&mut s, "edit.undo", &Value::Null).unwrap();

    assert!(s.doc.project.layer(LayerId(flat)).is_err(), "one undo step removes the flattened layer");
    assert!(s.doc.project.layer(LayerId(folder)).unwrap().visible, "and shows the folder again");
}

#[test]
fn a_flattened_folder_bakes_a_mask_that_reads_the_layers_below() {
    let mut s = session();
    id(execute(&mut s, "layer.add.fill", &json!({"color": [0.0, 0.0, 1.0], "height": -0.3})).unwrap());
    id(execute(&mut s, "layer.add.paint", &Value::Null).unwrap());
    execute(&mut s, "tool.material", &json!({"channel": "height", "value": 0.6})).unwrap();
    stroke(&mut s, 40, [[160, 200], [240, 200]]);
    let folder = id(execute(&mut s, "layer.add.group", &Value::Null).unwrap());
    id(execute(&mut s, "layer.add.fill", &json!({"color": [1.0, 0.2, 0.0], "height": 0.2})).unwrap());
    execute(&mut s, "mask.add.heightCombination", &json!({"layer": folder})).unwrap();
    let before = composite(&mut s);

    id(execute(&mut s, "layer.flatten", &json!({"layer": folder})).unwrap());

    assert!(largest_difference(&composite(&mut s), &before) < TOLERANCE, "{}", largest_difference(&composite(&mut s), &before));
}
