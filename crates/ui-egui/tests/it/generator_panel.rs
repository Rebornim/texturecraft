//! The generator panel: global adjustments, restore defaults and image inputs edit the selected generator.

use egui::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;
use texturecraft_engine::core::{AxisBlend, BakedMap, EffectKind, Generator, GeneratorKind, LayerId, SectionParameters, Source, WearParameters};
use texturecraft_engine::{ViewMode, execute};
use texturecraft_ui_egui::TextureCraftApp;
use texturecraft_ui_egui::docking::Panel;
use texturecraft_ui_egui::theme::Brightness;

/// The sample session with a fill layer whose black mask holds a curvature generator, selected in the properties panel.
fn harness(render: bool) -> (Harness<'static, TextureCraftApp>, LayerId) {
    harness_with(render, json!({"generator": "curvature"}))
}

/// The sample session with a fill layer whose black mask holds the generator `added` describes, selected in the properties panel
fn harness_with(render: bool, added: serde_json::Value) -> (Harness<'static, TextureCraftApp>, LayerId) {
    let mut session = texturecraft_engine::sample_session(512, 256).unwrap();
    session.active_set = 1;
    session.camera.pitch = 0.5;
    let layer = execute(&mut session, "layer.add.fill", &json!({"color": [0.8, 0.3, 0.1]})).unwrap()["layer"].as_u64().unwrap();
    execute(&mut session, "mask.add.black", &json!({})).unwrap();
    let effect = execute(&mut session, "effect.add.generator", &added).unwrap()["effect"].as_u64().unwrap();
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

fn generator(harness: &Harness<'_, TextureCraftApp>, effect: LayerId) -> Generator {
    match &harness.state().session.doc.project.effect(effect).unwrap().1.kind {
        EffectKind::Generator(generator) => generator.clone(),
        _ => panic!("expected a generator"),
    }
}

/// Drags the track under slider `label` to the right.
fn drag_slider(harness: &mut Harness<'_, TextureCraftApp>, label: &str) {
    harness.get_by_label(label).scroll_to_me();
    harness.run_steps(3);
    let rect = harness.get_by_label(label).rect();
    let start = egui::pos2(rect.left() + 20.0, rect.bottom() + 10.0);
    let end = start + egui::vec2(120.0, 0.0);
    harness.drag_at(start);
    harness.run_steps(2);
    harness.hover_at(end);
    harness.run_steps(2);
    harness.drop_at(end);
    harness.run_steps(3);
}

#[test]
fn global_blur_applies_as_one_undo_step() {
    let (mut harness, effect) = harness(false);
    let history = harness.state().session.history_labels().len();
    drag_slider(&mut harness, "Global blur");
    assert!(generator(&harness, effect).global.blur > 0.0);
    assert_eq!(harness.state().session.history_labels().len(), history + 1);
    harness.state_mut().session.undo().unwrap();
    assert_eq!(generator(&harness, effect).global.blur, 0.0);
}

#[test]
fn restore_defaults_puts_the_generator_back_to_its_kind_defaults() {
    let (mut harness, effect) = harness(false);
    let edits = json!({"effect": effect.0, "level": 0.9, "blur": 0.5, "balance": 0.2, "resolutionFactor": -2, "seed": 99});
    execute(&mut harness.state_mut().session, "effect.set", &edits).unwrap();
    harness.run_steps(3);
    harness.get_by_label("Restore defaults").scroll_to_me();
    harness.run_steps(2);
    harness.get_by_label("Restore defaults").click();
    harness.run_steps(3);
    assert_eq!(generator(&harness, effect), Generator::new(GeneratorKind::Curvature));
}

#[test]
fn a_wear_control_moves_an_earlier_wear_generator_onto_the_wear_controls() {
    let (mut harness, effect) = harness_with(false, json!({"generator": "edgeWear", "level": 0.4}));
    assert_eq!(generator(&harness, effect).wear, None, "level keeps it on the earlier model");

    drag_slider(&mut harness, "Wear level");

    let wear = generator(&harness, effect).wear.expect("on the wear controls now");
    let defaults = WearParameters::defaults(GeneratorKind::EdgeWear).unwrap();
    assert_ne!(wear.level, defaults.level, "the drag set the level");
    assert_eq!(WearParameters { level: defaults.level, ..wear }, defaults, "the other controls start at their defaults");
}

#[test]
fn restore_defaults_keeps_a_wear_generator_on_its_wear_controls() {
    let (mut harness, effect) = harness_with(false, json!({"generator": "dirt", "grungeAmount": 0.9, "seed": 3}));
    harness.get_by_label("Restore defaults").scroll_to_me();
    harness.run_steps(2);
    harness.get_by_label("Restore defaults").click();
    harness.run_steps(3);
    assert_eq!(generator(&harness, effect), Generator::new(GeneratorKind::Dirt));
}

#[test]
fn an_input_can_read_a_uniform_value_and_the_mesh_map_again() {
    let (mut harness, effect) = harness(false);
    harness.get_all_by_role(Role::ComboBox).find(|node| node.value().as_deref() == Some("Mesh map: Curvature")).unwrap().scroll_to_me();
    harness.run_steps(3);
    harness.get_all_by_role(Role::ComboBox).find(|node| node.value().as_deref() == Some("Mesh map: Curvature")).unwrap().click();
    harness.run_steps(2);
    harness.get_by_label("Uniform value").click();
    harness.run_steps(3);
    assert!(matches!(generator(&harness, effect).inputs.get(&BakedMap::Curvature), Some(Source::Uniform(_))));

    harness.get_all_by_role(Role::ComboBox).find(|node| node.value().as_deref() == Some("Uniform value")).unwrap().click();
    harness.run_steps(2);
    harness.get_by_label("Mesh map").click();
    harness.run_steps(3);
    assert!(generator(&harness, effect).inputs.is_empty());
}

/// How many nodes carry `label`
fn count(harness: &Harness<'_, TextureCraftApp>, label: &str) -> usize {
    harness.query_all_by_label(label).count()
}

#[test]
fn each_kind_shows_its_section_rows_and_no_earlier_ones() {
    let rows = [
        ("curvature", &["Sharp", "Huge", "Brightness", "Global contrast"][..], &["Balance", "Blending mode", "Level"][..]),
        ("ambientOcclusion", &["Blur", "Balance", "Global contrast"][..], &["Brightness", "Blending mode", "Level"][..]),
        ("position", &["Balance", "Brightness", "Right to left", "Front to back"][..], &["Blur", "Level", "Sharp"][..]),
        ("worldSpaceNormals", &["Blur", "Balance", "Brightness", "Top to bottom"][..], &["Level", "Softness"][..]),
        (
            "direction",
            &["Horizontal angle", "Vertical angle", "Highlight glossiness", "Highlight level", "Light attenuation"][..],
            &["Global blur", "Level"][..],
        ),
    ];
    for (kind, shown, hidden) in rows {
        let (harness, _) = harness_with(false, json!({"generator": kind}));
        for label in shown {
            assert!(count(&harness, label) > 0, "{kind} shows {label}");
        }
        for label in hidden {
            assert_eq!(count(&harness, label), 0, "{kind} hides {label}");
        }
    }

    let (harness, _) = harness_with(false, json!({"generator": "worldSpaceNormals"}));
    assert_eq!(count(&harness, "Blending mode"), 3);
    for axis in ["Right to left", "Top to bottom", "Front to back"] {
        assert_eq!(count(&harness, axis), 2, "{axis}: a section heading and its weight slider");
    }
    let (harness, _) = harness_with(false, json!({"generator": "direction"}));
    assert_eq!(count(&harness, "Angle"), 2, "an Angle slider beside each dial");
    let (harness, _) = harness_with(false, json!({"generator": "position", "level": 0.4}));
    assert!(count(&harness, "Level") > 0 && count(&harness, "Balance") > 0, "the earlier model keeps its level beside the section at its defaults");
}

/// The blending mode dropdown in the Front to back section: the first dropdown below its heading, the topmost "Front to back"
fn front_to_back_dropdown<'a>(harness: &'a Harness<'_, TextureCraftApp>) -> egui_kittest::Node<'a> {
    let heading = harness.query_all_by_label("Front to back").map(|node| node.rect().top()).fold(f32::INFINITY, f32::min);
    let below = harness.get_all_by_role(Role::ComboBox).filter(|node| node.rect().top() > heading);
    below.min_by(|a, b| a.rect().top().total_cmp(&b.rect().top())).unwrap()
}

#[test]
fn each_axis_has_its_own_blending_mode_dropdown() {
    let (mut harness, effect) = harness_with(false, json!({"generator": "worldSpaceNormals"}));
    let added = |harness: &Harness<'_, TextureCraftApp>| {
        harness.get_all_by_role(Role::ComboBox).filter(|node| node.value().as_deref() == Some("Linear Dodge (Add)")).count()
    };
    assert_eq!(added(&harness), 3);
    front_to_back_dropdown(&harness).scroll_to_me();
    harness.run_steps(3);
    front_to_back_dropdown(&harness).click();
    harness.run_steps(2);
    harness.get_by_label("Multiply").click();
    harness.run_steps(3);

    let axes = generator(&harness, effect).section.unwrap().axes;
    assert_eq!(axes.map(|axis| axis.blend), [AxisBlend::Add, AxisBlend::Add, AxisBlend::Multiply], "only front to back changed");
}

