//! Symmetry: the toolbar toggle, the settings popup and the 3D view's manipulator

use egui::{Event, Modifiers, PointerButton, Pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};
use texturecraft_engine::Tool;
use texturecraft_engine::geom::glam::Vec3;
use texturecraft_ui_egui::TextureCraftApp;
use texturecraft_ui_egui::theme::Brightness;

fn app_harness() -> Harness<'static, TextureCraftApp> {
    let session = texturecraft_engine::sample_session(1024, 256).unwrap();
    let mut harness =
        Harness::builder().with_size(egui::vec2(1600.0, 960.0)).with_pixels_per_point(1.0).wgpu().build_eframe(|cc| TextureCraftApp::new(cc, session));
    harness.state_mut().session.tool.tool = Tool::Paint;
    harness.run_steps(4);
    harness
}

/// Hold a primary-button drag from `from` to `to` without releasing it
fn hold_drag(harness: &mut Harness<'_, TextureCraftApp>, from: Pos2, to: Pos2) {
    harness.hover_at(from);
    harness.run_steps(1);
    harness.event(Event::PointerButton { pos: from, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    harness.run_steps(2);

    for step in 1..=4 {
        harness.hover_at(from + (to - from) * (step as f32 / 4.0));
        harness.run_steps(1);
    }
}

fn release(harness: &mut Harness<'_, TextureCraftApp>, at: Pos2) {
    harness.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    harness.run_steps(2);
}

fn save(harness: &mut Harness<'_, TextureCraftApp>, dir: &std::path::Path, name: &str) {
    harness.run_steps(4);
    let path = dir.join(format!("{name}.png"));
    harness.render().unwrap().save(&path).unwrap();
    eprintln!("wrote {}", path.display());
}

/// The brush settings as agents read them
fn brush(harness: &mut Harness<'_, TextureCraftApp>) -> Value {
    texturecraft_engine::execute(&mut harness.state_mut().session, "tool.brush", &json!({})).unwrap()
}

/// Where world point `p` is in the 3D view
fn on_screen(harness: &Harness<'_, TextureCraftApp>, p: Vec3) -> Pos2 {
    let rect = harness.state().viewport_rect;
    let ndc = harness.state().session.camera.project(p, rect.width() / rect.height()).unwrap();
    egui::pos2(rect.left() + (ndc.x * 0.5 + 0.5) * rect.width(), rect.top() + (0.5 - ndc.y * 0.5) * rect.height())
}

#[test]
fn the_toolbar_popup_and_manipulator_change_the_brush_symmetry() {
    let mut harness = app_harness();

    harness.get_by_label("Symmetry").click();
    harness.run_steps(2);
    assert_eq!(brush(&mut harness)["symmetry"], json!([true, false, false]), "the toggle mirrors on X");

    harness.get_by_label("Symmetry settings").click();
    harness.run_steps(2);
    harness.get_by_role_and_label(egui::accesskit::Role::RadioButton, "Y").click();
    harness.run_steps(2);
    assert_eq!(brush(&mut harness)["symmetry"], json!([false, true, false]), "one axis at a time");

    harness.get_by_label("Symmetry settings").click();
    harness.state_mut().ui.symmetry.show_manipulator = true;
    harness.run_steps(2);

    // Drag the arrow along Y, starting on its shaft
    let (lo, hi) = harness.state().session.mesh.bounds();
    let origin = (lo + hi) * 0.5;
    let center = on_screen(&harness, origin);
    let along = (on_screen(&harness, origin + Vec3::Y * 0.05) - center).normalized();
    let from = center + along * 40.0;
    hold_drag(&mut harness, from, from + along * 60.0);
    release(&mut harness, from + along * 60.0);

    let offset = brush(&mut harness)["symmetryOffset"].clone();
    assert!(offset[1].as_f64().unwrap() > 0.05, "the plane moved up: {offset}");
    assert_eq!((offset[0].as_f64(), offset[2].as_f64()), (Some(0.0), Some(0.0)), "only along Y: {offset}");

    harness.get_by_label("Symmetry").click();
    harness.run_steps(2);
    let settings = brush(&mut harness);
    assert_eq!(settings["symmetryOn"], json!(false), "the toggle turns it off");
    assert_eq!(settings["symmetry"], json!([false, true, false]), "and keeps the settings");
}

#[test]
#[ignore]
fn symmetry_snapshots() {
    let Some(output_dir) = std::env::var_os("TC_UI_SNAPSHOTS").map(std::path::PathBuf::from) else {
        eprintln!("set TC_UI_SNAPSHOTS=<dir> to write the pictures");
        return;
    };

    std::fs::create_dir_all(&output_dir).unwrap();
    for (brightness, theme) in [(Brightness::MediumDark, "medium"), (Brightness::Dark, "dark"), (Brightness::Light, "light")] {
        let mut harness = app_harness();
        harness.state_mut().ui.prefs.brightness = brightness;
        let center = harness.state().viewport_rect.center();

        // Mirror X from the toolbar toggle, cursor on the ball
        harness.get_by_label("Symmetry").click();
        harness.hover_at(center + vec2(-60.0, -40.0));
        save(&mut harness, &output_dir, &format!("mirror-x-{theme}"));

        harness.get_by_label("Symmetry settings").click();
        save(&mut harness, &output_dir, &format!("popup-mirror-{theme}"));
        harness.get_by_label("Symmetry settings").click();

        // Manipulator on: at rest, hovered, then mid-drag to the right
        harness.state_mut().ui.symmetry.show_manipulator = true;
        harness.hover_at(center + vec2(-150.0, 120.0));
        save(&mut harness, &output_dir, &format!("manipulator-{theme}"));
        harness.hover_at(center + vec2(60.0, 0.0));
        save(&mut harness, &output_dir, &format!("manipulator-hover-{theme}"));
        harness.run_steps(2);
        hold_drag(&mut harness, center + vec2(40.0, 0.0), center + vec2(100.0, 0.0));
        save(&mut harness, &output_dir, &format!("mirror-drag-{theme}"));
        release(&mut harness, center + vec2(100.0, 0.0));
        harness.hover_at(center + vec2(-80.0, 60.0));
        save(&mut harness, &output_dir, &format!("mirror-offset-{theme}"));

        // An older brush mirroring on all three axes
        let symmetry = &mut harness.state_mut().session.tool.brush.symmetry;
        (symmetry.x, symmetry.y, symmetry.z, symmetry.offset) = (true, true, true, [0.0; 3]);
        harness.state_mut().ui.symmetry.show_manipulator = false;
        harness.hover_at(center + vec2(-60.0, -40.0));
        save(&mut harness, &output_dir, &format!("mirror-xyz-{theme}"));

        // Radial: six copies around Y with the axis and manipulator
        let symmetry = &mut harness.state_mut().session.tool.brush.symmetry;
        (symmetry.x, symmetry.y, symmetry.z, symmetry.radial, symmetry.axis) = (false, false, false, 6, 1);
        harness.state_mut().ui.symmetry.show_manipulator = true;
        harness.hover_at(center + vec2(-70.0, -30.0));
        save(&mut harness, &output_dir, &format!("radial-{theme}"));
        hold_drag(&mut harness, center + vec2(22.0, 8.0), center + vec2(70.0, 30.0));
        save(&mut harness, &output_dir, &format!("radial-drag-{theme}"));
        release(&mut harness, center + vec2(70.0, 30.0));

        harness.get_by_label("Symmetry settings").click();
        save(&mut harness, &output_dir, &format!("popup-radial-{theme}"));
    }
}
