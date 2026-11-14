//! `effect.set` keys of Baked Lighting Stylized past Baked Lighting's sun and ambient

use serde_json::Value;
use texturecraft_core::{DirectionalLight, LightingOutput, StylizedLighting};

use crate::Result;
use crate::commands::{bad, color_param, f32p, strp};

/// The keys that give a Baked Lighting saved before them its stylized settings
pub(crate) const KEYS: [&str; 17] = [
    "sunColor",
    "skyColor",
    "horizonColor",
    "groundColor",
    "light1",
    "light2",
    "diffuseCavity",
    "specularAo",
    "specularCavity",
    "cavitySmoothness",
    "edgesIntensity",
    "edgesSmoothness",
    "input",
    "output",
    "dielectricReflectance",
    "normalDetails",
    "heightToNormal",
];

/// The highest Dielectric Reflectance, Cavity Smoothness and Height to Normal Intensity
pub const MAX_DIELECTRIC_REFLECTANCE: f32 = 0.16;
pub const MAX_CAVITY_SMOOTHNESS: f32 = 4.0;
pub const MAX_HEIGHT_TO_NORMAL: f32 = 3.0;

/// The highest intensity of Light 1 and Light 2
pub const MAX_LIGHT_INTENSITY: f32 = 2.0;

/// Whether `p` sets any stylized setting
pub(crate) fn sets_any(p: &Value) -> bool {
    KEYS.iter().any(|key| p.get(*key).is_some())
}

/// Set the stylized settings `p` names on `s`, clamped; a bad color or choice is refused
pub(crate) fn apply(p: &Value, s: &mut StylizedLighting, cmd: &str) -> Result<()> {
    let set = |slot: &mut f32, key: &str, hi: f32| {
        if let Some(v) = f32p(p, key) {
            *slot = v.clamp(0.0, hi);
        }
    };
    for (key, slot) in
        [("sunColor", &mut s.sun_color), ("skyColor", &mut s.sky_color), ("horizonColor", &mut s.horizon_color), ("groundColor", &mut s.ground_color)]
    {
        color(p, key, slot, cmd)?;
    }
    for (key, light) in ["light1", "light2"].into_iter().zip(s.lights.iter_mut()) {
        if let Some(params) = p.get(key) {
            set_light(params, light, key, cmd)?;
        }
    }
    set(&mut s.diffuse_cavity, "diffuseCavity", 1.0);
    set(&mut s.specular_ao, "specularAo", 1.0);
    set(&mut s.specular_cavity, "specularCavity", 1.0);
    set(&mut s.cavity_smoothness, "cavitySmoothness", MAX_CAVITY_SMOOTHNESS);
    set(&mut s.edges_intensity, "edgesIntensity", 1.0);
    set(&mut s.edges_smoothness, "edgesSmoothness", 1.0);
    set(&mut s.dielectric_reflectance, "dielectricReflectance", MAX_DIELECTRIC_REFLECTANCE);
    set(&mut s.height_to_normal, "heightToNormal", MAX_HEIGHT_TO_NORMAL);
    if p.get("input").is_some() {
        s.specular_glossiness = match strp(p, "input") {
            Some("metalRoughness") => false,
            Some("specularGlossiness") => true,
            _ => return Err(bad(cmd, "`input` must be \"metalRoughness\" or \"specularGlossiness\"")),
        };
    }
    if p.get("output").is_some() {
        let id = strp(p, "output");
        s.output = LightingOutput::ALL
            .into_iter()
            .find(|output| Some(output.id()) == id)
            .ok_or_else(|| bad(cmd, "`output` must be \"diffuseSpecular\", \"diffuse\" or \"specular\""))?;
    }
    if p.get("normalDetails").is_some() {
        s.mesh_only = match strp(p, "normalDetails") {
            Some("mesh") => true,
            Some("meshHeightNormal") => false,
            _ => return Err(bad(cmd, "`normalDetails` must be \"mesh\" or \"meshHeightNormal\"")),
        };
    }
    Ok(())
}

/// Light 1 or Light 2 from `{azimuth, elevation, intensity, color}` (any of them)
fn set_light(p: &Value, light: &mut DirectionalLight, key: &str, cmd: &str) -> Result<()> {
    if !p.is_object() {
        return Err(bad(cmd, format!("`{key}` must be an object {{azimuth, elevation, intensity, color}}")));
    }

    if let Some(v) = f32p(p, "azimuth") {
        light.azimuth = v.rem_euclid(360.0);
    }
    if let Some(v) = f32p(p, "elevation") {
        light.elevation = v.rem_euclid(360.0);
    }
    if let Some(v) = f32p(p, "intensity") {
        light.intensity = v.clamp(0.0, MAX_LIGHT_INTENSITY);
    }
    color(p, "color", &mut light.color, cmd)
}

/// A color key, refused unless it reads as a color
fn color(p: &Value, key: &str, slot: &mut [f32; 3], cmd: &str) -> Result<()> {
    if p.get(key).is_some() {
        let v = color_param(cmd, p, key)?.ok_or_else(|| bad(cmd, format!("`{key}` must be a color ([r, g, b] linear or \"#rrggbb\")")))?;
        *slot = v.map(|x| x.clamp(0.0, 1.0));
    }
    Ok(())
}
