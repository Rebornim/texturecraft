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

#[test]
fn the_baking_process_tab_lists_each_set_and_map_with_how_it_went() {
    let mut harness = harness();
    {
        // Set 1 has no tile left to bake, so its bake fails
        let set = &mut harness.state_mut().session.doc.project.texture_sets[1];
        set.uv_tiles = vec![1001];
        set.bake_excluded_tiles.insert(1001);
        set.bake.maps.select([BakedMap::Id]);
    }
    assert!(harness.state().ui.dock.is_visible(Panel::BakingLog), "the bake layout shows the Baking Log");

    harness.state_mut().start_bake(vec![0, 1]);
    run_to_the_end(&mut harness);

    let names: Vec<String> = harness.state().session.doc.project.texture_sets.iter().map(|set| set.name.clone()).collect();
    for map in ["ID", "Position"] {
        assert!(harness.query_by_label(&format!("{} {map}, Baked", names[0])).is_some(), "set 0's {map} row is ticked");
    }
    assert!(harness.query_by_label(&format!("{} ID, Not baked", names[1])).is_some(), "set 1's row failed");
}
