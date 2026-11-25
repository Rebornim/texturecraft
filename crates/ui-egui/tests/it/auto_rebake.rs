//! Auto-rebake in the app: a settings change rebakes the auto-rebake map alone, in the background

use egui_kittest::Harness;
use serde_json::json;
use texturecraft_engine::bake::BakeMapState;
use texturecraft_engine::core::BakedMap;
use texturecraft_engine::geom::primitives;
use texturecraft_engine::{Session, execute};
use texturecraft_ui_egui::TextureCraftApp;
use texturecraft_ui_egui::state::Mode;

/// A plane in Bake mode with its position and ID maps baked and Auto-rebake on for the position map
fn harness(resolution: u32) -> Harness<'static, TextureCraftApp> {
    let mesh = primitives::builtin("plane").unwrap();
    let mut session = Session::new_project("t", mesh, "builtin:plane", None, 128).unwrap();
    execute(&mut session, "bake.run", &json!({ "resolution": resolution, "maps": ["position", "id"], "dilation": 0 })).unwrap();
    execute(&mut session, "bake.autoRebake", &json!({ "map": "position" })).unwrap();

    let mut harness = Harness::builder().with_size(egui::vec2(1600.0, 1000.0)).build_eframe(|context| TextureCraftApp::new(context, session));
    harness.state_mut().ui.mode = Mode::Baking;
    harness.run_steps(4);
    harness
}

fn change_settings(harness: &mut Harness<'static, TextureCraftApp>, params: serde_json::Value) {
    execute(&mut harness.state_mut().session, "bake.settings", &params).unwrap();
}

/// Step frames until `done` holds, up to a generous limit
fn step_until(harness: &mut Harness<'static, TextureCraftApp>, what: &str, done: impl Fn(&TextureCraftApp) -> bool) {
    for _ in 0..4000 {
        harness.run_steps(1);
        if done(harness.state()) {
            return;
        }

        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    panic!("{what} did not happen");
}

#[test]
fn a_settings_change_rebakes_only_the_auto_rebake_map_without_a_toast() {
    let mut harness = harness(64);
    let id_before = harness.state().session.baked(0).unwrap().maps[&BakedMap::Id].clone();
    change_settings(&mut harness, json!({ "positionMode": "boundingBox" }));

    step_until(&mut harness, "the rebake", |app| app.bake.is_some());
    step_until(&mut harness, "the end of the rebake", |app| app.bake.is_none());

    let app = harness.state();
    assert!(app.session.bake_changes(0, BakedMap::Position).is_empty(), "{:?}", app.session.bake_changes(0, BakedMap::Position));
    assert!(std::sync::Arc::ptr_eq(&app.session.baked(0).unwrap().maps[&BakedMap::Id], &id_before), "the ID map was not baked again");
    let rows: Vec<_> = app.session.bake_log.maps.iter().map(|row| (row.map, row.state.clone())).collect();
    assert_eq!(rows, [(BakedMap::Position, BakeMapState::Baked)]);
    assert!(app.toasts.is_empty(), "a rebake that went well is quiet");
}

#[test]
fn nothing_is_rebaked_outside_bake_mode() {
    let mut harness = harness(64);
    harness.state_mut().ui.mode = Mode::Painting;
    change_settings(&mut harness, json!({ "positionMode": "boundingBox" }));

    harness.run_steps(120);

    assert!(harness.state().bake.is_none());
    assert!(!harness.state().session.bake_changes(0, BakedMap::Position).is_empty());
}

#[test]
fn a_change_while_the_rebake_runs_cancels_it_so_the_newest_settings_win() {
    let mut harness = harness(64);
    execute(&mut harness.state_mut().session, "bake.run", &json!({ "maps": ["ambientOcclusion"], "ao": { "rays": 1 } })).unwrap();
    execute(&mut harness.state_mut().session, "bake.autoRebake", &json!({ "map": "ambientOcclusion" })).unwrap();

    // A slow rebake, so the next change lands while it runs
    change_settings(&mut harness, json!({ "resolution": 2048, "ao": { "rays": 256 } }));
    step_until(&mut harness, "the rebake", |app| app.bake.as_ref().is_some_and(|job| job.current.is_some()));
    change_settings(&mut harness, json!({ "ao": { "rays": 128 } }));
    harness.run_steps(1);

    let app = harness.state();
    assert!(app.bake.is_none(), "the outdated rebake stopped");
    assert_eq!(app.session.bake_log.maps[0].state, BakeMapState::Canceled);
    execute(&mut harness.state_mut().session, "bake.autoRebake", &json!({ "map": null })).unwrap();
}
