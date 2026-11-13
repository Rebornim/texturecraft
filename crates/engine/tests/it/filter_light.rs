//! The light and shadow filters: Glow (a colored halo around the shapes) and Drop Shadow

use serde_json::json;
use texturecraft_engine::core::{Channel, DirectionalLight, EffectKind, Filter, LayerId, LightingOutput};
use texturecraft_engine::geom::primitives;
use texturecraft_engine::{Session, execute};

fn session() -> Session {
    Session::new_project("t", primitives::plane(2.0, 2), "builtin:plane", None, 128).unwrap()
}

/// A fill layer with library filter `id` in its mask: the effect
fn masked(s: &mut Session, id: &str) -> LayerId {
    execute(s, "layer.add.fill", &json!({})).unwrap();
    execute(s, "mask.add.black", &json!({})).unwrap();
    LayerId(execute(s, "effect.add.filter", &json!({"filter": id, "target": "mask"})).unwrap()["effect"].as_u64().unwrap())
}

fn filter_of(s: &Session, effect: LayerId) -> Filter {
    match &s.doc.project.effect(effect).unwrap().1.kind {
        EffectKind::Filter(filter) => filter.clone(),
        other => panic!("not a filter: {other:?}"),
    }
}

/// Projects saved before keep their additive glow, read and saved as they were; a new Glow is the halo, named the same
#[test]
fn a_glow_saved_before_stays_additive_and_a_new_glow_is_the_halo() {
    let saved = json!({"Glow": {"radius": 16.0, "amount": 1.0}});
    let filter: Filter = serde_json::from_value(saved.clone()).unwrap();
    assert_eq!(filter, Filter::Glow { radius: 16.0, amount: 1.0 });
    assert_eq!(serde_json::to_value(&filter).unwrap(), saved);

    let mut s = session();
    let effect = masked(&mut s, "glow");
    let Filter::HaloGlow { on_mask, precise, width, spread, opacity, .. } = filter_of(&s, effect) else { panic!("{:?}", filter_of(&s, effect)) };
    assert_eq!((on_mask, precise, width, spread, opacity), (false, false, 0.15, 0.0, 1.0), "the workflow's defaults");
    assert_eq!(filter_of(&s, effect).label(), "Glow");
    assert_eq!(filter.label(), "Glow");
}

/// Glow's Used on a Mask, Mode, Width (inner glow below 0), Spread, Opacity and Color set and clamp; bad values are refused
#[test]
fn glow_settings_set_clamp_and_refuse() {
    let mut s = session();
    let effect = masked(&mut s, "glow");
    let set = json!({"effect": effect.0, "usedOnMask": true, "mode": "precise", "width": -3.0, "spread": 0.4, "opacity": 0.6, "color": [0.2, 0.4, 0.6]});
    execute(&mut s, "effect.set", &set).unwrap();
    assert_eq!(filter_of(&s, effect), Filter::HaloGlow { on_mask: true, precise: true, width: -1.0, spread: 0.4, opacity: 0.6, color: [0.2, 0.4, 0.6] });
    for bad in [json!({"mode": "hard"}), json!({"usedOnMask": 1}), json!({"color": "red"})] {
        let mut params = bad.clone();
        params["effect"] = json!(effect.0);
        assert!(execute(&mut s, "effect.set", &params).is_err(), "{bad}");
    }
    assert_eq!(filter_of(&s, effect), Filter::HaloGlow { on_mask: true, precise: true, width: -1.0, spread: 0.4, opacity: 0.6, color: [0.2, 0.4, 0.6] });
    assert_eq!(execute(&mut s, "effect.filter.restoreDefaults", &json!({"effect": effect.0})).unwrap()["changed"], json!(true));
    assert!(matches!(filter_of(&s, effect), Filter::HaloGlow { width: 0.15, .. }));
}

/// Drop Shadow: the workflow's defaults, then Angle wrapping, Distance both ways, Size, Spread, Opacity and Color
#[test]
fn drop_shadow_defaults_and_settings() {
    let mut s = session();
    let effect = masked(&mut s, "Drop Shadow");
    assert_eq!(filter_of(&s, effect), Filter::DropShadow { angle: 90.0, distance: 0.1, size: 0.2, spread: 0.0, opacity: 0.5, color: [0.0; 3] });
    let set = json!({"effect": effect.0, "angle": 450.0, "distance": -0.9, "size": 2.0, "spread": 0.3, "opacity": 0.25, "color": "#ffffff"});
    execute(&mut s, "effect.set", &set).unwrap();
    assert_eq!(filter_of(&s, effect), Filter::DropShadow { angle: 90.0, distance: -0.5, size: 1.0, spread: 0.3, opacity: 0.25, color: [1.0; 3] });
    assert!(execute(&mut s, "effect.set", &json!({"effect": effect.0, "color": [1.0]})).is_err());
    let listed = execute(&mut s, "filter.list", &json!({})).unwrap();
    assert!(listed.as_array().unwrap().iter().any(|f| f["id"] == "dropShadow" && f["mask"] == true));
}

