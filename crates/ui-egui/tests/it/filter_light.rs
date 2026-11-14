//! The light and shadow filters in Properties: Glow's Used on a Mask, Mode and signed Width, Drop Shadow's Angle and Distance, the Baked Lighting cards and rows

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};
use texturecraft_engine::core::{EffectKind, Filter, LayerId, LightingOutput};
use texturecraft_engine::{StackTarget, execute};
use texturecraft_ui_egui::TextureCraftApp;

/// The app with library filter `filter` in a fill layer's mask, selected so Properties shows it
fn app_with(filter: &str) -> (Harness<'static, TextureCraftApp>, LayerId) {
    let mut session = texturecraft_engine::sample_session(256, 64).unwrap();
    let layer = execute(&mut session, "layer.add.fill", &Value::Null).unwrap();
    execute(&mut session, "mask.add.white", &json!({"layer": layer["layer"]})).unwrap();
    let effect = execute(&mut session, "effect.add.filter", &json!({"target": "mask", "filter": filter})).unwrap();
    let effect = LayerId(effect["effect"].as_u64().unwrap());
    execute(&mut session, "layer.select", &json!({"layer": layer["layer"]})).unwrap();
    session.target = StackTarget::Effect(effect);

    let mut harness = Harness::builder().with_size(egui::vec2(1600.0, 1400.0)).build_eframe(|cc| TextureCraftApp::new(cc, session));
    harness.state_mut().ui.prefs.language = "en".into();
    harness.run_steps(4);
    (harness, effect)
}

/// A press and release of the primary button at `pos`, as the pointer does it
fn click_at(harness: &mut Harness<'_, TextureCraftApp>, pos: egui::Pos2) {
    let modifiers = egui::Modifiers::NONE;
    harness.event(egui::Event::PointerMoved(pos));
    harness.event(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: true, modifiers });
    harness.run_steps(1);
    harness.event(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: false, modifiers });
    harness.run_steps(3);
}

fn filter_of(harness: &Harness<'_, TextureCraftApp>, effect: LayerId) -> Filter {
    match &harness.state().session.doc.project.effect(effect).unwrap().1.kind {
        EffectKind::Filter(filter) => filter.clone(),
        other => panic!("not a filter: {other:?}"),
    }
}

/// Clicks on Used on a Mask, Precise and the left end of Width turn the glow into a precise inner glow laid as white on the mask
#[test]
fn glow_rows_set_used_on_a_mask_precise_and_an_inner_width() {
    let (mut harness, effect) = app_with("glow");
    let toggle = harness.get_by_role_and_label(Role::CheckBox, "Used on a Mask").rect();
    click_at(&mut harness, toggle.center());
    let precise = harness.get_by_role_and_label(Role::RadioButton, "Precise").rect();
    click_at(&mut harness, precise.center());
    let width = harness.get_by_role_and_label(Role::Slider, "Width").rect();
    click_at(&mut harness, egui::pos2(width.left() + 1.0, width.center().y));
    let Filter::HaloGlow { on_mask, precise, width, .. } = filter_of(&harness, effect) else { panic!("{:?}", filter_of(&harness, effect)) };
    assert!(on_mask && precise, "{on_mask} {precise}");
    assert!(width < -0.9, "inner glow: {width}");
}

/// Drop Shadow's Distance reaches both ways and its Angle row carries a dial
#[test]
fn drop_shadow_distance_goes_both_ways() {
    let (mut harness, effect) = app_with("dropShadow");
    let distance = harness.get_by_role_and_label(Role::Slider, "Distance").rect();
    click_at(&mut harness, egui::pos2(distance.left() + 1.0, distance.center().y));
    let Filter::DropShadow { distance, angle, .. } = filter_of(&harness, effect) else { panic!() };
    assert!(distance < -0.45, "towards the light: {distance}");
    assert_eq!(angle, 90.0);
    assert!(harness.query_all_by_label("Angle").next().is_some());
}

