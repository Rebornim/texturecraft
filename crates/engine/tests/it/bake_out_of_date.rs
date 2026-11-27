//! Out-of-date mesh maps: what each map was baked from, compared with what a bake would read now

use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::json;
use texturecraft_engine::bake::Progress;
use texturecraft_engine::core::{BakeChange, BakedMap, CageType};
use texturecraft_engine::geom::primitives;
use texturecraft_engine::{Session, execute};

/// A plane with its position map baked by `bake.run`
fn baked_plane() -> Session {
    let mesh = primitives::builtin("plane").unwrap();
    let mut session = Session::new_project("t", mesh, "builtin:plane", None, 128).unwrap();
    execute(&mut session, "bake.run", &json!({ "resolution": 64, "maps": ["position"], "dilation": 0 })).unwrap();
    session
}

fn changes(session: &Session) -> Vec<BakeChange> {
    session.bake_changes(0, BakedMap::Position)
}

#[test]
fn a_map_goes_out_of_date_when_its_settings_or_the_shared_ones_change_and_a_bake_brings_it_back() {
    let mut session = baked_plane();
    assert!(changes(&session).is_empty(), "fresh bake: {:?}", changes(&session));

    execute(&mut session, "bake.settings", &json!({ "positionMode": "boundingBox" })).unwrap();
    assert_eq!(changes(&session), [BakeChange::MapSettings]);
    execute(&mut session, "bake.settings", &json!({ "dilation": 4 })).unwrap();
    assert_eq!(changes(&session), [BakeChange::Settings, BakeChange::MapSettings]);
    let inspect = execute(&mut session, "project.inspect", &json!({})).unwrap();
    assert_eq!(inspect["textureSets"][0]["outOfDate"], json!({ "position": ["settings", "mapSettings"] }));

    execute(&mut session, "bake.run", &json!({})).unwrap();
    assert!(changes(&session).is_empty(), "rebaked: {:?}", changes(&session));
}

#[test]
fn the_record_is_saved_with_the_project() {
    let mut session = baked_plane();
    execute(&mut session, "bake.settings", &json!({ "positionMode": "boundingBox" })).unwrap();

    let reopened = Session::from_project_bytes(&session.to_project_bytes().unwrap()).unwrap();

    assert_eq!(changes(&reopened), [BakeChange::MapSettings]);
}

#[test]
fn maps_saved_before_records_existed_and_imported_maps_never_go_out_of_date() {
    let mut old = baked_plane();
    old.doc.project.texture_sets[0].bake_sources.clear();
    let mut old = Session::from_project_bytes(&old.to_project_bytes().unwrap()).unwrap();
    execute(&mut old, "bake.settings", &json!({ "positionMode": "boundingBox" })).unwrap();
    assert!(changes(&old).is_empty(), "{:?}", changes(&old));

    let mut imported = baked_plane();
    let mut png = Vec::new();
    image::RgbImage::from_pixel(4, 4, image::Rgb([128, 64, 32])).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
    imported.import_resource_bytes("position.png", png).unwrap();
    execute(&mut imported, "textureSet.meshMap.set", &json!({ "map": "position", "image": "position.png" })).unwrap();
    execute(&mut imported, "bake.settings", &json!({ "positionMode": "boundingBox" })).unwrap();
    assert!(changes(&imported).is_empty(), "{:?}", changes(&imported));
}

#[test]
fn adding_a_high_poly_puts_the_maps_out_of_date() {
    let mut session = baked_plane();

    session.add_high_poly_bytes("detail_high.obj", b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n".to_vec()).unwrap();

    assert_eq!(changes(&session), [BakeChange::HighPoly]);
}

#[test]
fn a_cage_file_counts_only_for_sets_that_bake_with_it() {
    let mut session = baked_plane();
    session.add_high_poly_bytes("detail_high.obj", b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n".to_vec()).unwrap();
    execute(&mut session, "bake.run", &json!({})).unwrap();

    let cage: String = (0..session.mesh.triangle_count()).map(|_| "v 0 0 0\nv 1 0 0\nv 0 1 0\nf -3 -2 -1\n").collect();
    session.set_cage_bytes("cage.obj", cage.into_bytes()).unwrap();
    assert!(changes(&session).is_empty(), "the cage type is not Custom File: {:?}", changes(&session));

    session.doc.project.texture_sets[0].bake.cage.kind = CageType::CustomFile;
    assert!(changes(&session).contains(&BakeChange::Cage), "{:?}", changes(&session));
}

#[test]
fn a_setting_changed_while_the_bake_runs_leaves_the_new_map_out_of_date() {
    let mut session = baked_plane();
    let task = session.bake_task(0).unwrap();
    execute(&mut session, "bake.settings", &json!({ "positionMode": "boundingBox" })).unwrap();

    let streamed_once = AtomicBool::new(false);
    let output = task.run_streaming(&Progress::new(), &mut |_| streamed_once.store(true, Ordering::Relaxed)).unwrap();
    session.apply_streamed_bake(0, output).unwrap();

    assert!(streamed_once.load(Ordering::Relaxed));
    assert_eq!(changes(&session), [BakeChange::MapSettings]);
}

/// Two quads side by side, one texture set each, lifted by `lift` where a high poly or cage stands off the low poly
fn two_parts(lift: f32) -> String {
    let mut obj = String::new();
    for left in [0.0, 2.0] {
        let right = left + 1.0;
        obj += &format!("v {left} 0 {lift}\nv {right} 0 {lift}\nv {right} 1 {lift}\nv {left} 1 {lift}\n");
    }
    obj += "vt 0 0\nvt 1 0\nvt 1 1\nvt 0 1\n";
    obj += "usemtl Left\nf 1/1 2/2 3/3\nf 1/1 3/3 4/4\nusemtl Right\nf 5/1 6/2 7/3\nf 5/1 7/3 8/4\n";
    obj
}

#[test]
fn a_reopened_project_keeps_every_sets_maps_up_to_date() {
    let mut session = Session::from_mesh_bytes_with_uvs("parts.obj", two_parts(0.0).as_bytes(), 128, "parts", None, false).unwrap();
    session.add_high_poly_bytes("parts_high.obj", two_parts(0.05).into_bytes()).unwrap();
    session.set_cage_bytes("parts_cage.obj", two_parts(0.2).into_bytes()).unwrap();
    for texture_set in &mut session.doc.project.texture_sets {
        texture_set.bake.cage.kind = CageType::CustomFile;
    }
    execute(&mut session, "bake.run", &json!({ "ts": "all", "resolution": 32, "maps": ["position", "normal"], "dilation": 0 })).unwrap();
    execute(&mut session, "bake.autoRebake", &json!({ "ts": 1, "map": "normal" })).unwrap();
    let records = |session: &Session| session.doc.project.texture_sets.iter().map(|set| set.bake_sources.clone()).collect::<Vec<_>>();
    assert_eq!(session.set_count(), 2);
    assert!(records(&session).iter().all(|sources| sources.len() == 2), "both maps of both sets are recorded");

    let reopened = Session::from_project_bytes(&session.to_project_bytes().unwrap()).unwrap();

    assert_eq!(records(&reopened), records(&session), "the records are saved as they were");
    for texture_set in 0..reopened.set_count() {
        assert!(reopened.out_of_date_maps(texture_set).is_empty(), "set {texture_set}: {:?}", reopened.out_of_date_maps(texture_set));
    }
    assert!(reopened.auto_rebake_due().is_empty(), "opening the project starts no rebake");
}
