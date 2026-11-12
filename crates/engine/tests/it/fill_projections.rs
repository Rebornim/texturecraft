//! The 3D projection settings and the projection kinds of a fill, through engine commands

use serde_json::{Value, json};
use texturecraft_engine::core::{Channel, LayerId, LayerKind, Projection, Projector};
use texturecraft_engine::geom::primitives;
use texturecraft_engine::{Session, execute};

fn session() -> Session {
    let mut s = Session::new_project("t", primitives::sphere(1.0, 48, 24), "builtin:sphere", None, 128).unwrap();
    s.update();
    s
}

/// A fill layer with a checker procedural in base color
fn checker_fill(s: &mut Session) -> LayerId {
    let layer = LayerId(execute(s, "layer.add.fill", &Value::Null).unwrap()["layer"].as_u64().unwrap());
    execute(s, "layer.fill.set", &json!({"layer": layer.0, "channel": "baseColor", "procedural": "Checker", "scale": 4, "low": [0, 0, 0], "high": [1, 1, 1]}))
        .unwrap();
    layer
}

fn set(s: &mut Session, layer: LayerId, mut p: Value) {
    p["layer"] = json!(layer.0);
    execute(s, "layer.fill.projection", &p).unwrap();
}

fn projection(s: &Session, layer: LayerId) -> Projection {
    match &s.doc.project.layer(layer).unwrap().kind {
        LayerKind::Fill(f) => f.projection.clone(),
        _ => panic!("a fill layer"),
    }
}

/// Base color red of every texel
fn snapshot(s: &mut Session) -> Vec<f32> {
    s.update();
    let cache = &s.caches[0];
    let (w, h) = cache.size;
    (0..w * h).map(|i| cache.composite.value(Channel::BaseColor, i % w, i / w)[0]).collect()
}

/// Rotation, per-axis scale and offset move every 3D projection; the defaults and older saves change nothing
#[test]
fn three_d_projection_settings_turn_stretch_and_move_every_3d_projection() {
    let mut s = session();
    let layer = checker_fill(&mut s);
    for kind in ["triplanar", "planar", "spherical", "cylindrical"] {
        set(&mut s, layer, json!({"projection": kind}));
        let plain = snapshot(&mut s);

        set(&mut s, layer, json!({"projectorRotation": [30, 45, 60]}));
        assert_ne!(snapshot(&mut s), plain, "{kind}: rotation turns the pattern");
        set(&mut s, layer, json!({"projectorRotation": [0, 0, 0], "projectorScale": [2, 0.5, 1]}));
        assert_ne!(snapshot(&mut s), plain, "{kind}: per-axis scale stretches the pattern");
        set(&mut s, layer, json!({"projectorScale": 1, "offset": [0.3, 0.2, 0.1]}));
        assert_ne!(snapshot(&mut s), plain, "{kind}: offset moves the pattern");
        set(&mut s, layer, json!({"offset": 0}));
        assert_eq!(snapshot(&mut s), plain, "{kind}: the defaults change nothing");

        set(&mut s, layer, json!({"projectorRotation": [10, 20, 30], "projectorScale": [1, 2, 3]}));
        let mut saved = serde_json::to_value(projection(&s, layer)).unwrap();
        let fields = saved.as_object_mut().unwrap().values_mut().next().unwrap().as_object_mut().unwrap();
        fields.remove("projector");
        let loaded: Projection = serde_json::from_value(saved).unwrap();
        let settings = match loaded {
            Projection::Triplanar { projector, .. }
            | Projection::Planar { projector, .. }
            | Projection::Spherical { projector, .. }
            | Projection::Cylindrical { projector, .. } => projector,
            other => panic!("{kind}: loaded as {other:?}"),
        };
        assert_eq!(settings, Projector::default(), "{kind}: an older save has no turn or stretch");
        set(&mut s, layer, json!({"projectorRotation": 0, "projectorScale": 1}));
    }

    let inspect = execute(&mut s, "project.inspect", &Value::Null).unwrap();
    let row = inspect["layers"].as_array().unwrap().iter().find(|l| l["id"] == layer.0).unwrap().clone();
    assert_eq!(row["fill"]["projectionSettings"]["Cylindrical"]["projector"]["scale"], json!([1.0, 1.0, 1.0]));
}

/// Fill (Match per UV Tile) shows the same pattern on both UDIM tiles, where UV runs on across them
#[test]
fn match_per_uv_tile_repeats_the_source_once_per_tile() {
    let mut s = Session::new_project("t", primitives::udim_plane(), "builtin:udim_plane", None, 128).unwrap();
    let layer = LayerId(execute(&mut s, "layer.add.fill", &Value::Null).unwrap()["layer"].as_u64().unwrap());
    execute(
        &mut s,
        "layer.fill.set",
        &json!({"layer": layer.0, "channel": "baseColor", "procedural": "Clouds", "scale": 3, "low": [0, 0, 0], "high": [1, 1, 1]}),
    )
    .unwrap();
    let tiles = |s: &mut Session| {
        s.update();
        let composite = &s.caches[0].composite;
        let tile = |block: u32| (0..128 * 128).map(|i| composite.value(Channel::BaseColor, block * 128 + i % 128, i / 128)[0]).collect::<Vec<f32>>();
        (tile(0), tile(1))
    };
    let (left, right) = tiles(&mut s);
    assert_ne!(left, right, "a UV projection runs on into the next tile");

    set(&mut s, layer, json!({"projection": "uvTile"}));
    assert_eq!(projection(&s, layer), Projection::UvTile);
    let (left, right) = tiles(&mut s);
    let worst = left.iter().zip(&right).map(|(a, b)| (a - b).abs()).fold(0.0, f32::max);
    assert!(worst < 1e-4, "every tile shows the same pattern (worst difference {worst})");
}