#[test]
fn a_section_control_moves_an_earlier_generator_onto_the_section_controls() {
    let (mut harness, effect) = harness_with(false, json!({"generator": "position", "level": 0.4}));
    assert_eq!(generator(&harness, effect).section, None);

    drag_slider(&mut harness, "Balance");

    let section = generator(&harness, effect).section.expect("on the section controls now");
    let defaults = SectionParameters::defaults(GeneratorKind::Position).unwrap();
    assert_ne!(section.balance, defaults.balance, "the drag set the balance");
    assert_eq!(SectionParameters { balance: defaults.balance, ..section }, defaults, "the other controls start at their defaults");
    assert_eq!(count(&harness, "Level"), 0, "the earlier rows are gone");
}

#[test]
fn the_light_dial_turns_towards_the_pointer() {
    let (mut harness, effect) = harness_with(false, json!({"generator": "direction"}));
    harness.get_by_role_and_label(Role::Slider, "Horizontal angle").scroll_to_me();
    harness.run_steps(3);
    let rect = harness.get_by_role_and_label(Role::Slider, "Horizontal angle").rect();
    let left = egui::pos2(rect.left() + 2.0, rect.center().y);
    harness.drag_at(left);
    harness.run_steps(2);
    harness.drop_at(left);
    harness.run_steps(3);
    let horizontal = generator(&harness, effect).light.unwrap().horizontal;
    assert!(!(1.0..=359.0).contains(&horizontal), "0 is on the left, as in the reference: {horizontal}");
}

