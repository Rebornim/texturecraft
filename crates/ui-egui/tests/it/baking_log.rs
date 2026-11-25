//! The Baking Log panel: the latest bake, map by map

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use texturecraft_engine::bake::{BakeLogMap, BakeMapState};
use texturecraft_engine::core::{BakeResolution, BakedMap};
use texturecraft_ui_egui::TextureCraftApp;
use texturecraft_ui_egui::docking::Panel;
use texturecraft_ui_egui::state::Mode;

fn harness() -> Harness<'static, TextureCraftApp> {
    let mut session = texturecraft_engine::sample_session(256, 64).unwrap();
    for texture_set in &mut session.doc.project.texture_sets {
        texture_set.bake = Default::default();
        texture_set.bake.common.resolution = BakeResolution::R64;
        texture_set.bake.maps.select([BakedMap::Id, BakedMap::Position]);
    }

    let mut harness = Harness::builder()
        .with_size(egui::vec2(1600.0, 1000.0))
        .with_context_setup(texturecraft_ui_egui::framework::install_headless)
        .build_eframe(|context| TextureCraftApp::new(context, session));
    harness.state_mut().ui.mode = Mode::Baking;
    harness.run_steps(4);
    harness
}

fn run_to_the_end(harness: &mut Harness<'static, TextureCraftApp>) {
    for _ in 0..4000 {
        harness.run_steps(1);
        if harness.state().bake.is_none() {
            harness.run_steps(2);
            return;
        }

        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    panic!("the bake did not finish");
}

/// Bake sets 0 and 1, where set 1 has no tile left to bake, so its bake fails
fn bake_with_set_1_failing(harness: &mut Harness<'static, TextureCraftApp>, set_1_maps: &[BakedMap]) {
    let set = &mut harness.state_mut().session.doc.project.texture_sets[1];
    set.uv_tiles = vec![1001];
    set.bake_excluded_tiles.insert(1001);
    set.bake.maps.select(set_1_maps.iter().copied());

    harness.state_mut().start_bake(vec![0, 1]);
    run_to_the_end(harness);
}

#[test]
fn the_baking_process_tab_lists_each_set_and_map_with_how_it_went() {
    let mut harness = harness();
    assert!(harness.state().ui.dock.is_visible(Panel::BakingLog), "the bake layout shows the Baking Log");

    bake_with_set_1_failing(&mut harness, &[BakedMap::Id]);

    let names: Vec<String> = harness.state().session.doc.project.texture_sets.iter().map(|set| set.name.clone()).collect();
    for map in ["ID", "Position"] {
        assert!(harness.query_by_label(&format!("{} {map}, Baked", names[0])).is_some(), "set 0's {map} row is ticked");
    }
    assert!(harness.query_by_label(&format!("{} ID, Not baked", names[1])).is_some(), "set 1's row failed");
}

#[test]
fn a_failed_maps_arrow_opens_its_settings() {
    let mut harness = harness();

    // Three rows fit the docked log without scrolling
    harness.state_mut().session.doc.project.texture_sets[0].bake.maps.select([BakedMap::Id]);
    bake_with_set_1_failing(&mut harness, &[BakedMap::Id, BakedMap::Position]);
    harness.state_mut().ui.dock.focus(Panel::CommonSettings);
    harness.run_steps(3);
    assert_eq!(harness.state().session.ts(), 0);
    assert!(harness.query_by_label("Normalization Scale").is_none(), "Position's settings are not shown yet");

    let set_name = harness.state().session.doc.project.texture_sets[1].name.clone();
    harness.get_by_label(&format!("Open the {set_name} Position settings")).click();
    harness.run_steps(3);

    assert_eq!(harness.state().session.ts(), 1, "the failed map's texture set is the active one");
    assert!(harness.state().ui.dock.is_visible(Panel::MeshMapSettings), "the arrow fronts Mesh Map Settings");
    harness.get_by_label("Normalization Scale");
    assert!(
        harness.query_by_label(&format!("Open the {} ID settings", harness.state().session.doc.project.texture_sets[0].name)).is_none(),
        "a baked map has no arrow"
    );
}

/// Put a finished bake of set 0's ID and Position maps in the log, the Position map ending as `position`
fn log_a_bake(harness: &mut Harness<'static, TextureCraftApp>, position: BakeMapState) -> String {
    let set_name = harness.state().session.doc.project.texture_sets[0].name.clone();
    let row = |map, state| BakeLogMap { texture_set: 0, set_name: set_name.clone(), map, state };
    harness.state_mut().session.bake_log.maps = vec![row(BakedMap::Id, BakeMapState::Baked), row(BakedMap::Position, position)];
    harness.run_steps(3);
    set_name
}

#[test]
fn the_issues_filter_keeps_only_the_maps_with_a_problem() {
    let mut harness = harness();
    let set_name = log_a_bake(&mut harness, BakeMapState::Failed("The high poly file could not be read.".into()));
    harness.get_by_label(&format!("{set_name} ID, Baked"));

    harness.get_by_label("1, Show Only Issues").click();
    harness.run_steps(3);

    assert!(harness.query_by_label(&format!("{set_name} ID, Baked")).is_none(), "a baked map is filtered out");
    harness.get_by_label(&format!("{set_name} Position, Not baked"));
    assert!(harness.query_by_label("Deselect filter to view.").is_none(), "an issue left: no empty text");
}

#[test]
fn the_issues_filter_says_so_when_nothing_went_wrong() {
    let mut harness = harness();
    let set_name = log_a_bake(&mut harness, BakeMapState::Baked);
    harness.get_by_label("Show Only Issues").click();
    harness.run_steps(3);

    assert!(harness.query_by_label(&format!("{set_name} ID, Baked")).is_none());
    harness.get_by_label("All textures baked successfully so far.");
    harness.get_by_label("Deselect filter to view.");

    harness.get_by_label("Show Only Issues").click();
    harness.run_steps(3);
    harness.get_by_label(&format!("{set_name} ID, Baked"));
    assert!(harness.query_by_label("All textures baked successfully so far.").is_none(), "off again: every row is back");
}