/// A content fill layer with library filter `id` on its content: the effect
fn on_content(s: &mut Session, id: &str) -> LayerId {
    execute(s, "layer.add.fill", &json!({})).unwrap();
    LayerId(execute(s, "effect.add.filter", &json!({"target": "content", "filter": id})).unwrap()["effect"].as_u64().unwrap())
}

/// Projects saved before keep their Baked Lighting, read and saved as they were and named Baked Lighting Stylized; a new one takes the workflow's defaults
#[test]
fn a_baked_lighting_saved_before_stays_and_a_new_one_is_stylized() {
    let saved = json!({"BakedLighting": {"azimuth": 45.0, "elevation": 45.0, "light": 0.75, "ambient": 0.5, "occlusion": 1.0}});
    let filter: Filter = serde_json::from_value(saved.clone()).unwrap();
    assert!(filter.stylized_lighting().is_none(), "lights as before");
    assert_eq!(serde_json::to_value(&filter).unwrap(), saved);
    assert_eq!(filter.label(), "Baked Lighting Stylized");
    assert_eq!(filter.former_label(), Some("Baked Lighting"));

    let mut s = session();
    let effect = on_content(&mut s, "Baked Lighting");
    let made = filter_of(&s, effect);
    let Filter::BakedLighting { azimuth, elevation, light, ambient, occlusion, .. } = made.clone() else { panic!("{made:?}") };
    assert_eq!((azimuth, elevation, light, ambient, occlusion), (0.0, 45.0, 1.0, 1.0, 0.0), "the workflow's sun and sky");
    let (stylized, _) = made.stylized_lighting().unwrap();
    assert_eq!(
        (stylized.output, stylized.mesh_only, stylized.dielectric_reflectance, stylized.cavity_smoothness),
        (LightingOutput::DiffuseSpecular, false, 0.04, 0.25)
    );
    assert_eq!((stylized.lights[0].azimuth, stylized.lights[1].azimuth, stylized.lights[0].intensity), (90.0, 180.0, 0.0));
    let listed = execute(&mut s, "filter.list", &json!({})).unwrap();
    assert!(listed.as_array().unwrap().iter().any(|f| f["id"] == "bakedLighting" && f["mask"] == false && f["label"] == "Baked Lighting Stylized"));
}

/// The stylized settings set and clamp; one set on a Baked Lighting saved before gives it the rest lit as it was; bad choices are refused
#[test]
fn stylized_lighting_settings_set_clamp_and_refuse() {
    let mut s = session();
    let effect = on_content(&mut s, "bakedLighting");
    let old = Filter::BakedLighting { azimuth: 45.0, elevation: 45.0, light: 0.8, ambient: 0.4, occlusion: 1.0, stylized: None };
    s.doc.project.effect_mut(effect).unwrap().kind = EffectKind::Filter(old);
    let set = json!({
        "effect": effect.0, "skyColor": [0.1, 0.2, 0.3], "light1": {"azimuth": 450.0, "elevation": -30.0, "intensity": 5.0, "color": "#ff0000"},
        "diffuseCavity": 0.5, "cavitySmoothness": 9.0, "dielectricReflectance": 1.0, "heightToNormal": 2.0,
        "input": "specularGlossiness", "output": "specular", "normalDetails": "meshHeightNormal"
    });
    execute(&mut s, "effect.set", &set).unwrap();
    let filter = filter_of(&s, effect);
    let (stylized, light) = filter.stylized_lighting().unwrap();
    assert_eq!((light.sun.intensity, light.sky_intensity, stylized.sun_color), (0.8, 0.4, [1.0; 3]), "the rest as it lit");
    assert_eq!(stylized.sky_color, [0.1, 0.2, 0.3]);
    assert_eq!(stylized.lights[0], DirectionalLight { azimuth: 90.0, elevation: 330.0, intensity: 2.0, color: [1.0, 0.0, 0.0] });
    assert_eq!((stylized.diffuse_cavity, stylized.cavity_smoothness, stylized.dielectric_reflectance, stylized.height_to_normal), (0.5, 4.0, 0.16, 2.0));
    assert!(stylized.specular_glossiness && !stylized.mesh_only && stylized.output == LightingOutput::Specular);
    for bad in [json!({"output": "both"}), json!({"input": 1}), json!({"normalDetails": "height"}), json!({"light2": 3}), json!({"groundColor": "dirt"})] {
        let mut params = bad.clone();
        params["effect"] = json!(effect.0);
        assert!(execute(&mut s, "effect.set", &params).is_err(), "{bad}");
    }
    assert_eq!(filter_of(&s, effect), filter, "refusals change nothing");
    assert_eq!(execute(&mut s, "effect.filter.restoreDefaults", &json!({"effect": effect.0})).unwrap()["changed"], json!(true));
    assert_eq!(filter_of(&s, effect).stylized_lighting().unwrap().0.output, LightingOutput::DiffuseSpecular);
}