#[test]
fn restore_defaults_puts_a_light_back_on_its_defaults() {
    let (mut harness, effect) = harness_with(false, json!({"generator": "direction", "lightHorizontal": 10.0, "highlightLevel": 0.7, "invert": true}));
    harness.get_by_label("Restore defaults").scroll_to_me();
    harness.run_steps(2);
    harness.get_by_label("Restore defaults").click();
    harness.run_steps(3);
    assert_eq!(generator(&harness, effect), Generator::new(GeneratorKind::Direction));
}

#[test]
#[ignore = "requires a GPU adapter and TC_UI_SNAPSHOTS"]
fn generator_panel_snapshots() {
    let directory = std::path::PathBuf::from(std::env::var_os("TC_UI_SNAPSHOTS").expect("set TC_UI_SNAPSHOTS"));
    std::fs::create_dir_all(&directory).unwrap();
    let (mut harness, effect) = harness(true);
    harness.run_steps(12);
    harness.render().unwrap().save(directory.join("generator-panel-default.png")).unwrap();

    for section in ["Attributes", "Resolution"] {
        harness.get_by_label(section).click();
        harness.run_steps(3);
    }
    harness.run_steps(6);
    for (brightness, name) in [(Brightness::MediumDark, "medium"), (Brightness::Dark, "dark"), (Brightness::Light, "light")] {
        harness.state_mut().ui.prefs.brightness = brightness;
        harness.run_steps(3);
        harness.render().unwrap().save(directory.join(format!("generator-panel-sections-open-{name}.png"))).unwrap();
    }
    harness.state_mut().ui.prefs.brightness = Brightness::MediumDark;

    let edits = json!({"effect": effect.0, "blur": 0.3, "balance": 0.6, "contrast": 0.4, "resolutionFactor": -2});
    execute(&mut harness.state_mut().session, "effect.set", &edits).unwrap();
    harness.state_mut().session.update();
    harness.run_steps(12);
    harness.render().unwrap().save(directory.join("generator-panel-adjusted.png")).unwrap();

    // The effect row with its blend mode and opacity, in the layer stack
    harness.state_mut().ui.dock.focus(Panel::Layers);
    harness.run_steps(12);
    harness.render().unwrap().save(directory.join("generator-effect-row.png")).unwrap();
}

