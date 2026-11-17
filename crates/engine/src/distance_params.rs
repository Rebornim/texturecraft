//! The 3D Distance and 3D Linear Gradient controls `effect.add.generator` and `effect.set` take

use serde_json::Value;
use texturecraft_core::{Generator, GeneratorKind};

use crate::Result;
use crate::commands::{bad, color_param, f32p};

/// The 3D Distance controls: the point's X, Y and Z across the mesh's bounds, then the radius, offset and contrast
pub const DISTANCE_KEYS: [&str; 6] = ["distanceX", "distanceY", "distanceZ", "distanceRadius", "distanceOffset", "distanceContrast"];

/// The 3D Linear Gradient controls: its start and end points ([x, y, z] across the mesh's bounds), balance and contrast
pub const LINEAR_GRADIENT_KEYS: [&str; 4] = ["gradientStart", "gradientEnd", "gradientBalance", "gradientContrast"];

/// The controls of the earlier model and the global adjustments, which the 3D kinds don't have
const OTHER_MODEL_KEYS: [&str; 9] = ["level", "softness", "noise", "noiseScale", "offset", "direction", "blur", "balance", "contrast"];

/// The 3D Distance and 3D Linear Gradient controls in `params`, each refused on other kinds; both kinds refuse the earlier model's and the global controls
pub fn distance_from(command: &str, params: &Value, generator: &mut Generator) -> Result<()> {
    let kind = generator.kind;
    for (keys, owner) in [(&DISTANCE_KEYS[..], GeneratorKind::ThreeDDistance), (&LINEAR_GRADIENT_KEYS[..], GeneratorKind::ThreeDLinearGradient)] {
        if let Some(key) = first_given(params, keys).filter(|_| kind != owner) {
            return Err(bad(command, format!("{} has no `{key}`: only {} has it", kind.label(), owner.label())));
        }
    }
    if !matches!(kind, GeneratorKind::ThreeDDistance | GeneratorKind::ThreeDLinearGradient) {
        return Ok(());
    }
    if let Some(key) = first_given(params, &OTHER_MODEL_KEYS) {
        return Err(bad(command, format!("{} has no `{key}`: it has its own controls only", kind.label())));
    }
    if kind == GeneratorKind::ThreeDLinearGradient {
        return linear_gradient_from(command, params, generator);
    }

    let mut distance = generator.distance_controls().unwrap_or_default();
    for (axis, key) in DISTANCE_KEYS.iter().take(3).enumerate() {
        if let (Some(value), Some(coordinate)) = (f32p(params, key), distance.point.get_mut(axis)) {
            *coordinate = value;
        }
    }
    for (key, value) in [("distanceRadius", &mut distance.radius), ("distanceOffset", &mut distance.offset), ("distanceContrast", &mut distance.contrast)] {
        if let Some(given) = f32p(params, key) {
            *value = given;
        }
    }
    generator.distance = Some(Box::new(distance.clamped()));
    Ok(())
}

/// The 3D Linear Gradient controls in `params` (the generator is one)
fn linear_gradient_from(command: &str, params: &Value, generator: &mut Generator) -> Result<()> {
    let mut gradient = generator.linear_gradient_controls().unwrap_or_default();
    if let Some(start) = color_param(command, params, "gradientStart")? {
        gradient.start = start;
    }
    if let Some(end) = color_param(command, params, "gradientEnd")? {
        gradient.end = end;
    }
    for (key, value) in [("gradientBalance", &mut gradient.balance), ("gradientContrast", &mut gradient.contrast)] {
        if let Some(given) = f32p(params, key) {
            *value = given;
        }
    }
    generator.linear_gradient = Some(Box::new(gradient.clamped()));
    Ok(())
}

/// The first of `keys` that `params` gives
fn first_given<'a>(params: &Value, keys: &[&'a str]) -> Option<&'a str> {
    keys.iter().find(|key| params.get(**key).is_some()).copied()
}
