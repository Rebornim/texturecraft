//! Temporary captures for the levels editor review (not part of the PR)

use std::path::{Path, PathBuf};

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};
use texturecraft_engine::core::{EffectKind, LayerId};
use texturecraft_engine::{Session, StackTarget, execute};
use texturecraft_ui_egui::TextureCraftApp;
use texturecraft_ui_egui::theme::Brightness;

fn save(harness: &mut Harness<'_, TextureCraftApp>, dir: &Path, name: &str) {
    harness.run_steps(6);
    let image = harness.render().unwrap_or_else(|error| panic!("{name}: {error}"));
    image.save(dir.join(format!("{name}.png"))).unwrap();
    eprintln!("wrote {name}");
}

fn harness(target: &str, brightness: Brightness) -> (Harness<'static, TextureCraftApp>, LayerId) {
    let mut session: Session = texturecraft_engine::sample_session(1024, 256).unwrap();
    execute(&mut session, "layer.add.fill", &json!({"color": [0.8, 0.3, 0.1], "roughness": 0.6})).unwrap();
    if target == "mask" {
        execute(&mut session, "mask.add.black", &Value::Null).unwrap();
        execute(&mut session, "effect.add.fill", &json!({"target": "mask", "procedural": "clouds"})).unwrap();
    }
    execute(&mut session, "effect.add.levels", &json!({"target": target})).unwrap();
    let effect = session
        .doc
        .project
        .layers
        .values()
        .flat_map(|l| l.content_effects.iter().chain(l.mask.iter().flat_map(|m| m.effects.iter())))
        .filter(|e| matches!(e.kind, EffectKind::Levels(_)))
        .map(|e| e.id)
        .max_by_key(|id| id.0)
        .unwrap();
    session.target = StackTarget::Effect(effect);
    let mut harness = Harness::builder().with_size(egui::vec2(1600.0, 960.0)).with_pixels_per_point(1.0).wgpu().build_eframe(|cc| TextureCraftApp::new(cc, session));
    harness.state_mut().ui.prefs.brightness = brightness;
    harness.run_steps(8);
    (harness, effect)
}

fn edit(harness: &mut Harness<'_, TextureCraftApp>, effect: LayerId) {
    let params = json!({"effect": effect.0, "inLow": 0.18, "inHigh": 0.78, "gamma": 1.7, "outLow": 0.92, "outHigh": 0.12});
    execute(&mut harness.state_mut().session, "effect.set", &params).unwrap();
}

#[test]
#[ignore]
fn levels_editor_shots() {
    let dir = PathBuf::from(std::env::var_os("TC_UI_SNAPSHOTS").expect("TC_UI_SNAPSHOTS"));
    std::fs::create_dir_all(&dir).unwrap();
    for (brightness, theme) in [(Brightness::MediumDark, "medium"), (Brightness::Dark, "dark"), (Brightness::Light, "light")] {
        for target in ["mask", "content"] {
            let (mut h, effect) = harness(target, brightness);
            save(&mut h, &dir, &format!("{theme}-{target}-1-default"));
            edit(&mut h, effect);
            save(&mut h, &dir, &format!("{theme}-{target}-2-edited"));

            // Mid-drag: gamma held left of where it started
            let at = h.get_by_role_and_label(Role::Slider, "Gamma").rect().center();
            h.drag_at(at);
            h.run_steps(2);
            h.hover_at(at + egui::vec2(-25.0, 0.0));
            h.run_steps(2);
            save(&mut h, &dir, &format!("{theme}-{target}-3-dragging-gamma"));
            h.drop_at(at + egui::vec2(-25.0, 0.0));
            h.run_steps(3);

            // Gamma at its lowest value sits on the input-high handle
            let params = json!({"effect": effect.0, "gamma": 0.01});
            execute(&mut h.state_mut().session, "effect.set", &params).unwrap();
            save(&mut h, &dir, &format!("{theme}-{target}-4-gamma-at-input-high"));

            // Double-click the output-high handle to type its value
            let at = h.get_by_role_and_label(Role::Slider, "Output high").rect().center();
            // Both clicks in one frame: the simulated frames are longer than the double-click delay
            h.event(egui::Event::PointerMoved(at));
            for pressed in [true, false, true, false] {
                h.event(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE });
            }
            h.run_steps(1);
            save(&mut h, &dir, &format!("{theme}-{target}-5-typing-output-high"));
        }
    }
}
