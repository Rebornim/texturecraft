//! Scratch (not committed): f1 snapshots (R + click layer pick) of the built Layers items, mid-interaction
//! TC_UI_SNAPSHOTS=<dir> cargo test -p texturecraft-ui-egui --release --test layers_s1_capture -- --ignored

use std::path::{Path, PathBuf};

use egui::{Event, Modifiers, PointerButton, Pos2, Rect, pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable, by};
use serde_json::{Value, json};
use texturecraft_engine::execute;
use texturecraft_engine::core::LayerKind;
use texturecraft_ui_egui::TextureCraftApp;
use texturecraft_ui_egui::docking::Panel;
use texturecraft_ui_egui::theme::Brightness;

type H<'a> = Harness<'a, TextureCraftApp>;

fn save(h: &mut H<'_>, dir: &Path, name: &str) {
    h.run_steps(8);
    match h.render() {
        Ok(img) => {
            img.save(dir.join(format!("{name}.png"))).unwrap();
            let mut out = String::new();
            for n in h.query_all(by()) {
                if let Some(l) = n.accesskit_node().label() {
                    let r = n.rect();
                    out.push_str(&format!("{:?}\t{}\t{:.1} {:.1} {:.1} {:.1}\n", n.accesskit_node().role(), l, r.left(), r.top(), r.right(), r.bottom()));
                }
            }
            std::fs::write(dir.join(format!("{name}.rects")), out).unwrap();
            eprintln!("wrote {name}");
        }
        Err(e) => eprintln!("MISS {name}: {e}"),
    }
}

fn hover(h: &mut H<'_>, pos: Pos2) {
    h.event(Event::PointerMoved(pos));
    h.run_steps(3);
}

fn click_at(h: &mut H<'_>, pos: Pos2, button: PointerButton, modifiers: Modifiers) {
    h.event(Event::PointerMoved(pos));
    h.run_steps(1);
    h.event(Event::PointerButton { pos, button, pressed: true, modifiers });
    h.run_steps(1);
    h.event(Event::PointerButton { pos, button, pressed: false, modifiers });
    h.run_steps(4);
}

fn escape(h: &mut H<'_>) {
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
}

fn find(h: &H<'_>, role: egui::Role, label: &str) -> Option<Rect> {
    h.query_all(by().role(role)).filter(|n| n.accesskit_node().label().is_some_and(|l| l == label || l.starts_with(&format!("{label} ")))).map(|n| n.rect()).next()
}

fn scene() -> texturecraft_engine::Session {
    let mut s = texturecraft_engine::sample_session(256, 64).unwrap();
    let rough = execute(&mut s, "layer.add.fill", &json!({"color": "#808080"})).unwrap()["layer"].clone();
    execute(&mut s, "layer.rename", &json!({"layer": rough, "name": "Roughness Details"})).unwrap();
    execute(&mut s, "mask.add.white", &json!({"layer": rough})).unwrap();
    let dirt = execute(&mut s, "layer.add.fill", &json!({"color": "#3a2a1a"})).unwrap()["layer"].clone();
    execute(&mut s, "layer.rename", &json!({"layer": dirt, "name": "Cavity Dirt"})).unwrap();
    execute(&mut s, "effect.add.generator", &json!({"layer": dirt, "generator": "dirt"})).unwrap();
    execute(&mut s, "effect.add.filter", &json!({"layer": dirt, "target": "content", "filter": "blur"})).ok();
    execute(&mut s, "effect.add.filter", &json!({"layer": dirt, "target": "content", "filter": "invert"})).ok();
    execute(&mut s, "layer.group", &json!({"layer": dirt})).unwrap();
    let folder = s.selected_layer().map(|l| l.0);
    execute(&mut s, "layer.rename", &json!({"layer": folder, "name": "Jade"})).ok();
    let paint = execute(&mut s, "layer.add.paint", &Value::Null).unwrap()["layer"].clone();
    execute(&mut s, "layer.rename", &json!({"layer": paint, "name": "Scratches"})).unwrap();
    execute(&mut s, "layer.select", &json!({"layer": dirt})).unwrap();

    s
}

fn harness(brightness: Brightness) -> H<'static> {
    let mut h = Harness::builder().with_size(egui::vec2(1600.0, 1900.0)).with_pixels_per_point(2.0).wgpu().build_eframe(|cc| TextureCraftApp::new(cc, scene()));
    h.state_mut().ui.prefs.language = "en".into();
    h.state_mut().ui.prefs.brightness = brightness;
    h.run_steps(6);
    h.state_mut().ui.dock.focus(Panel::Layers);
    h.state_mut().ui.dock.focus(Panel::Properties);
    h.run_steps(30);
    h
}

fn states(h: &mut H<'_>, dir: &Path, theme: &str) {
    let name = |n: &str| format!("{theme}-{n}");
    save(h, dir, &name("01-default"));

    // Collapse the folder holding Cavity Dirt, then hold R over the mesh and click three times
    let jade = h.state().session.doc.project.layers.values().find(|l| l.name == "Jade").map(|l| l.id);
    if let Some(LayerKind::Group { collapsed, .. }) = jade.and_then(|id| h.state_mut().session.doc.project.layers.get_mut(&id)).map(|l| &mut l.kind) {
        *collapsed = true;
    }
    h.run_steps(5);
    let spot = pos2(560.0, 520.0);
    let key = |pressed| Event::Key { key: egui::Key::R, physical_key: None, pressed, repeat: false, modifiers: Modifiers::NONE };
    h.event(key(true));
    hover(h, spot);
    save(h, dir, &name("02-r-held"));
    for i in 0..3 {
        click_at(h, spot, PointerButton::Primary, Modifiers::NONE);
        h.event(key(true));
        hover(h, spot);
        h.run_steps(10);
        let selected = h.state().session.selected_layer().and_then(|l| h.state().session.doc.project.layers.get(&l)).map(|l| (l.id.0, l.name.clone()));
        eprintln!("pick {i}: {selected:?} steps {}", h.state().session.history_labels().len());
        save(h, dir, &name(&format!("03-picked-{i}")));
    }
    h.event(key(false));
    h.state_mut().ui.windows.shortcuts = true;
    h.run_steps(10);
    save(h, dir, &name("04-shortcuts"));
}

#[test]
#[ignore]
fn layers_f1_capture() {
    let Some(dir) = std::env::var_os("TC_UI_SNAPSHOTS").map(PathBuf::from) else { return };
    std::fs::create_dir_all(&dir).unwrap();
    for (b, theme) in [(Brightness::MediumDark, "medium"), (Brightness::Dark, "dark"), (Brightness::Light, "light")] {
        let mut h = harness(b);
        states(&mut h, &dir, theme);
    }
}