#[test]
#[ignore = "requires a GPU adapter and TC_UI_SNAPSHOTS"]
fn wear_panel_snapshots() {
    let directory = std::path::PathBuf::from(std::env::var_os("TC_UI_SNAPSHOTS").expect("set TC_UI_SNAPSHOTS"));
    std::fs::create_dir_all(&directory).unwrap();
    for kind in ["edgeWear", "dirt", "fiberGlassEdgeWear"] {
        let (mut harness, _) = harness_with(true, json!({"generator": kind}));
        harness.run_steps(12);
        harness.render().unwrap().save(directory.join(format!("wear-panel-{kind}.png"))).unwrap();
    }
}

#[test]
#[ignore = "requires a GPU adapter and TC_UI_SNAPSHOTS"]
fn section_panel_snapshots() {
    let directory = std::path::PathBuf::from(std::env::var_os("TC_UI_SNAPSHOTS").expect("set TC_UI_SNAPSHOTS"));
    std::fs::create_dir_all(&directory).unwrap();
    for kind in ["curvature", "ambientOcclusion", "position", "worldSpaceNormals", "direction"] {
        let (mut harness, _) = harness_with(true, json!({"generator": kind}));
        harness.run_steps(12);
        harness.render().unwrap().save(directory.join(format!("section-panel-{kind}.png"))).unwrap();
    }
}

#[test]
fn use_texture_shows_the_texture_rows_and_its_slot() {
    let (harness, _) = harness_with(false, json!({"generator": "position"}));
    assert_eq!(count(&harness, "Use texture"), 1);
    for label in ["Grayscale conversion", "Texture opacity", "Triplanar contrast", "Texture"] {
        assert_eq!(count(&harness, label), 0, "{label} waits for Use texture");
    }

    let (harness, _) = harness_with(false, json!({"generator": "position", "useTexture": true}));
    for label in ["Grayscale conversion", "Texture opacity", "Triplanar", "Triplanar contrast", "Scale"] {
        assert_eq!(count(&harness, label), 1, "{label}");
    }
    assert_eq!(count(&harness, "Texture"), 2, "the Texture section and the Texture slot in Image inputs");
    assert_eq!(count(&harness, "Blending mode"), 4, "the texture's beside the three axes'");

    let (harness, _) = harness_with(false, json!({"generator": "dirt"}));
    assert_eq!(count(&harness, "Use texture"), 0, "wear generators have no texture");
}

#[test]
fn the_texture_slot_reads_a_uniform_value_until_it_is_given_one() {
    let (mut harness, effect) = harness_with(false, json!({"generator": "ambientOcclusion", "useTexture": true}));
    let slot = |harness: &Harness<'_, TextureCraftApp>| {
        harness.get_all_by_role(Role::ComboBox).filter(|node| node.value().as_deref() == Some("Uniform value")).count()
    };
    assert_eq!(slot(&harness), 1, "an empty texture slot is a uniform value");
    assert_eq!(count(&harness, "Value"), 1, "with its value slider");

    harness.get_all_by_role(Role::ComboBox).find(|node| node.value().as_deref() == Some("Uniform value")).unwrap().scroll_to_me();
    harness.run_steps(3);
    harness.get_all_by_role(Role::ComboBox).find(|node| node.value().as_deref() == Some("Uniform value")).unwrap().click();
    harness.run_steps(2);
    assert_eq!(count(&harness, "Mesh map"), 0, "the texture has no mesh map to go back to");
    // The slot's heading also reads "Uniform value": the menu entry is the button
    harness.get_by_role_and_label(Role::Button, "Uniform value").click();
    harness.run_steps(3);
    let texture = generator(&harness, effect).texture.unwrap();
    assert_eq!(texture.source, Some(Source::Uniform([0.0; 3])));
}

#[test]
#[ignore = "requires a GPU adapter and TC_UI_SNAPSHOTS"]
fn texture_panel_snapshots() {
    let directory = std::path::PathBuf::from(std::env::var_os("TC_UI_SNAPSHOTS").expect("set TC_UI_SNAPSHOTS"));
    std::fs::create_dir_all(&directory).unwrap();
    for kind in ["position", "ambientOcclusion"] {
        let (mut harness, _) = harness_with(true, json!({"generator": kind, "useTexture": true, "textureOpacity": 0.6, "textureBlend": "normal"}));
        harness.get_by_label("Use texture").scroll_to_me();
        harness.run_steps(12);
        for (brightness, name) in [(Brightness::MediumDark, "medium"), (Brightness::Dark, "dark"), (Brightness::Light, "light")] {
            harness.state_mut().ui.prefs.brightness = brightness;
            harness.run_steps(3);
            harness.render().unwrap().save(directory.join(format!("texture-panel-{kind}-{name}.png"))).unwrap();
        }
    }
}
