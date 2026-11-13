//! The gradient filters: Gradient (positions, quantity, input transformation, linear blending, debug), Gradient Curve and Gradient Dynamic

use serde_json::{Value, json};
use texturecraft_engine::core::{Channel, EffectKind, Filter, GrayscaleMode, LayerId, layer::gradient};
use texturecraft_engine::geom::glam::Vec2;
use texturecraft_engine::geom::primitives;
use texturecraft_engine::{Session, execute};

const SIZE: u32 = 128;

fn session() -> Session {
    let mut s = Session::new_project("t", primitives::sphere(1.0, 48, 24), "builtin:sphere", None, SIZE).unwrap();
    s.viewport = Vec2::new(400.0, 400.0);
    s.camera.yaw = 0.0;
    s.camera.pitch = 0.0;
    s.update();
    s
}

/// Composited value of `channel` under the viewport center, and the UV it reads
fn center(s: &mut Session, channel: Channel) -> ([f32; 3], Vec2) {
    s.update();
    let hit = s.pick(Vec2::new(200.0, 200.0), s.viewport).unwrap();
    (s.caches[0].composite.value(channel, (hit.uv.x * SIZE as f32) as u32, (hit.uv.y * SIZE as f32) as u32), hit.uv)
}

/// A fill layer of base color `color` under content filter `params`: the effect
fn filtered(s: &mut Session, color: [f32; 3], params: Value) -> LayerId {
    execute(s, "layer.add.fill", &json!({"color": color, "roughness": 0.3})).unwrap();
    let mut params = params;
    params["target"] = json!("content");
    LayerId(execute(s, "effect.add.filter", &params).unwrap()["effect"].as_u64().unwrap())
}

fn filter_of(s: &Session, effect: LayerId) -> Filter {
    match &s.doc.project.effect(effect).unwrap().1.kind {
        EffectKind::Filter(filter) => filter.clone(),
        other => panic!("not a filter: {other:?}"),
    }
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 6e-3
}

/// Projects saved before read a gradient as evenly spaced colors from the texel's luma, blended in linear, and save it as before
#[test]
fn a_gradient_saved_before_reads_and_saves_as_before() {
    let saved = json!({"GradientMap": {"colors": [[1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]]}});
    let filter: Filter = serde_json::from_value(saved.clone()).unwrap();
    assert_eq!(filter, Filter::even_gradient(vec![[1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]]));
    assert_eq!(serde_json::to_value(&filter).unwrap(), saved, "nothing new is saved");
    let Filter::GradientMap { colors, input, .. } = &filter else { panic!() };
    assert_eq!(input.mode, GrayscaleMode::Luma);
    for t in [0.0, 0.2, 0.37, 0.5, 0.81, 1.0] {
        assert_eq!(filter.ramp_at(t), gradient(colors, t), "bit for bit as before at {t}");
    }
}

/// The library's Gradient: black, mid gray and white at 0, 0.5 and 1; places, quantity, invert, balance and linear blending move what a texel reads
#[test]
fn gradient_places_quantity_and_input_transformation() {
    let mut s = session();
    let effect = filtered(&mut s, [0.25; 3], json!({"filter": "gradientMap"}));
    assert_eq!(s.doc.project.effect(effect).unwrap().1.name, "Gradient");
    let Filter::GradientMap { quantity, input, linear, .. } = filter_of(&s, effect) else { panic!() };
    assert_eq!((quantity, input.mode, linear), (Some(3), GrayscaleMode::Average, true));
    let mid = 0.214;
    assert!(near(center(&mut s, Channel::BaseColor).0[0], mid / 2.0), "halfway from black to mid gray");

    execute(&mut s, "effect.set", &json!({"effect": effect.0, "positions": [0.0, 0.25, 1.0]})).unwrap();
    assert!(near(center(&mut s, Channel::BaseColor).0[0], mid), "mid gray moved to 0.25");
    execute(&mut s, "effect.set", &json!({"effect": effect.0, "invert": true})).unwrap();
    assert!(near(center(&mut s, Channel::BaseColor).0[0], mid + (1.0 - mid) * (0.5 / 0.75)), "inverted, it reads 0.75");
    execute(&mut s, "effect.set", &json!({"effect": effect.0, "invert": false, "balance": 0.75})).unwrap();
    assert!(near(center(&mut s, Channel::BaseColor).0[0], mid + (1.0 - mid) * (0.25 / 0.75)), "balance 0.75 lifts it to 0.5");

    execute(&mut s, "effect.set", &json!({"effect": effect.0, "balance": 0.5, "positions": [0.0, 0.5, 1.0], "linear": false})).unwrap();
    let srgb = center(&mut s, Channel::BaseColor).0[0];
    assert!(near(srgb, 0.0508), "blended in sRGB, halfway from black to mid gray is sRGB 0.25: {srgb}");

    execute(&mut s, "effect.set", &json!({"effect": effect.0, "quantity": 4})).unwrap();
    assert_eq!(filter_of(&s, effect).ramp_colors().len(), 4);
    for bad in [
        json!({"quantity": 11}),
        json!({"quantity": 1}),
        json!({"grayscaleMode": "x"}),
        json!({"positions": [0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0]}),
        json!({"debug": 3}),
    ] {
        let mut params = bad.clone();
        params["effect"] = json!(effect.0);
        assert!(execute(&mut s, "effect.set", &params).is_err(), "{bad} refused");
    }
    execute(&mut s, "effect.set", &json!({"effect": effect.0, "grayscaleMode": "max"})).unwrap();
    let Filter::GradientMap { input, .. } = filter_of(&s, effect) else { panic!() };
    assert_eq!(input.mode, GrayscaleMode::Max);
}

