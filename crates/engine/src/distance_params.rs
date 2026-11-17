//! The controls of the kinds with only the reference's own controls (3D Distance, 3D Linear Gradient, UV Border Distance, Grayscale Conversion) that `effect.add.generator` and `effect.set` take

use serde_json::Value;
use texturecraft_core::{DrippingRustParameters, Generator, GeneratorKind, GrayscaleConversion, KindControls, UvRandomColorParameters};

use crate::Result;
use crate::commands::{bad, boolp, color_of, color_param, f32p, u64p};

/// The 3D Distance controls: the point's X, Y and Z across the mesh's bounds, then the radius, offset and contrast
pub const DISTANCE_KEYS: [&str; 6] = ["distanceX", "distanceY", "distanceZ", "distanceRadius", "distanceOffset", "distanceContrast"];

/// The 3D Linear Gradient controls: its start and end points ([x, y, z] across the mesh's bounds), balance and contrast
pub const LINEAR_GRADIENT_KEYS: [&str; 4] = ["gradientStart", "gradientEnd", "gradientBalance", "gradientContrast"];

/// The UV Border Distance controls
pub const BORDER_DISTANCE_KEYS: [&str; 4] = ["borderDistance", "borderSmoothness", "borderBalance", "borderContrast"];

/// The Grayscale Conversion controls (its image is the `image` input)
pub const GRAYSCALE_KEYS: [&str; 6] = ["grayscaleType", "grayscaleBalance", "grayscaleContrast", "grayscaleTile", "grayscaleRotation", "grayscaleSafeRotation"];

/// The Dripping Rust controls (it also takes the seed and invert)
pub const DRIPPING_RUST_KEYS: [&str; 7] =
    ["rustSpreading", "rustContrast", "spreadingSmoothness", "dripsIntensity", "dripsSmoothness", "dripsSamples", "dripsAxis"];

/// The UV Random Color controls (it also takes the seed)
pub const UV_RANDOM_COLOR_KEYS: [&str; 2] = ["colorSourceMode", "gradientColors"];

/// The controls of the earlier model and the global adjustments, which these kinds don't have
const OTHER_MODEL_KEYS: [&str; 9] = ["level", "softness", "noise", "noiseScale", "offset", "direction", "blur", "balance", "contrast"];

