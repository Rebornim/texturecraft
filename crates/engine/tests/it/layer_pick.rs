//! Picking a layer on the mesh (R + click): a pick selects the top layer laying something down there, and clicks on the same spot walk down the stack

use serde_json::{Value, json};
use texturecraft_engine::core::LayerKind;
use texturecraft_engine::geom::glam::Vec2;
use texturecraft_engine::geom::primitives;
use texturecraft_engine::{Session, execute};

/// The middle of the sphere in the test viewport, where the lower paint layer is painted
const MIDDLE: [f32; 2] = [200.0, 200.0];

/// A spot on the sphere left of the middle, where only the upper paint layer is painted
const LEFT: [f32; 2] = [130.0, 200.0];

/// The test stack: (session, the fill at the bottom, a paint layer over the middle, a paint layer over the left spot above it)
fn stack() -> (Session, u64, u64, u64) {
    let mut s = Session::new_project("t", primitives::sphere(1.0, 48, 24), "builtin:sphere", None, 256).unwrap();
    s.viewport = Vec2::new(400.0, 400.0);
    s.camera.yaw = 0.0;
    s.camera.pitch = 0.0;
    let fill = id(execute(&mut s, "layer.add.fill", &json!({"color": [0.0, 0.0, 1.0]})).unwrap());
    let middle = id(execute(&mut s, "layer.add.paint", &Value::Null).unwrap());
    execute(&mut s, "tool.brush", &json!({"size": 20, "hardness": 100})).unwrap();
    execute(&mut s, "tool.stroke", &json!({"points": [[190, 200], [210, 200]]})).unwrap();
    let left = id(execute(&mut s, "layer.add.paint", &Value::Null).unwrap());
    execute(&mut s, "tool.stroke", &json!({"points": [[125, 200], [135, 200]]})).unwrap();
    s.update();
    (s, fill, middle, left)
}

fn id(reply: Value) -> u64 {
    reply["layer"].as_u64().unwrap()
}

fn click(s: &mut Session, [x, y]: [f32; 2]) -> Option<u64> {
    execute(s, "layer.pickAt", &json!({"x": x, "y": y})).ok().map(id)
}

#[test]
fn clicks_on_one_spot_walk_down_the_layers_laying_something_there_and_wrap() {
    let (mut s, fill, middle, left) = stack();

    assert_eq!(click(&mut s, MIDDLE), Some(middle), "the top layer painted there, not the one painted elsewhere");
    assert_eq!(s.selected_layer_id().map(|layer| layer.0), Some(middle));
    assert_eq!(click(&mut s, MIDDLE), Some(fill), "the same spot again: the next layer down");
    assert_eq!(click(&mut s, MIDDLE), Some(middle), "past the bottom: back to the top");
    assert_eq!(click(&mut s, LEFT), Some(left), "another spot starts at its top layer");
}

#[test]
fn hidden_layers_and_channels_out_of_view_lay_nothing_down() {
    let (mut s, fill, middle, _) = stack();
    execute(&mut s, "layer.blend", &json!({"layer": middle, "channel": "baseColor", "opacity": 0.0})).unwrap();
    execute(&mut s, "viewport.view", &json!({"mode": "baseColor"})).unwrap();
    assert_eq!(click(&mut s, MIDDLE), Some(fill), "in the base colour view the layer at zero base colour opacity shows nothing");

    execute(&mut s, "viewport.view", &json!({"mode": "material"})).unwrap();
    assert_eq!(click(&mut s, MIDDLE), Some(middle), "the material view counts its other channels");

    execute(&mut s, "layer.visible", &json!({"layer": middle, "visible": false})).unwrap();
    assert_eq!(click(&mut s, MIDDLE), Some(fill), "a hidden layer is skipped");
}

#[test]
fn a_pick_opens_the_collapsed_folders_holding_the_picked_layer() {
    let (mut s, _, middle, _) = stack();
    execute(&mut s, "layer.group", &json!({"layer": middle})).unwrap();
    let inner = s.selected_layer_id().unwrap();
    execute(&mut s, "layer.group", &json!({"layer": inner.0})).unwrap();
    let outer = s.selected_layer_id().unwrap();
    for folder in [inner, outer] {
        if let Some(LayerKind::Group { collapsed, .. }) = s.doc.project.layers.get_mut(&folder).map(|layer| &mut layer.kind) {
            *collapsed = true;
        }
    }

    assert_eq!(click(&mut s, MIDDLE), Some(middle));

    let open = |folder| matches!(s.doc.project.layers[&folder].kind, LayerKind::Group { collapsed: false, .. });
    assert!(open(inner) && open(outer), "both folders holding the picked layer are open");
}