/// Baked Lighting Environment: the workflow's defaults, only the Color pill lit, rotations wrapping and the rest clamped, Restore defaults
#[test]
fn environment_lighting_defaults_and_settings() {
    let mut s = session();
    let effect = on_content(&mut s, "bakedLightingEnvironment");
    let made = filter_of(&s, effect);
    assert_eq!(
        made,
        Filter::EnvironmentLighting { horizontal: 0.0, vertical: 0.0, exposure: 0.0, height_intensity: 4.0, ambient_occlusion: 1.0, specular_occlusion: 0.0 }
    );
    assert_eq!((made.label(), s.doc.project.effect(effect).unwrap().1.name.as_str()), ("Baked Lighting Environment", "Baked Lighting Environment"));
    let off = &s.doc.project.effect(effect).unwrap().1.off_channels;
    assert!(!off.contains(&Channel::BaseColor) && off.contains(&Channel::Roughness) && off.contains(&Channel::Height), "only the Color pill is lit: {off:?}");
    let stylized = on_content(&mut s, "bakedLighting");
    assert!(s.doc.project.effect(stylized).unwrap().1.off_channels.contains(&Channel::Metallic), "Baked Lighting Stylized lights the Color pill only too");
    let blur = on_content(&mut s, "blur");
    assert!(s.doc.project.effect(blur).unwrap().1.off_channels.is_empty(), "other filters work on every channel");

    let set = json!({"effect": effect.0, "horizontalRotation": 370.0, "verticalRotation": -90.0, "exposure": 9.0, "heightIntensity": -1.0, "ambientOcclusion": 0.5, "specularOcclusion": 2.0});
    execute(&mut s, "effect.set", &set).unwrap();
    assert_eq!(
        filter_of(&s, effect),
        Filter::EnvironmentLighting {
            horizontal: 10.0,
            vertical: 270.0,
            exposure: 5.0,
            height_intensity: 0.0,
            ambient_occlusion: 0.5,
            specular_occlusion: 1.0
        }
    );
    assert_eq!(execute(&mut s, "effect.filter.restoreDefaults", &json!({"effect": effect.0})).unwrap()["changed"], json!(true));
    assert_eq!(filter_of(&s, effect), made);
    let listed = execute(&mut s, "filter.list", &json!({})).unwrap();
    assert!(listed.as_array().unwrap().iter().any(|f| f["id"] == "bakedLightingEnvironment" && f["mask"] == false));
}

/// Baked Lighting Environment lights the base color with the viewport environment: another environment relights it, Exposure brightens it
#[test]
fn environment_lighting_follows_the_viewport_environment() {
    let mut s = session();
    execute(&mut s, "layer.add.fill", &json!({"color": [0.5, 0.5, 0.5]})).unwrap();
    let center = |s: &mut Session| {
        s.update();
        s.caches[0].composite.value(Channel::BaseColor, 64, 64)
    };
    let unlit = center(&mut s);
    let effect =
        LayerId(execute(&mut s, "effect.add.filter", &json!({"target": "content", "filter": "bakedLightingEnvironment"})).unwrap()["effect"].as_u64().unwrap());
    let studio = center(&mut s);
    assert_ne!(studio, unlit, "the studio lights the base color");
    execute(&mut s, "display.set", &json!({"environment": "Night"})).unwrap();
    let night = center(&mut s);
    assert!(night[0] < studio[0] * 0.5, "the night lights it less: {night:?} vs {studio:?}");
    execute(&mut s, "effect.set", &json!({"effect": effect.0, "exposure": 3.0})).unwrap();
    assert!(center(&mut s)[0] > night[0], "exposure brightens it");
}
