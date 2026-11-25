//! Auto-rebake: one map per texture set, due when it goes out of date, rebaked alone

use serde_json::json;
use texturecraft_engine::bake::Progress;
use texturecraft_engine::core::BakedMap;
use texturecraft_engine::geom::primitives;
use texturecraft_engine::{Session, execute};

/// A plane with its position and ID maps baked
fn baked_plane() -> Session {
    let mesh = primitives::builtin("plane").unwrap();
    let mut session = Session::new_project("t", mesh, "builtin:plane", None, 128).unwrap();
    execute(&mut session, "bake.run", &json!({ "resolution": 64, "maps": ["position", "id"], "dilation": 0 })).unwrap();
    session
}

fn due_maps(session: &Session) -> Vec<(usize, BakedMap)> {
    session.auto_rebake_due().into_iter().map(|(set, map, _)| (set, map)).collect()
}

#[test]
fn one_map_per_set_is_auto_rebaked_and_it_is_due_only_once_out_of_date() {
    let mut session = baked_plane();
    execute(&mut session, "bake.autoRebake", &json!({ "map": "id" })).unwrap();
    let turned_on = execute(&mut session, "bake.autoRebake", &json!({ "map": "position" })).unwrap();
    assert_eq!(turned_on, json!({ "ts": 0, "autoRebake": "position" }), "the second map replaces the first");
    assert!(due_maps(&session).is_empty(), "up to date: {:?}", due_maps(&session));

    execute(&mut session, "bake.settings", &json!({ "positionMode": "boundingBox" })).unwrap();
    assert_eq!(due_maps(&session), [(0, BakedMap::Position)]);

    execute(&mut session, "bake.autoRebake", &json!({ "map": null })).unwrap();
    assert!(due_maps(&session).is_empty(), "turned off: {:?}", due_maps(&session));
    assert!(execute(&mut session, "bake.autoRebake", &json!({ "map": "glow" })).is_err());
}

#[test]
fn the_auto_rebake_map_is_saved_with_the_project_and_undone_like_an_edit() {
    let mut session = baked_plane();
    let before = session.to_project_bytes().unwrap();
    execute(&mut session, "bake.autoRebake", &json!({ "map": "position" })).unwrap();

    let reopened = Session::from_project_bytes(&session.to_project_bytes().unwrap()).unwrap();
    assert_eq!(reopened.doc.project.texture_sets[0].auto_rebake, Some(BakedMap::Position));
    let inspect = execute(&mut session, "project.inspect", &json!({})).unwrap();
    assert_eq!(inspect["textureSets"][0]["autoRebake"], json!("position"));

    execute(&mut session, "edit.undo", &json!({})).unwrap();
    assert_eq!(session.doc.project.texture_sets[0].auto_rebake, None);
    assert_eq!(Session::from_project_bytes(&before).unwrap().doc.project.texture_sets[0].auto_rebake, None, "older projects have none");
}

#[test]
fn a_one_map_bake_brings_that_map_up_to_date_and_leaves_the_others_alone() {
    let mut session = baked_plane();
    execute(&mut session, "bake.settings", &json!({ "maps": ["id"], "positionMode": "boundingBox" })).unwrap();
    let id_before = session.baked(0).unwrap().maps[&BakedMap::Id].clone();

    let task = session.bake_task_for(0, &[BakedMap::Position]).unwrap();
    assert_eq!(task.maps(), [BakedMap::Position], "an unchecked map bakes when asked for");
    let output = task.run_streaming(&Progress::new(), &mut |_| {}).unwrap();
    session.apply_streamed_bake(0, output).unwrap();

    assert!(session.bake_changes(0, BakedMap::Position).is_empty(), "{:?}", session.bake_changes(0, BakedMap::Position));
    let baked = session.baked(0).unwrap();
    assert!(std::sync::Arc::ptr_eq(&baked.maps[&BakedMap::Id], &id_before), "the ID map was not baked again");
    assert_eq!(session.doc.project.texture_sets[0].bake.maps.enabled().collect::<Vec<_>>(), [BakedMap::Id], "the checks stay");
}
