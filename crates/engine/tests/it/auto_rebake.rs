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

#[test]
fn a_new_output_size_rebakes_every_checked_map_so_none_is_lost() {
    let mut session = baked_plane();
    execute(&mut session, "bake.autoRebake", &json!({ "map": "position" })).unwrap();
    execute(&mut session, "bake.settings", &json!({ "resolution": 128 })).unwrap();

    let maps = session.auto_rebake_maps(0, BakedMap::Position);
    let output = session.bake_task_for(0, &maps).unwrap().run_streaming(&Progress::new(), &mut |_| {}).unwrap();
    session.apply_streamed_bake(0, output).unwrap();

    let baked = session.baked(0).unwrap();
    assert!(baked.maps.contains_key(&BakedMap::Id), "the ID map survives the new size: {:?}", baked.maps.keys().collect::<Vec<_>>());
    assert!(session.out_of_date_maps(0).is_empty(), "{:?}", session.out_of_date_maps(0));
}

#[test]
fn a_skew_stroke_rebakes_only_its_box_and_the_box_matches_a_whole_bake() {
    let mut session = super::bake::leaning_session(true);
    execute(&mut session, "bake.run", &json!({ "maps": ["position"], "dilation": 2 })).unwrap();
    execute(&mut session, "bake.autoRebake", &json!({ "map": "position" })).unwrap();
    assert!(session.auto_rebake_painted().is_empty(), "nothing painted since the bake");
    let before = session.baked(0).unwrap().maps[&BakedMap::Position].clone();

    execute(&mut session, "bake.skew.paint", &json!({ "points": [[0.3, 0.3], [0.4, 0.35]], "radius": 0.05, "size": 64 })).unwrap();
    let painted: Vec<_> = session.auto_rebake_painted().into_iter().map(|(set, map, _, _)| (set, map)).collect();
    assert_eq!(painted, [(0, BakedMap::Position)]);
    assert!(session.auto_rebake_due().is_empty(), "painting is not a settings change: {:?}", due_maps(&session));

    let mut task = session.bake_painted_task(0, BakedMap::Position).unwrap();
    task.tracer = None;
    let region = task.region.expect("a stroke rebakes only its box");
    let mut whole = session.bake_task_for(0, &[BakedMap::Position]).unwrap();
    whole.tracer = None;
    let whole = whole.run(&Progress::new()).unwrap().maps.remove(&BakedMap::Position).unwrap();
    let output = task.run_streaming(&Progress::new(), &mut |_| {}).unwrap();

    let width = output.resolution as usize;
    assert!(((region.max.0 - region.min.0) as usize) < width / 2, "the box is far smaller than the map: {region:?}");
    let components = BakedMap::Position.components();
    let pixels = output.maps[&BakedMap::Position].chunks(components).zip(before.chunks(components)).zip(whole.chunks(components));
    let mut changed = 0;
    for (index, ((rebaked, kept), baked_whole)) in pixels.enumerate() {
        if region.contains(index % width, index / width) {
            assert_eq!(rebaked, baked_whole, "texel {index} inside the box");
            changed += usize::from(rebaked != kept);
        } else {
            assert_eq!(rebaked, kept, "texel {index} outside the box");
        }
    }
    assert!(changed > 0, "the stroke changed the map inside its box");

    session.apply_streamed_bake(0, output).unwrap();
    assert!(session.auto_rebake_painted().is_empty(), "the rebake took the stroke");
}

#[test]
fn a_stroke_painted_while_the_rebake_runs_waits_for_the_next_one() {
    let mut session = super::bake::leaning_session(true);
    execute(&mut session, "bake.run", &json!({ "maps": ["position"], "dilation": 2 })).unwrap();
    execute(&mut session, "bake.autoRebake", &json!({ "map": "position" })).unwrap();
    let stroke = |at: f32| json!({ "points": [[at, at]], "radius": 0.05, "size": 64 });
    execute(&mut session, "bake.skew.paint", &stroke(0.3)).unwrap();

    let mut task = session.bake_painted_task(0, BakedMap::Position).unwrap();
    task.tracer = None;
    execute(&mut session, "bake.skew.paint", &stroke(0.7)).unwrap();
    let output = task.run_streaming(&Progress::new(), &mut |_| {}).unwrap();
    session.apply_streamed_bake(0, output).unwrap();

    let painted: Vec<_> = session.auto_rebake_painted().into_iter().map(|(set, map, _, _)| (set, map)).collect();
    assert_eq!(painted, [(0, BakedMap::Position)], "the second stroke is not in the stored map yet");
}
