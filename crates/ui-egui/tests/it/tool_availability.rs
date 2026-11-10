//! The shelf greys tools that can't act on the selection; the top bar and Properties follow it

use egui_kittest::Harness;
use serde_json::{Value, json};
use texturecraft_engine::{Session, StackTarget, Tool, execute};
use texturecraft_ui_egui::TextureCraftApp;
use texturecraft_ui_egui::state::Mode;
use texturecraft_ui_egui::theme::Brightness;

fn add_layer(session: &mut Session, command: &str) -> u64 {
    execute(session, command, &json!({})).unwrap()["layer"].as_u64().unwrap()
}

fn select(session: &mut Session, layer: u64, target: Value) {
    execute(session, "layer.select", &json!({"layer": layer, "target": target})).unwrap();
}

fn select_nothing(session: &mut Session) {
    let set = session.ts();
    session.selected.remove(&set);
    session.target = StackTarget::Content;
}

#[test]
#[ignore = "offscreen GPU snapshots; set TC_UI_SNAPSHOTS to an output directory"]
fn tool_availability_snapshots() {
    let Some(directory) = std::env::var_os("TC_UI_SNAPSHOTS").map(std::path::PathBuf::from) else {
        return;
    };

    std::fs::create_dir_all(&directory).unwrap();
    let session = texturecraft_engine::sample_session(256, 64).unwrap();
    let mut harness = Harness::builder().with_size(egui::vec2(1600.0, 960.0)).wgpu().build_eframe(|context| TextureCraftApp::new(context, session));
    let save = |harness: &mut Harness<'_, TextureCraftApp>, name: &str| {
        for (brightness, suffix) in [(Brightness::MediumDark, "medium"), (Brightness::Dark, "dark"), (Brightness::Light, "light")] {
            harness.state_mut().ui.prefs.brightness = brightness;
            harness.run_steps(8);
            harness.render().unwrap().save(directory.join(format!("{name}-{suffix}.png"))).unwrap();
        }
    };

    let session = &mut harness.state_mut().session;
    let paint = add_layer(session, "layer.add.paint");
    select(session, paint, json!("content"));
    session.tool.tool = Tool::Paint;
    save(&mut harness, "01-paint-layer");

    let session = &mut harness.state_mut().session;
    let fill = add_layer(session, "layer.add.fill");
    select(session, fill, json!("content"));
    session.tool.tool = Tool::PolygonFill;
    save(&mut harness, "04-fill-content-polygon-fill");

    let session = &mut harness.state_mut().session;
    execute(session, "effect.add.filter", &json!({"layer": paint, "target": "content", "filter": "blur"})).unwrap();
    let filter = session.doc.project.layer(texturecraft_engine::core::LayerId(paint)).unwrap().content_effects.last().unwrap().id.0;
    select(session, paint, json!(filter));
    session.tool.tool = Tool::Paint;
    save(&mut harness, "07-filter");

    let session = &mut harness.state_mut().session;
    execute(session, "mask.add.black", &json!({"layer": paint})).unwrap();
    select(session, paint, json!("mask"));
    session.tool.tool = Tool::Smudge;
    save(&mut harness, "05-mask-smudge");

    let session = &mut harness.state_mut().session;
    let folder = add_layer(session, "layer.add.group");
    select(session, folder, json!("content"));
    session.tool.tool = Tool::Paint;
    save(&mut harness, "09-folder");

    select_nothing(&mut harness.state_mut().session);
    save(&mut harness, "12-nothing-selected");

    harness.state_mut().ui.mode = Mode::Baking;
    save(&mut harness, "15-bake");

    harness.state_mut().ui.mode = Mode::Rendering;
    save(&mut harness, "16-render");
}