/// Display Gradient (Debug) shows the ramp left to right across the texture
#[test]
fn display_gradient_shows_the_ramp_across_the_texture() {
    let mut s = session();
    let effect = filtered(&mut s, [0.9; 3], json!({"filter": "gradientMap", "debug": true}));
    let (value, uv) = center(&mut s, Channel::BaseColor);
    let ramp = filter_of(&s, effect).ramp_at(uv.x).unwrap();
    assert!((0..3).all(|k| (value[k] - ramp[k]).abs() < 0.03), "{value:?} vs {ramp:?} at u {}", uv.x);
}

/// An old gradient that gains a quantity keeps its colors' places and adds the new one at 1
#[test]
fn a_saved_gradient_keeps_its_places_when_it_gains_a_color() {
    let mut s = session();
    let effect = filtered(&mut s, [0.5; 3], json!({"filter": "gradientMap"}));
    if let Some(found) = s.doc.project.effect_mut(effect) {
        found.kind = EffectKind::Filter(Filter::even_gradient(vec![[0.0; 3], [1.0, 0.0, 0.0], [1.0; 3]]));
    }
    execute(&mut s, "effect.set", &json!({"effect": effect.0, "quantity": 4})).unwrap();
    let Filter::GradientMap { colors, positions, quantity, .. } = filter_of(&s, effect) else { panic!() };
    assert_eq!((positions, quantity), (vec![0.0, 0.5, 1.0, 1.0], Some(4)));
    assert_eq!(colors.last(), Some(&[1.0; 3]), "the new color starts as the last one");
}

/// Gradient Curve: values remapped through the preset after balance, contrast and invert
#[test]
fn gradient_curve_remaps_through_its_preset() {
    let mut s = session();
    let effect = filtered(&mut s, [0.25; 3], json!({"filter": "gradientCurve"}));
    assert_eq!(s.doc.project.effect(effect).unwrap().1.name, "Gradient Curve");
    let round = (1.0f32 - 0.75 * 0.75).sqrt();
    assert!(near(center(&mut s, Channel::BaseColor).0[0], round), "Round at 0.25");
    execute(&mut s, "effect.set", &json!({"effect": effect.0, "preset": "easeIn"})).unwrap();
    assert!(near(center(&mut s, Channel::BaseColor).0[0], 0.0625), "Ease In at 0.25");
    execute(&mut s, "effect.set", &json!({"effect": effect.0, "invert": true})).unwrap();
    assert!(near(center(&mut s, Channel::BaseColor).0[0], 0.5625), "Ease In at 0.75");
    assert!(execute(&mut s, "effect.set", &json!({"effect": effect.0, "preset": "spiral"})).is_err());
    let Filter::GradientCurve { preset, .. } = filter_of(&s, effect) else { panic!() };
    assert_eq!(preset.id(), "easeIn", "a refused preset changes nothing");
}

/// Gradient Dynamic with a uniform Gradient Source: every texel reads that color (scalars its gray)
#[test]
fn gradient_dynamic_reads_its_uniform_source() {
    let mut s = session();
    let effect = filtered(&mut s, [0.9, 0.1, 0.1], json!({"filter": "gradientDynamic", "color": [0.2, 0.4, 0.6]}));
    assert_eq!(s.doc.project.effect(effect).unwrap().1.name, "Gradient Dynamic");
    let color = center(&mut s, Channel::BaseColor).0;
    assert!(near(color[0], 0.2) && near(color[1], 0.4) && near(color[2], 0.6), "{color:?}");
    execute(&mut s, "effect.set", &json!({"effect": effect.0, "orientation": "vertical", "position": 0.7})).unwrap();
    let Filter::GradientDynamic { vertical, position, .. } = filter_of(&s, effect) else { panic!() };
    assert!(vertical && position == 0.7);
    assert!(execute(&mut s, "effect.set", &json!({"effect": effect.0, "orientation": "diagonal"})).is_err());
}
