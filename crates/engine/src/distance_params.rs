//! The 3D Distance controls `effect.add.generator` and `effect.set` take

use serde_json::Value;
use texturecraft_core::{Generator, GeneratorKind};

use crate::Result;
use crate::commands::{bad, f32p};

/// The 3D Distance controls: the point's X, Y and Z across the mesh's bounds, then the radius, offset and contrast
pub const DISTANCE_KEYS: [&str; 6] = ["distanceX", "distanceY", "distanceZ", "distanceRadius", "distanceOffset", "distanceContrast"];

/// The controls of the earlier model and the global adjustments, which 3D Distance doesn't have
const OTHER_MODEL_KEYS: [&str; 9] = ["level", "softness", "noise", "noiseScale", "offset", "direction", "blur", "balance", "contrast"];

/// The 3D Distance controls in `params`, refused on other kinds; 3D Distance refuses the earlier model's controls
pub fn distance_from(command: &str, params: &Value, generator: &mut Generator) -> Result<()> {
    if generator.kind != GeneratorKind::ThreeDDistance {
        return match first_given(params, &DISTANCE_KEYS) {
            Some(key) => Err(bad(command, format!("{} has no `{key}`: only 3D Distance has it", generator.kind.label()))),
            None => Ok(()),
        };
    }
    if let Some(key) = first_given(params, &OTHER_MODEL_KEYS) {
        return Err(bad(
            command,
            format!("3D Distance has no `{key}`: its controls are distanceX, distanceY, distanceZ, distanceRadius, distanceOffset and distanceContrast"),
        ));
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

/// The first of `keys` that `params` gives
fn first_given<'a>(params: &Value, keys: &[&'a str]) -> Option<&'a str> {
    keys.iter().find(|key| params.get(**key).is_some()).copied()
}
