//! The Baking Log's record of the latest bake, map by map

use std::collections::BTreeSet;

use serde_json::json;
use texturecraft_engine::bake::BakeMapState;
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
    log.begin(&session.doc.project, &[0, 1]);

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
