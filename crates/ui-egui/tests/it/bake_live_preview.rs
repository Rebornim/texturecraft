//! The map preview: the selected map's settings show it on the model, small while a slider is dragged

use egui::{Event, Modifiers, PointerButton, Role};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use texturecraft_engine::core::{BakeResolution, BakedMap};
use texturecraft_ui_egui::TextureCraftApp;
use texturecraft_ui_egui::docking::Panel;
use texturecraft_ui_egui::state::Mode;

/// Bake mode on the sample with Ambient Occlusion's settings in front
fn harness() -> Harness<'static, TextureCraftApp> {
    let mut session = texturecraft_engine::sample_session(256, 64).unwrap();
    session.doc.project.texture_sets[0].bake = Default::default();
    session.doc.project.texture_sets[0].bake.common.resolution = BakeResolution::R1024;
    // No earlier bake to start the preview from: its first picture is a preview bake at the chosen size
    session.doc.baked.clear();
    let mut harness = Harness::builder().with_size(egui::vec2(1600.0, 1200.0)).build_eframe(|context| TextureCraftApp::new(context, session));
    harness.state_mut().ui.mode = Mode::Baking;
    harness.state_mut().ui.bake_visualization.map_preview_size = BakeResolution::R256;
    harness.run_steps(4);
    harness.get(egui_kittest::kittest::by().role(Role::Button).label("Ambient Occlusion")).click();
    harness.state_mut().ui.dock.focus(Panel::MeshMapSettings);
    harness.run_steps(3);
    harness
}

/// The size of the picture the preview shows for the first set, once it is `wanted` (the worker gets a few seconds)
fn wait_for_preview(harness: &mut Harness<'static, TextureCraftApp>, wanted: impl Fn(u32) -> bool) -> Option<u32> {
    for _ in 0..600 {
        harness.run_steps(1);
        let shown =
            harness.state().session.bake_preview_baked(0).and_then(|baked| baked.maps.contains_key(&BakedMap::AmbientOcclusion).then_some(baked.resolution));
        if shown.is_some_and(&wanted) {
            return shown;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    None
}

#[test]
fn the_selected_map_shows_on_the_model_and_common_settings_hide_it_at_once() {
    let mut harness = harness();

    let shown = wait_for_preview(&mut harness, |_| true);
    assert!(shown.is_some(), "selecting a map's settings shows it");
    assert_eq!(harness.state().session.view, texturecraft_engine::ViewMode::MeshMap(BakedMap::AmbientOcclusion));

    harness.state_mut().ui.dock.focus(Panel::CommonSettings);
    harness.run_steps(1);
    assert!(!harness.state().session.bake_preview_active(), "no lingering picture");
    assert_ne!(harness.state().session.view, texturecraft_engine::ViewMode::MeshMap(BakedMap::AmbientOcclusion));
}

#[test]
fn a_dragged_slider_previews_smaller_than_the_picture_it_settles_on() {
    let mut harness = harness();
    let settled = wait_for_preview(&mut harness, |_| true).unwrap();
    let track = harness.get_by_role_and_label(Role::Slider, "Secondary Rays").rect();
    let (from, to) = (track.center(), track.left_center() + egui::vec2(track.width() * 0.1, 0.0));

    harness.hover_at(from);
    harness.run_steps(1);
    harness.event(Event::PointerButton { pos: from, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    harness.run_steps(1);
    harness.hover_at(to);
    let dragged = wait_for_preview(&mut harness, |size| size < settled);
    harness.event(Event::PointerButton { pos: to, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    let released = wait_for_preview(&mut harness, |size| size == settled);

    assert!(dragged.is_some(), "while dragging the preview bakes smaller");
    assert!(released.is_some(), "the release refines it to the chosen size");
}

#[test]
fn switched_off_the_preview_never_shows() {
    let mut harness = harness();
    wait_for_preview(&mut harness, |_| true);

    harness.state_mut().ui.bake_visualization.map_preview = false;
    harness.run_steps(1);

    assert!(!harness.state().session.bake_preview_active());
}

#[test]
fn pictures_keep_coming_while_the_slider_is_still_moving() {
    let mut harness = harness();
    // Many rays: each preview bake takes several frames, as on a real mesh
    harness.state_mut().session.doc.project.texture_sets[0].bake.maps.ao.parameters.rays.rays = 256;
    wait_for_preview(&mut harness, |_| true);
    let track = harness.get_by_role_and_label(Role::Slider, "Spread Angle").rect();
    let from = track.left_center() + egui::vec2(track.width() * 0.05, 0.0);
    let picture = |harness: &Harness<'static, TextureCraftApp>| {
        harness.state().session.bake_preview_baked(0).and_then(|baked| baked.maps.get(&BakedMap::AmbientOcclusion).cloned())
    };

    harness.hover_at(from);
    harness.run_steps(1);
    harness.event(Event::PointerButton { pos: from, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    // Holding the last picture keeps a new one from reusing its address
    let mut last = picture(&harness);
    let mut pictures = 0;
    for step in 0..600 {
        // Every frame moves the slider, as a steady drag does
        harness.hover_at(from + egui::vec2(track.width() * 0.002 * (step % 200) as f32, 0.0));
        harness.run_steps(1);
        let now = picture(&harness);
        if now.as_ref().map(std::sync::Arc::as_ptr) != last.as_ref().map(std::sync::Arc::as_ptr) {
            pictures += 1;
            last = now;
        }
    }

    assert!(pictures > 2, "new pictures arrive while the slider never stops");
}
