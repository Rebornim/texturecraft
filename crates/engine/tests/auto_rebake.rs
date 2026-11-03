//! Auto-rebake: one mesh map bakes itself again when its bake inputs change.

use std::sync::Arc;

use serde_json::{Value, json};
use texturecraft_engine::core::BakedMap;
use texturecraft_engine::geom::primitives;
use texturecraft_engine::{Session, execute};

/// A plane with a high poly just above it, baked once with height and opacity.
fn baked_session(tag: &str) -> (Session, std::path::PathBuf) {
    let mesh = primitives::builtin("plane").unwrap();
    let mut obj = String::from("o Part_high\n");
    for p in &mesh.positions {
        obj += &format!("v {} {} {}\n", p.x, p.y + 0.002, p.z);
    }
    for t in mesh.indices.chunks_exact(3) {
        obj += &format!("f {} {} {}\n", t[0] + 1, t[1] + 1, t[2] + 1);
    }
    let dir = std::env::temp_dir().join(format!("tc_auto_rebake_{tag}_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("hp.obj");
    std::fs::write(&path, obj).unwrap();
    let mut s = Session::new_project("t", mesh, "builtin:plane", None, 256).unwrap();
    execute(&mut s, "bake.highPoly.add", &json!({ "path": path.display().to_string() })).unwrap();
    execute(&mut s, "bake.settings", &json!({ "resolution": 128, "dilation": 0, "maps": ["height", "opacity"] })).unwrap();
    execute(&mut s, "bake.run", &json!({ "cpu": true })).unwrap();
    (s, dir)
}

fn due(s: &mut Session) -> Vec<u64> {
    execute(s, "bake.autoRebake", &Value::Null).unwrap()["due"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap()).collect()
}

fn image(s: &Session, map: BakedMap) -> Arc<Vec<f32>> {
    s.doc.baked[&0].maps[&map].clone()
}

#[test]
fn a_changed_bake_input_rebakes_only_the_chosen_map() {
    let (mut s, dir) = baked_session("rebake");
    // Off by default; nothing is due and the run command says what to do first.
    assert_eq!(execute(&mut s, "bake.autoRebake", &Value::Null).unwrap()["map"], Value::Null);
    assert!(execute(&mut s, "bake.autoRebake.run", &Value::Null).is_err());
    let r = execute(&mut s, "bake.autoRebake", &json!({ "map": "height" })).unwrap();
    assert_eq!(r["map"], "height");
    assert!(due(&mut s).is_empty(), "turning it on takes the current bake as up to date");

    // Which maps are ticked is not a bake input; a setting is.
    execute(&mut s, "bake.settings", &json!({ "maps": ["height", "opacity", "ambient_occlusion"] })).unwrap();
    assert!(due(&mut s).is_empty());
    execute(&mut s, "bake.settings", &json!({ "maxFrontal": 0.2, "maps": ["height", "opacity"] })).unwrap();
    assert_eq!(due(&mut s), [0]);

    let (height, opacity) = (image(&s, BakedMap::Height), image(&s, BakedMap::Opacity));
    let r = execute(&mut s, "bake.autoRebake.run", &json!({ "cpu": true })).unwrap();
    assert_eq!(r["rebaked"], json!([0]));
    assert!(!Arc::ptr_eq(&height, &image(&s, BakedMap::Height)), "the Auto-rebake map was baked again");
    assert!(Arc::ptr_eq(&opacity, &image(&s, BakedMap::Opacity)), "the other maps were left alone");
    assert!(due(&mut s).is_empty(), "and the set is up to date");
    assert_eq!(execute(&mut s, "bake.autoRebake.run", &Value::Null).unwrap()["rebaked"], json!([]));

    // A manual bake of the map also counts; a changed high poly list is a change again.
    execute(&mut s, "bake.settings", &json!({ "antialiasing": 2 })).unwrap();
    assert_eq!(due(&mut s), [0]);
    s.begin_bake(0);
    let out = s.bake_task_for(0, None).unwrap().run(&texturecraft_engine::bake::Progress::new()).unwrap();
    s.apply_bake(0, out).unwrap();
    assert!(due(&mut s).is_empty(), "a manual bake of the map marks it up to date");
    execute(&mut s, "bake.highPoly.remove", &json!({ "all": true })).unwrap();
    assert_eq!(due(&mut s), [0]);

    // A different output size is left to a manual bake: the maps would end up at two sizes.
    execute(&mut s, "bake.settings", &json!({ "resolution": 256 })).unwrap();
    assert!(due(&mut s).is_empty());

    // Hostile input, and turning it off.
    for bad in [json!({ "map": "nonsense" }), json!({ "map": 3 })] {
        assert!(execute(&mut s, "bake.autoRebake", &bad).is_err(), "{bad}");
    }
    assert_eq!(execute(&mut s, "bake.autoRebake", &json!({ "map": null })).unwrap()["map"], Value::Null);
    assert!(due(&mut s).is_empty());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_cancelled_or_failed_bake_is_not_queued_again_until_the_inputs_change() {
    let (mut s, dir) = baked_session("cancel");
    execute(&mut s, "bake.autoRebake", &json!({ "map": "height" })).unwrap();
    assert!(due(&mut s).is_empty());
    execute(&mut s, "bake.settings", &json!({ "maxFrontal": 0.2 })).unwrap();
    assert_eq!(due(&mut s), [0]);

    // What the app does: start the bake, cancel it, record the abandoned bake.
    s.begin_bake(0);
    let mut task = s.bake_task_for(0, Some(BakedMap::Height)).unwrap();
    task.tracer = None;
    let progress = texturecraft_engine::bake::Progress::new();
    progress.cancel();
    assert!(task.run(&progress).is_err(), "a cancelled bake fails");
    s.bake_abandoned([0]);
    assert!(due(&mut s).is_empty(), "not queued again on the next frame");

    // The next change makes it due again; a bake that never started (queued behind a failed
    // one) is recorded the same way.
    execute(&mut s, "bake.settings", &json!({ "maxFrontal": 0.3 })).unwrap();
    assert_eq!(due(&mut s), [0]);
    s.bake_abandoned([0]);
    assert!(due(&mut s).is_empty());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_set_without_the_map_baked_is_left_alone() {
    let mut s = Session::new_project("t", primitives::builtin("plane").unwrap(), "builtin:plane", None, 128).unwrap();
    execute(&mut s, "bake.autoRebake", &json!({ "map": "height" })).unwrap();
    execute(&mut s, "bake.settings", &json!({ "maxFrontal": 0.3 })).unwrap();
    assert!(due(&mut s).is_empty(), "Auto-rebake keeps a bake up to date, it doesn't start one");
}

#[test]
fn undoing_a_rebake_brings_back_a_stale_map_and_it_is_due_again() {
    let (mut s, dir) = baked_session("undo");
    execute(&mut s, "bake.autoRebake", &json!({ "map": "height" })).unwrap();
    assert!(due(&mut s).is_empty());
    execute(&mut s, "bake.settings", &json!({ "maxFrontal": 0.2 })).unwrap();
    assert_eq!(due(&mut s), [0]);
    execute(&mut s, "bake.autoRebake.run", &json!({ "cpu": true })).unwrap();
    assert!(due(&mut s).is_empty());
    // Undo the bake: the older map is back, baked with the older settings, which no longer match.
    let newer = image(&s, BakedMap::Height);
    execute(&mut s, "edit.undo", &Value::Null).unwrap();
    assert!(!Arc::ptr_eq(&newer, &image(&s, BakedMap::Height)), "undo restored the older map");
    assert_eq!(due(&mut s), [0], "the restored map is out of date and rebakes");
    // Redo brings the newer map back, which does match.
    execute(&mut s, "edit.redo", &Value::Null).unwrap();
    assert!(due(&mut s).is_empty(), "the newer map matches the current settings");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn painting_or_clearing_the_skew_map_is_a_change() {
    let (mut s, dir) = baked_session("skew");
    execute(&mut s, "bake.autoRebake", &json!({ "map": "height" })).unwrap();
    assert!(due(&mut s).is_empty());
    execute(&mut s, "bake.skew.set", &json!({ "value": 0.5 })).unwrap();
    assert_eq!(due(&mut s), [0], "a skew map is a bake input");
    execute(&mut s, "bake.autoRebake.run", &json!({ "cpu": true })).unwrap();
    assert!(due(&mut s).is_empty());
    execute(&mut s, "bake.skew.paint", &json!({ "points": [[0.5, 0.5]], "size": 32 })).unwrap();
    assert_eq!(due(&mut s), [0], "so is a stroke painted into it");
    execute(&mut s, "bake.autoRebake.run", &json!({ "cpu": true })).unwrap();
    execute(&mut s, "bake.skew.clear", &Value::Null).unwrap();
    assert_eq!(due(&mut s), [0], "and so is taking it away");
    let _ = std::fs::remove_dir_all(dir);
}

/// Edit > Change mesh swaps the mesh under a baked project: the high poly and Auto-rebake stay (the
/// new mesh makes the map out of date), the project counts as changed, and image resources are decoded.
#[test]
fn changing_the_mesh_keeps_the_bake_setup_and_marks_the_project_changed() {
    let (mut s, dir) = baked_session("change");
    let mut obj = String::new();
    let plane = primitives::builtin("plane").unwrap();
    for p in &plane.positions {
        obj += &format!("v {} {} {}\n", p.x, p.y, p.z);
    }
    for uv in &plane.uvs {
        obj += &format!("vt {} {}\n", uv.x, 1.0 - uv.y);
    }
    for t in plane.indices.chunks_exact(3) {
        obj += &format!("f {a}/{a} {b}/{b} {c}/{c}\n", a = t[0] + 1, b = t[1] + 1, c = t[2] + 1);
    }
    let new_mesh = dir.join("low.obj");
    std::fs::write(&new_mesh, obj).unwrap();
    // A 1x1 PNG as an image resource.
    let png = &[
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08,
        0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8, 0xcf, 0xc0, 0xf0, 0x1f, 0x00,
        0x05, 0x00, 0x01, 0xff, 0x89, 0x99, 0x3d, 0x1d, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];
    s.import_resource_bytes("dot.png", png.to_vec()).unwrap();
    execute(&mut s, "bake.skew.set", &json!({ "value": 1.0 })).unwrap();
    execute(&mut s, "bake.autoRebake", &json!({ "map": "height" })).unwrap();
    assert!(due(&mut s).is_empty());
    s.modified = false;

    execute(&mut s, "file.changeMesh", &json!({ "path": new_mesh.display().to_string() })).unwrap();
    assert_eq!(s.bake_meshes.high.len(), 1, "the high poly stays");
    assert!(s.bake_meshes.skew.is_some(), "the skew map stays");
    assert!(s.modified, "the project differs from the saved one");
    assert_eq!(s.images.len(), 1, "image resources are decoded in the new session");
    assert_eq!(execute(&mut s, "bake.autoRebake", &Value::Null).unwrap()["map"], "height", "Auto-rebake stays on");
    assert_eq!(due(&mut s), [0], "and the map baked for the old mesh is out of date");

    std::fs::remove_dir_all(&dir).ok();
}
