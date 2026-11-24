// Scratch timing for the map preview (drop before the PR): UI frame times while a map slider is dragged.
// Compiles on the branch before the feature too, so the same file measures old and new.
use egui::{Event, Modifiers, PointerButton, Role};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use texturecraft_ui_egui::TextureCraftApp;
use texturecraft_ui_egui::docking::Panel;
use texturecraft_ui_egui::state::Mode;

#[test]
#[ignore]
fn live_preview_frames() {
    let path = std::env::var("PREVIEW_MESH").unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let mut session = texturecraft_engine::Session::from_mesh_bytes("low.fbx", &bytes, 2048, "frames").unwrap();
    session.add_high_poly_bytes("high.fbx", bytes).unwrap();
    session.doc.project.texture_sets[0].bake.high_poly.use_low_as_high = false;
    let mut h = Harness::builder().with_size(egui::vec2(1600.0, 1000.0)).build_eframe(|cc| TextureCraftApp::new(cc, session));
    h.state_mut().ui.mode = Mode::Baking;
    h.run_steps(10);
    h.get(egui_kittest::kittest::by().role(Role::Button).label("Ambient Occlusion")).click();
    h.state_mut().ui.dock.focus(Panel::MeshMapSettings);
    h.run_steps(30);

    let track = h.get_by_role_and_label(Role::Slider, "Secondary Rays").rect();
    let mut frames = Vec::new();
    for sweep in 0..4 {
        let from = track.left_center() + egui::vec2(track.width() * if sweep % 2 == 0 { 0.1 } else { 0.4 }, 0.0);
        let step = if sweep % 2 == 0 { 0.01 } else { -0.01 };
        h.hover_at(from);
        h.run_steps(1);
        h.event(Event::PointerButton { pos: from, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
        for k in 1..=30 {
            h.hover_at(from + egui::vec2(track.width() * step * k as f32, 0.0));
            let started = std::time::Instant::now();
            h.run_steps(1);
            frames.push(started.elapsed().as_secs_f64() * 1000.0);
            std::thread::sleep(std::time::Duration::from_millis(16));
        }
        h.event(Event::PointerButton { pos: from, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
        for _ in 0..40 {
            let started = std::time::Instant::now();
            h.run_steps(1);
            frames.push(started.elapsed().as_secs_f64() * 1000.0);
            std::thread::sleep(std::time::Duration::from_millis(16));
        }
    }
    frames.sort_by(f64::total_cmp);
    let at = |q: f64| frames[((frames.len() - 1) as f64 * q) as usize];
    println!("FRAMES n {} median {:.2} ms p95 {:.2} ms max {:.2} ms", frames.len(), at(0.5), at(0.95), at(1.0));
}