/// The app with lighting filter `filter` on a fill layer's content, selected so Properties shows it
pub(crate) fn app_with_lighting(filter: &str) -> (Harness<'static, TextureCraftApp>, LayerId) {
    let mut session = texturecraft_engine::sample_session(256, 64).unwrap();
    let layer = execute(&mut session, "layer.add.fill", &Value::Null).unwrap();
    let effect = execute(&mut session, "effect.add.filter", &json!({"target": "content", "filter": filter})).unwrap();
    let effect = LayerId(effect["effect"].as_u64().unwrap());
    execute(&mut session, "layer.select", &json!({"layer": layer["layer"]})).unwrap();
    session.target = StackTarget::Effect(effect);

    let mut harness = Harness::builder().with_size(egui::vec2(1600.0, 2400.0)).build_eframe(|cc| TextureCraftApp::new(cc, session));
    harness.state_mut().ui.prefs.language = "en".into();
    harness.run_steps(4);
    (harness, effect)
}

/// Baked Lighting Stylized's cards edit their own settings: Sun Intensity's right end, Light 1's Intensity and Output's Specular only
#[test]
fn baked_lighting_stylized_cards_set_the_sun_a_light_and_the_output() {
    let (mut harness, effect) = app_with_lighting("bakedLighting");
    let sun = harness.get_by_role_and_label(Role::Slider, "Sun Intensity").rect();
    click_at(&mut harness, egui::pos2(sun.right() - 1.0, sun.center().y));
    // Light 1's card comes before Light 2's
    let light_1 = harness.query_all_by_role_and_label(Role::Slider, "Intensity").map(|node| node.rect()).min_by(|a, b| a.top().total_cmp(&b.top())).unwrap();
    click_at(&mut harness, egui::pos2(light_1.right() - 1.0, light_1.center().y));
    let output = harness.get_by_role_and_label(Role::ComboBox, "Diffuse + Specular").rect();
    click_at(&mut harness, output.center());
    let specular = harness.get_by_label("Specular only").rect();
    click_at(&mut harness, specular.center());

    let filter = filter_of(&harness, effect);
    let (stylized, light) = filter.stylized_lighting().unwrap();
    assert!(light.sun.intensity > 9.5, "{}", light.sun.intensity);
    assert!(stylized.lights[0].intensity > 1.9 && stylized.lights[1].intensity == 0.0, "{:?}", stylized.lights);
    assert_eq!(stylized.output, LightingOutput::Specular);
}

/// Baked Lighting Environment's rows: Horizontal Rotation with its dial, a signed Exposure, and the stacked Specular Occlusion Intensity under its label
#[test]
fn baked_lighting_environment_rows_set_rotation_exposure_and_occlusion() {
    let (mut harness, effect) = app_with_lighting("bakedLightingEnvironment");
    let rotation = harness.get_by_role_and_label(Role::Slider, "Horizontal Rotation").rect();
    click_at(&mut harness, egui::pos2(rotation.center().x, rotation.center().y));
    let exposure = harness.get_by_role_and_label(Role::Slider, "Exposure").rect();
    click_at(&mut harness, egui::pos2(exposure.left() + 1.0, exposure.center().y));
    let specular = harness.get_by_role_and_label(Role::Slider, "Specular Occlusion Intensity").rect();
    click_at(&mut harness, egui::pos2(specular.right() - 1.0, specular.center().y));
    let Filter::EnvironmentLighting { horizontal, exposure, specular_occlusion, ambient_occlusion, .. } = filter_of(&harness, effect) else {
        panic!("{:?}", filter_of(&harness, effect))
    };

    assert!((150.0..210.0).contains(&horizontal), "half way round: {horizontal}");
    assert!(exposure < -4.5, "darker: {exposure}");
    assert!(specular_occlusion > 0.95 && ambient_occlusion == 1.0, "{specular_occlusion} {ambient_occlusion}");
}
