//! The Baking Log's record of the latest bake, map by map

use std::collections::BTreeSet;

use serde_json::json;
use texturecraft_engine::bake::{BakeHighState, BakeMapState};
use texturecraft_engine::core::BakedMap;
use texturecraft_engine::geom::primitives;
use texturecraft_engine::{Session, execute, sample_session};

#[test]
fn bake_run_lists_every_checked_map_of_the_set_as_baked() {
    let mesh = primitives::builtin("plane").unwrap();
    let mut session = Session::new_project("t", mesh, "builtin:plane", None, 256).unwrap();

    execute(&mut session, "bake.run", &json!({ "resolution": 64, "maps": ["opacity", "position"], "dilation": 0 })).unwrap();

    let set_name = &session.doc.project.texture_sets[0].name;
    let rows = &session.bake_log.maps;
    let maps: BTreeSet<BakedMap> = rows.iter().map(|row| row.map).collect();
    assert_eq!(maps, BTreeSet::from([BakedMap::Position, BakedMap::Opacity]));
    assert!(rows.iter().all(|row| row.state == BakeMapState::Baked && &row.set_name == set_name), "{rows:?}");
    assert!(!session.bake_log.is_running());
}

#[test]
fn a_canceled_bake_keeps_the_finished_sets_and_cancels_the_rest() {
    let mut session = sample_session(256, 64).unwrap();
    for set in &mut session.doc.project.texture_sets {
        set.bake.maps.select([BakedMap::Normal, BakedMap::Curvature]);
    }
    let log = &mut session.bake_log;
    log.begin(&session.doc.project, &session.doc.bake_meshes.high, &[0, 1]);

    log.set_started(0);
    log.map_baked(0, BakedMap::Normal);
    log.map_baked(0, BakedMap::Curvature);
    log.set_finished(0);
    log.set_started(1);
    log.map_baked(1, BakedMap::Normal);
    let baking: Vec<_> = log.maps.iter().filter(|row| row.state == BakeMapState::Baking).map(|row| (row.texture_set, row.map)).collect();
    assert_eq!(baking, [(1, BakedMap::Curvature)], "the next map of the running set is the one in progress");

    log.cancel(Some(1));

    let state_of = |set: usize| log.maps.iter().filter(move |row| row.texture_set == set).map(|row| row.state.clone()).collect::<Vec<_>>();
    assert!(state_of(0).iter().all(|state| *state == BakeMapState::Baked), "{:?}", log.maps);
    assert!(state_of(1).iter().all(|state| *state == BakeMapState::Canceled), "maps of the set not kept are canceled: {:?}", log.maps);
    assert!(!log.is_running());
}

#[test]
fn bake_log_reports_each_map_with_its_state_and_a_failures_reason() {
    let mesh = primitives::builtin("plane").unwrap();
    let mut session = Session::new_project("t", mesh, "builtin:plane", None, 256).unwrap();
    execute(&mut session, "bake.run", &json!({ "resolution": 64, "maps": ["opacity", "position"], "dilation": 0 })).unwrap();

    let log = execute(&mut session, "bake.log", &json!({})).unwrap();
    assert_eq!(log["running"], false);
    let maps: BTreeSet<&str> = log["maps"].as_array().unwrap().iter().map(|row| row["map"].as_str().unwrap()).collect();
    assert_eq!(maps, BTreeSet::from(["opacity", "position"]));
    assert!(log["maps"].as_array().unwrap().iter().all(|row| row["state"] == "baked" && row["textureSet"] == 0 && row.get("reason").is_none()), "{log}");

    session.bake_log.set_failed(0, "The high poly file could not be read.");
    let log = execute(&mut session, "bake.log", &json!({})).unwrap();
    assert!(log["maps"].as_array().unwrap().iter().all(|row| row["state"] == "failed" && row["reason"] == "The high poly file could not be read."), "{log}");
}

#[test]
fn a_bake_lists_the_high_polys_each_set_loads_and_skips_a_set_baking_from_its_low_poly() {
    let mut session = sample_session(256, 64).unwrap();
    session.add_high_poly_bytes("ball_high.obj", b"o Ball_high\nv 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n".to_vec()).unwrap();
    session.doc.project.texture_sets[1].bake.high_poly.use_low_as_high = true;

    execute(&mut session, "bake.run", &json!({ "ts": [0, 1], "resolution": 64, "maps": ["opacity"], "dilation": 0 })).unwrap();

    let highs: Vec<_> = session.bake_log.highs.iter().map(|row| (row.texture_set, row.file.as_str(), row.state.clone())).collect();
    assert_eq!(highs, [(0, "ball_high.obj", BakeHighState::Loaded)]);
    let log = execute(&mut session, "bake.log", &json!({})).unwrap();
    assert_eq!(log["highPolys"][0]["state"], "loaded", "{log}");
}

#[test]
fn a_set_that_fails_before_its_high_polys_are_ready_marks_them_failed_and_a_cancel_the_rest_canceled() {
    let mut session = sample_session(256, 64).unwrap();
    session.add_high_poly_bytes("ball_high.obj", b"o Ball_high\nv 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n".to_vec()).unwrap();
    let log = &mut session.bake_log;
    log.begin(&session.doc.project, &session.doc.bake_meshes.high, &[0, 1]);

    log.set_started(0);
    assert_eq!(log.highs[0].state, BakeHighState::Loading);
    assert!(log.is_loading_high_polys());
    log.set_failed(0, "The cage file could not be read.");
    log.set_started(1);
    log.cancel(Some(1));

    assert_eq!(log.highs[0].state, BakeHighState::Failed("The cage file could not be read.".into()));
    assert_eq!(log.highs[1].state, BakeHighState::Canceled);
    assert!(!log.is_loading_high_polys());
}
