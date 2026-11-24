// Scratch snapshots for the bake pass 2 visual build: not committed
use std::path::PathBuf;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use texturecraft_ui_egui::TextureCraftApp;
use texturecraft_ui_egui::state::Mode;

/// A tiny high poly, under a made-up Windows-style folder so the list shows a shortened path
const HIGH_POLY: &str = "C:/Users/artist/OneDrive/Documents/Desktop/Gunhigh.obj";
const HIGH_POLY_OBJ: &[u8] = b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n";

fn app(_low: &str, high: Option<&str>) -> Harness<'static, TextureCraftApp> {
    let mut session = texturecraft_engine::sample_session(2048, 256).unwrap();
    if high.is_some() {
        session.add_high_poly_bytes(HIGH_POLY, HIGH_POLY_OBJ.to_vec()).unwrap();
    }
    let mut h = Harness::builder().with_size(egui::vec2(1918.0, 1400.0)).with_pixels_per_point(1.0).wgpu().build_eframe(|cc| TextureCraftApp::new(cc, session));
    h.state_mut().ui.mode = Mode::Baking;
    for set in &mut h.state_mut().session.doc.project.texture_sets {
        set.bake.high_poly.use_low_as_high = false;
    }
    h.run_steps(30);
    h
}

#[test]
#[ignore]
fn bake_style_snapshots() {
    let Some(dir) = std::env::var_os("TC_UI_SNAPSHOTS").map(PathBuf::from) else { return };
    std::fs::create_dir_all(&dir).unwrap();
    let mut h = app("Gunlow.fbx", Some("Gunhigh.fbx"));
    h.state_mut().ui.dock.focus(texturecraft_ui_egui::docking::Panel::CommonSettings);
    h.run_steps(10);
    h.render().unwrap().save(dir.join("common-high.png")).unwrap();
    h.get_all_by_label("Reset Cage").next().unwrap().click();
    h.run_steps(6);
    h.render().unwrap().save(dir.join("cage-menu.png")).unwrap();
    h.key_press(egui::Key::Escape);
    h.run_steps(4);
    h.get_by_label("Paint Skew Correction").click();
    h.run_steps(6);
    h.render().unwrap().save(dir.join("skew-painting.png")).unwrap();
    h.get_by_label("Stop Painting").click();
    h.run_steps(4);
    for set in &mut h.state_mut().session.doc.project.texture_sets {
        set.bake.cage.kind = texturecraft_engine::core::CageType::CustomFile;
    }
    h.run_steps(6);
    h.get_by_label("Paint Skew Correction").hover();
    h.run_steps(10);
    h.render().unwrap().save(dir.join("custom-cage.png")).unwrap();

    h.get(egui_kittest::kittest::by().role(egui::accesskit::Role::Button).label("Curvature")).hover();
    h.run_steps(4);
    h.render().unwrap().save(dir.join("bakers-hover.png")).unwrap();
    h.get_by_label("Mesh Map Bakers Menu").click();
    h.run_steps(4);
    h.render().unwrap().save(dir.join("bakers-menu.png")).unwrap();
    h.key_press(egui::Key::Escape);
    h.run_steps(4);
    h.state_mut().ui.dock.focus(texturecraft_ui_egui::docking::Panel::MeshMapSettings);
    for map in ["Ambient Occlusion", "Curvature", "Position", "ID", "Height", "Normal"] {
        h.get(egui_kittest::kittest::by().role(egui::accesskit::Role::Button).label(map)).click();
        h.run_steps(10);
        h.render().unwrap().save(dir.join(format!("map-{map}.png"))).unwrap();
    }

    let mut h = app("Gunlow.fbx", None);
    for set in &mut h.state_mut().session.doc.project.texture_sets {
        set.bake.high_poly.use_low_as_high = false;
    }
    h.state_mut().ui.dock.focus(texturecraft_ui_egui::docking::Panel::CommonSettings);
    h.run_steps(10);
    h.get_by_label("Max Frontal Distance").hover();
    h.run_steps(10);
    h.render().unwrap().save(dir.join("common-no-high.png")).unwrap();

    // The sync window: the second set left the group and differs in size and antialiasing
    {
        let second = &mut h.state_mut().session.doc.project.texture_sets[1].bake;
        second.sync.common = false;
        second.common.resolution = texturecraft_engine::core::BakeResolution::R1024;
        second.common.antialiasing = texturecraft_engine::core::BakeAntialiasing::Subsampling4x4;
        second.common.dilation = 8;
    }
    h.state_mut().ui.dock.focus(texturecraft_ui_egui::docking::Panel::MeshMapBakers);
    h.run_steps(6);
    h.get_by_label("Synchronize Common Settings").click();
    h.run_steps(8);
    h.render().unwrap().save(dir.join("sync-window.png")).unwrap();
}
