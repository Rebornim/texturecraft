//! The Baking Log panel: the latest bake, map by map

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
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
