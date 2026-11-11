//! The pattern panel and the fill source picker: a mask fill picks the Tile Generator and its settings edit it

use egui::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;
use texturecraft_engine::core::{EffectKind, LayerId, Procedural, ProceduralKind, Source, TileGenerator};
use texturecraft_engine::{ViewMode, execute};
use texturecraft_ui_egui::TextureCraftApp;
use texturecraft_ui_egui::docking::Panel;
use texturecraft_ui_egui::theme::Brightness;

/// The sample session with an anchor point on a fill layer, and above it a fill layer whose black mask holds a uniform mask fill, selected
fn harness(render: bool) -> (Harness<'static, TextureCraftApp>, LayerId) {
    let mut session = texturecraft_engine::sample_session(512, 256).unwrap();
    session.active_set = 1;
    session.camera.pitch = 0.5;
    execute(&mut session, "layer.add.fill", &json!({"height": 0.6})).unwrap();
    execute(&mut session, "effect.add.anchor", &json!({"name": "Height"})).unwrap();
    let layer = execute(&mut session, "layer.add.fill", &json!({"color": [0.8, 0.3, 0.1]})).unwrap()["layer"].as_u64().unwrap();
    execute(&mut session, "mask.add.black", &json!({})).unwrap();
    let effect = execute(&mut session, "effect.add.fill", &json!({"value": 1.0})).unwrap()["effect"].as_u64().unwrap();
    session.view = ViewMode::Mask(LayerId(layer));
    session.update();
    let builder = Harness::builder().with_size(egui::vec2(1600.0, 2400.0));
    let builder = if render { builder.wgpu() } else { builder };
    let mut harness = builder.build_eframe(|context| TextureCraftApp::new(context, session));
    harness.state_mut().ui.dock.close(Panel::Layers);
    harness.state_mut().ui.dock.close(Panel::TextureSetSettings);
    harness.state_mut().ui.dock.focus(Panel::Properties);
    harness.run_steps(6);
    (harness, LayerId(effect))
}

fn source(harness: &Harness<'_, TextureCraftApp>, effect: LayerId) -> Source {
    match &harness.state().session.doc.project.effect(effect).unwrap().1.kind {
        EffectKind::Fill { source, .. } => source.clone(),
        _ => panic!("expected a mask fill"),
    }
}

fn pattern(harness: &Harness<'_, TextureCraftApp>, effect: LayerId) -> Procedural {
    match source(harness, effect) {
        Source::Procedural { pattern, .. } => pattern,
        other => panic!("expected a pattern, the fill reads {other:?}"),
    }
}

/// Opens the mask fill's source picker from its Grayscale slot and clicks the tile labelled `tile`
fn pick(harness: &mut Harness<'_, TextureCraftApp>, tile: &str) {
    harness.get_by_role_and_label(Role::ComboBox, "Grayscale").click();
    harness.run_steps(3);
    harness.get_by_label(tile).click();
    harness.run_steps(3);
}

/// Where a slider's track is grabbed, from its label's left end and bottom, and how far it is dragged (points)
const TRACK_GRAB_INSET: f32 = 20.0;
const TRACK_BELOW_LABEL: f32 = 10.0;
const DRAG_DISTANCE: f32 = 120.0;