/// The controls of the kinds with only their own controls in `params`, each refused on other kinds; these kinds refuse the earlier model's and the global controls
pub fn distance_from(command: &str, params: &Value, generator: &mut Generator) -> Result<()> {
    let kind = generator.kind;
    for (keys, owner) in [
        (&DISTANCE_KEYS[..], GeneratorKind::ThreeDDistance),
        (&LINEAR_GRADIENT_KEYS[..], GeneratorKind::ThreeDLinearGradient),
        (&BORDER_DISTANCE_KEYS[..], GeneratorKind::UvBorderDistance),
        (&GRAYSCALE_KEYS[..], GeneratorKind::GrayscaleConversion),
        (&DRIPPING_RUST_KEYS[..], GeneratorKind::DrippingRust),
        (&UV_RANDOM_COLOR_KEYS[..], GeneratorKind::UvRandomColor),
    ] {
        if let Some(key) = first_given(params, keys).filter(|_| kind != owner) {
            return Err(bad(command, format!("{} has no `{key}`: only {} has it", kind.label(), owner.label())));
        }
    }
    if !kind.has_no_earlier_model() || kind == GeneratorKind::FiberGlassEdgeWear {
        return Ok(());
    }
    if let Some(key) = first_given(params, &OTHER_MODEL_KEYS) {
        return Err(bad(command, format!("{} has no `{key}`: it has its own controls only", kind.label())));
    }
    if kind == GeneratorKind::ThreeDLinearGradient {
        return linear_gradient_from(command, params, generator);
    }
    if kind == GeneratorKind::GrayscaleConversion {
        return grayscale_from(command, params, generator);
    }
    if kind == GeneratorKind::DrippingRust {
        return dripping_rust_from(command, params, generator);
    }
    if kind == GeneratorKind::UvRandomColor {
        return uv_random_color_from(command, params, generator);
    }
    if kind == GeneratorKind::UvBorderDistance {
        let mut border = generator.border_distance_controls().unwrap_or_default();
        for (key, value) in [
            ("borderDistance", &mut border.distance),
            ("borderSmoothness", &mut border.smoothness),
            ("borderBalance", &mut border.balance),
            ("borderContrast", &mut border.contrast),
        ] {
            if let Some(given) = f32p(params, key) {
                *value = given;
            }
        }
        generator.controls = Some(Box::new(KindControls::BorderDistance(border.clamped())));
        return Ok(());
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
    generator.controls = Some(Box::new(KindControls::Distance(distance.clamped())));
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
    generator.controls = Some(Box::new(KindControls::LinearGradient(gradient.clamped())));
    Ok(())
}

/// The Grayscale Conversion controls in `params` (the generator is one)
fn grayscale_from(command: &str, params: &Value, generator: &mut Generator) -> Result<()> {
    let mut grayscale = generator.grayscale_controls().unwrap_or_default();
    if let Some(value) = params.get("grayscaleType") {
        let name = value.as_str().ok_or_else(|| bad(command, "`grayscaleType` must be a string"))?;
        grayscale.grayscale = GrayscaleConversion::parse(name).ok_or_else(|| bad(command, format!("unknown grayscale type `{name}`")))?;
    }
    for (key, value) in [
        ("grayscaleBalance", &mut grayscale.balance),
        ("grayscaleContrast", &mut grayscale.contrast),
        ("grayscaleTile", &mut grayscale.tile),
        ("grayscaleRotation", &mut grayscale.rotation),
    ] {
        if let Some(given) = f32p(params, key) {
            *value = given;
        }
    }
    if let Some(safe) = boolp(params, "grayscaleSafeRotation") {
        grayscale.safe_rotation = safe;
    }
    generator.controls = Some(Box::new(KindControls::Grayscale(grayscale.clamped())));
    Ok(())
}

/// The Dripping Rust controls in `params` (the generator is one)
fn dripping_rust_from(command: &str, params: &Value, generator: &mut Generator) -> Result<()> {
    let Some(KindControls::DrippingRust(mut rust)) = generator.kind_controls() else { return Ok(()) };
    for (key, value) in [
        ("rustSpreading", &mut rust.spreading),
        ("rustContrast", &mut rust.contrast),
        ("spreadingSmoothness", &mut rust.spreading_smoothness),
        ("dripsIntensity", &mut rust.drips_intensity),
        ("dripsSmoothness", &mut rust.drips_smoothness),
    ] {
        if let Some(given) = f32p(params, key) {
            *value = given;
        }
    }
    if let Some(samples) = u64p(params, "dripsSamples") {
        rust.drips_samples = samples.min(u64::from(DrippingRustParameters::MAX_SAMPLES)) as u32;
    }
    if let Some(value) = params.get("dripsAxis") {
        let name = value.as_str().ok_or_else(|| bad(command, "`dripsAxis` must be x, y or z"))?;
        rust.axis = match name.to_ascii_lowercase().as_str() {
            "x" => 0,
            "y" => 1,
            "z" => 2,
            _ => return Err(bad(command, format!("unknown axis `{name}`: x, y or z"))),
        };
    }
    generator.controls = Some(Box::new(KindControls::DrippingRust(rust.clamped())));
    Ok(())
}

/// The UV Random Color controls in `params` (the generator is one)
fn uv_random_color_from(command: &str, params: &Value, generator: &mut Generator) -> Result<()> {
    let Some(KindControls::UvRandomColor(mut random)) = generator.kind_controls() else { return Ok(()) };
    if let Some(value) = params.get("colorSourceMode") {
        random.custom_gradient = match value.as_str().map(str::to_ascii_lowercase).as_deref() {
            Some("random") => false,
            Some("customgradient" | "custom gradient" | "gradient") => true,
            _ => return Err(bad(command, "`colorSourceMode` must be random or customGradient")),
        };
    }
    if let Some(value) = params.get("gradientColors") {
        let invalid = || bad(command, "`gradientColors` takes 1 to 16 colors, each [r, g, b] or \"#rrggbb\"");
        let items = value.as_array().filter(|items| (1..=UvRandomColorParameters::MAX_GRADIENT_COLORS).contains(&items.len())).ok_or_else(invalid)?;
        random.gradient = items.iter().map(|item| color_of(item).ok_or_else(invalid)).collect::<Result<_>>()?;
    }
    generator.controls = Some(Box::new(KindControls::UvRandomColor(random.clamped())));
    Ok(())
}

/// The first of `keys` that `params` gives
fn first_given<'a>(params: &Value, keys: &[&'a str]) -> Option<&'a str> {
    keys.iter().find(|key| params.get(**key).is_some()).copied()
}