/// Runs frames until the thumbnails rendered off the UI thread arrive
fn settle(harness: &mut Harness<'_, TextureCraftApp>) {
    for _ in 0..40 {
        harness.run_steps(3);
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// Drags the track under slider `label` to the right
fn drag_slider(harness: &mut Harness<'_, TextureCraftApp>, label: &str) {
    harness.get_by_label(label).scroll_to_me();
    harness.run_steps(3);
    // The track runs under the label: grab it near its left end and drag well to the right
    let rect = harness.get_by_label(label).rect();
    let start = egui::pos2(rect.left() + TRACK_GRAB_INSET, rect.bottom() + TRACK_BELOW_LABEL);
    let end = start + egui::vec2(DRAG_DISTANCE, 0.0);
    harness.drag_at(start);
    harness.run_steps(2);
    harness.hover_at(end);
    harness.run_steps(2);
    harness.drop_at(end);
    harness.run_steps(3);
}

#[test]
fn the_picker_sets_a_mask_fill_to_the_tile_generator_and_back_to_a_value() {
    let (mut harness, effect) = harness(false);
    pick(&mut harness, "Tile Generator");
    assert_eq!(pattern(&harness, effect).kind, ProceduralKind::TileGenerator);

    pick(&mut harness, "Uniform value");
    assert!(matches!(source(&harness, effect), Source::Uniform(_)));
}

#[test]
fn the_anchor_points_tab_picks_an_anchor() {
    let (mut harness, effect) = harness(false);
    harness.get_by_role_and_label(Role::ComboBox, "Grayscale").click();
    harness.run_steps(3);
    harness.get_by_label("Anchor points").click();
    harness.run_steps(3);
    // An anchor tile is labelled "<layer> / <anchor>", as the anchors below a layer are listed
    let project = &harness.state().session.doc.project;
    let owner = project.layers.values().find(|layer| layer.content_effects.iter().any(|effect| effect.name == "Height")).unwrap();
    let label = format!("{} / Height", owner.name);
    harness.get_by_role_and_label(Role::Button, &label).click();
    harness.run_steps(3);
    assert!(source(&harness, effect).anchor().is_some());
}

#[test]
fn an_amount_drag_is_one_undo_step_and_restore_defaults_puts_the_tiles_back() {
    let (mut harness, effect) = harness(false);
    execute(&mut harness.state_mut().session, "effect.set", &json!({"effect": effect.0, "procedural": "Tile Generator"})).unwrap();
    harness.run_steps(3);
    let history = harness.state().session.history_labels().len();

    drag_slider(&mut harness, "X amount");
    let tile = pattern(&harness, effect).tile.unwrap();
    assert!(tile.amount[0] > TileGenerator::default().amount[0], "the drag adds columns: {:?}", tile.amount);
    assert_eq!(harness.state().session.history_labels().len(), history + 1, "one drag is one undo step");

    harness.get_by_label("Restore defaults").scroll_to_me();
    harness.run_steps(2);
    harness.get_by_label("Restore defaults").click();
    harness.run_steps(3);
    assert_eq!(*pattern(&harness, effect).tile.unwrap(), TileGenerator::default());
}

#[test]
#[ignore = "requires a GPU adapter and TC_UI_SNAPSHOTS"]
fn tile_generator_snapshots() {
    let directory = std::path::PathBuf::from(std::env::var_os("TC_UI_SNAPSHOTS").expect("set TC_UI_SNAPSHOTS"));
    std::fs::create_dir_all(&directory).unwrap();
    let (mut harness, effect) = harness(true);

    // The picker over the slot, both tabs
    harness.get_by_role_and_label(Role::ComboBox, "Grayscale").click();
    settle(&mut harness);
    harness.render().unwrap().save(directory.join("picker-resources.png")).unwrap();
    harness.get_by_label("Anchor points").click();
    settle(&mut harness);
    harness.render().unwrap().save(directory.join("picker-anchors.png")).unwrap();
    harness.key_press(egui::Key::Escape);
    harness.run_steps(3);

    execute(&mut harness.state_mut().session, "effect.set", &json!({"effect": effect.0, "procedural": "Tile Generator"})).unwrap();
    harness.state_mut().session.update();
    harness.run_steps(12);
    for (brightness, name) in [(Brightness::MediumDark, "medium"), (Brightness::Dark, "dark"), (Brightness::Light, "light")] {
        harness.state_mut().ui.prefs.brightness = brightness;
        harness.run_steps(6);
        harness.render().unwrap().save(directory.join(format!("tile-panel-{name}.png"))).unwrap();
    }
    harness.state_mut().ui.prefs.brightness = Brightness::Dark;

    for (sections, name) in [(["Pattern", "Size"], "pattern-size"), (["Position", "Rotation"], "position-rotation")] {
        for section in sections {
            // A section's title can also be a control's label above it ("Position"): the title is the last one
            harness.get_all_by_label(section).last().unwrap().scroll_to_me();
            // The scroll animates: the title must be on screen before the click
            harness.run_steps(12);
            harness.get_all_by_label(section).last().unwrap().click();
            harness.run_steps(3);
        }
        harness.run_steps(6);
        harness.render().unwrap().save(directory.join(format!("tile-sections-{name}.png"))).unwrap();
    }
    harness.get_all_by_label("Color").last().unwrap().scroll_to_me();
    harness.run_steps(12);
    harness.get_all_by_label("Color").last().unwrap().click();
    harness.run_steps(6);
    harness.render().unwrap().save(directory.join("tile-sections-color.png")).unwrap();

    // A changed pattern, in the 2D view
    let edits = json!({"effect": effect.0, "tile": {"shape": "Disc", "amount": [8, 4], "offset": 0.5, "scaleRandom": 0.6, "luminanceRandom": 0.5}});
    execute(&mut harness.state_mut().session, "effect.set", &edits).unwrap();
    harness.state_mut().session.update();
    settle(&mut harness);
    harness.render().unwrap().save(directory.join("tile-adjusted.png")).unwrap();
}
