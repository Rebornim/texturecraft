//! What Baked Lighting Stylized adds to Baked Lighting's sun and ambient: sky colors, two more lights, cavity, edges and highlights

use std::f32::consts::PI;

use serde::{Deserialize, Serialize};

use crate::directional_light::DirectionalLight;
use crate::lighting_output::LightingOutput;
use crate::sun_and_sky::SunAndSky;
use crate::{Channel, mix3};

/// Baked Lighting Stylized's settings past the sun's direction and strength, the sky's strength and Diffuse AO (kept on the filter, as Baked Lighting saved them)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StylizedLighting {
    pub sun_color: [f32; 3],
    /// The ambient light from straight above, at the horizon and from below; normals between blend them
    pub sky_color: [f32; 3],
    pub horizon_color: [f32; 3],
    pub ground_color: [f32; 3],
    /// Light 1 and Light 2
    pub lights: [DirectionalLight; 2],
    /// How much the cavities (concave curvature) darken the lit surface and its highlights
    pub diffuse_cavity: f32,
    pub specular_cavity: f32,
    /// How much the baked ambient occlusion darkens the highlights
    pub specular_ao: f32,
    /// The concavity (and convexity, for edges) over which cavity and edges fade in: 0 is a hard step
    pub cavity_smoothness: f32,
    /// How much the edges (convex curvature) lighten toward white
    pub edges_intensity: f32,
    pub edges_smoothness: f32,
    /// Reads the Specular and Glossiness channels instead of Metallic and Roughness (Input)
    pub specular_glossiness: bool,
    pub output: LightingOutput,
    /// The reflectance of non-metals at normal incidence
    pub dielectric_reflectance: f32,
    /// Lights the mesh's own normals, without the baked normal and height detail (Normal Details Type Mesh only)
    pub mesh_only: bool,
    pub height_to_normal: f32,
}

/// The roughness highlights never go below (a mirror would be a single texel of infinite light)
const MIN_ROUGHNESS: f32 = 0.08;

/// The smoothness below which cavity and edges switch on as a step
const MIN_SMOOTHNESS: f32 = 1e-3;

impl StylizedLighting {
    /// The settings that light exactly as Baked Lighting did: white light and sky, no extra lights, cavity, edges or highlights, the mesh's own normals
    pub const PLAIN: StylizedLighting = StylizedLighting {
        sun_color: [1.0; 3],
        sky_color: [1.0; 3],
        horizon_color: [1.0; 3],
        ground_color: [1.0; 3],
        lights: [DirectionalLight::off(90.0), DirectionalLight::off(180.0)],
        diffuse_cavity: 0.0,
        specular_cavity: 0.0,
        specular_ao: 0.0,
        cavity_smoothness: 0.25,
        edges_intensity: 0.0,
        edges_smoothness: 0.0,
        specular_glossiness: false,
        output: LightingOutput::Diffuse,
        dielectric_reflectance: 0.04,
        mesh_only: true,
        height_to_normal: 1.0,
    };

    /// A new Baked Lighting Stylized: a warm sun, a blue sky over a brown ground, highlights, and the baked normal and height detail
    pub fn library() -> StylizedLighting {
        StylizedLighting {
            sun_color: [1.0, 0.905, 0.716],
            sky_color: [0.305, 0.456, 0.791],
            horizon_color: [0.644, 0.644, 0.61],
            ground_color: [0.102, 0.08, 0.061],
            output: LightingOutput::DiffuseSpecular,
            mesh_only: false,
            ..StylizedLighting::PLAIN
        }
    }

    /// The ambient light reaching a surface facing `nrm` from the sky, horizon and ground
    pub fn ambient(&self, nrm: [f32; 3]) -> [f32; 3] {
        let up = nrm[1].clamp(-1.0, 1.0);
        if up >= 0.0 { mix3(self.horizon_color, self.sky_color, up) } else { mix3(self.horizon_color, self.ground_color, -up) }
    }

    /// The lit base color of linear `albedo` facing `nrm` (world space, unit) with baked ambient occlusion `ao` and curvature `curvature` (0.5 flat),
    /// its material read from `channels` (straight values by [`Channel::index`]); the viewer looks straight at the surface
    pub fn shade(&self, light: SunAndSky, albedo: [f32; 3], nrm: [f32; 3], [ao, curvature]: [f32; 2], channels: &[[f32; 3]]) -> [f32; 3] {
        let channel = |channel: Channel| channels.get(channel.index()).copied().unwrap_or_else(|| channel.default_value());
        let unit = |x: f32| if x.is_finite() { x.clamp(0.0, 1.0) } else { 0.0 };
        let ao = if ao.is_finite() { ao.clamp(0.0, 1.0) } else { 1.0 };
        let curvature = if curvature.is_finite() { curvature } else { 0.5 };
        let cavity = fade_in((0.5 - curvature) * 2.0, self.cavity_smoothness);
        let edge = fade_in((curvature - 0.5) * 2.0, self.edges_smoothness);

        let (roughness, f0, diffuse_albedo) = if self.specular_glossiness {
            (1.0 - unit(channel(Channel::Glossiness)[0]), channel(Channel::Specular).map(unit), albedo)
        } else {
            let metal = unit(channel(Channel::Metallic)[0]);
            let f0 = mix3([unit(self.dielectric_reflectance); 3], albedo, metal);
            // Without highlights the whole base color is lit, as Baked Lighting did
            let kept = if self.output.specular() { 1.0 - metal } else { 1.0 };
            (unit(channel(Channel::Roughness)[0]), f0, albedo.map(|x| x * kept))
        };
        let roughness = roughness.max(MIN_ROUGHNESS);

        let lights = std::iter::once((light.sun.direction(), light.sun.intensity, self.sun_color))
            .chain(self.lights.iter().map(|l| (l.direction(), l.intensity, l.color)));
        let sky = self.ambient(nrm);
        let diffuse_ao = 1.0 + (ao - 1.0) * unit(light.diffuse_ao);
        let specular_ao = 1.0 + (ao - 1.0) * unit(self.specular_ao);
        let diffuse_cavity = 1.0 - cavity * unit(self.diffuse_cavity);
        let specular_cavity = 1.0 - cavity * unit(self.specular_cavity);

        let mut diffuse = sky.map(|s| s * light.sky_intensity * diffuse_ao * diffuse_cavity);
        let mut specular = [0.0f32; 3];
        for (dir, intensity, color) in lights {
            let facing = dot(nrm, dir);
            if facing <= 0.0 || intensity <= 0.0 {
                continue;
            }
            let half = normalize([nrm[0] + dir[0], nrm[1] + dir[1], nrm[2] + dir[2]]);
            let nh = dot(nrm, half).max(0.0);
            let d = ggx(nh, roughness) / 4.0 * facing * specular_cavity;
            for k in 0..3 {
                diffuse[k] += color[k] * intensity * facing;
                specular[k] += color[k] * intensity * d * schlick(f0[k], nh);
            }
        }
        for k in 0..3 {
            specular[k] += f0[k] * sky[k] * light.sky_intensity * specular_ao * specular_cavity;
        }

        let mut out = [0.0f32; 3];
        for k in 0..3 {
            let lit = if self.output.diffuse() { diffuse_albedo[k] * diffuse[k] } else { 0.0 };
            let shine = if self.output.specular() { specular[k] } else { 0.0 };
            let v = unit(lit + shine);
            out[k] = unit(v + (1.0 - v) * edge * unit(self.edges_intensity));
        }
        out
    }
}

/// `amount` (clamped 0..1) reaching full over `smoothness`
fn fade_in(amount: f32, smoothness: f32) -> f32 {
    let amount = amount.clamp(0.0, 1.0);
    if smoothness <= MIN_SMOOTHNESS { if amount > 0.0 { 1.0 } else { 0.0 } } else { (amount / smoothness).min(1.0) }
}

/// The GGX distribution of microfacets facing `nh` at `roughness`
fn ggx(nh: f32, roughness: f32) -> f32 {
    let a2 = (roughness * roughness).powi(2);
    let d = nh * nh * (a2 - 1.0) + 1.0;
    a2 / (PI * d * d)
}

/// Schlick's Fresnel of reflectance `f0` at cosine `cos`
fn schlick(f0: f32, cos: f32) -> f32 {
    f0 + (1.0 - f0) * (1.0 - cos.clamp(0.0, 1.0)).powi(5)
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let len = dot(v, v).sqrt();
    if len > 1e-6 { v.map(|x| x / len) } else { [0.0, 1.0, 0.0] }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Filter;

    const UP: [f32; 3] = [0.0, 1.0, 0.0];

    fn material(roughness: f32) -> Vec<[f32; 3]> {
        let mut channels: Vec<[f32; 3]> = Channel::ALL.iter().map(|c| c.default_value()).collect();
        channels[Channel::Roughness.index()] = [roughness; 3];
        channels
    }

    fn light(elevation: f32, intensity: f32, sky_intensity: f32) -> SunAndSky {
        SunAndSky { sun: DirectionalLight { azimuth: 0.0, elevation, intensity, color: [1.0; 3] }, sky_intensity, diffuse_ao: 0.5 }
    }

    /// The plain settings light as Baked Lighting did: the base color times sun facing plus occluded ambient
    #[test]
    fn plain_settings_light_as_baked_lighting_did() {
        let legacy = Filter::BakedLighting { azimuth: 0.0, elevation: 30.0, light: 0.6, ambient: 0.4, occlusion: 0.5, stylized: None };
        for (nrm, ao) in [(UP, 1.0), ([0.6, 0.8, 0.0], 0.3), ([0.0, -1.0, 0.0], 0.7)] {
            let k = legacy.baked_light(nrm, ao).unwrap_or(f32::NAN);
            let shaded = StylizedLighting::PLAIN.shade(light(30.0, 0.6, 0.4), [0.5, 0.25, 1.0], nrm, [ao, 0.5], &material(0.5));
            let expected = [0.5f32, 0.25, 1.0].map(|x| (x * k).clamp(0.0, 1.0));
            assert!(shaded.iter().zip(expected).all(|(a, b)| (a - b).abs() < 1e-5), "{nrm:?}: {shaded:?} vs {expected:?}");
        }
    }

    /// The sky, horizon and ground colors light a surface by where it faces; Light 1 adds its color
    #[test]
    fn sky_colors_and_extra_lights() {
        let colors = StylizedLighting { sky_color: [0.0, 0.0, 1.0], horizon_color: [0.0, 1.0, 0.0], ground_color: [1.0, 0.0, 0.0], ..StylizedLighting::PLAIN };
        let sky = light(45.0, 0.0, 1.0);
        let white = [1.0; 3];
        assert_eq!(colors.shade(sky, white, UP, [1.0, 0.5], &material(0.5)), [0.0, 0.0, 1.0]);
        assert_eq!(colors.shade(sky, white, [1.0, 0.0, 0.0], [1.0, 0.5], &material(0.5)), [0.0, 1.0, 0.0]);
        assert_eq!(colors.shade(sky, white, [0.0, -1.0, 0.0], [1.0, 0.5], &material(0.5)), [1.0, 0.0, 0.0]);

        let mut extra = StylizedLighting::PLAIN;
        extra.lights[0] = DirectionalLight { azimuth: 0.0, elevation: 90.0, intensity: 0.5, color: [1.0, 0.0, 0.0] };
        let dark = light(45.0, 0.0, 0.0);
        let lit = extra.shade(dark, white, UP, [1.0, 0.5], &material(0.5));
        assert!((lit[0] - 0.5).abs() < 1e-5 && lit[1] == 0.0, "{lit:?}");
    }

    /// Highlights: a smooth surface shines brighter than a rough one; Specular only keeps them alone, Diffuse only drops them
    #[test]
    fn highlights_follow_roughness_and_the_output() {
        let sun = light(90.0, 1.0, 0.0);
        let shiny = StylizedLighting { output: LightingOutput::Specular, ..StylizedLighting::library() };
        let albedo = [0.0; 3];
        let smooth = shiny.shade(sun, albedo, UP, [1.0, 0.5], &material(0.2));
        let rough = shiny.shade(sun, albedo, UP, [1.0, 0.5], &material(0.9));
        assert!(smooth[0] > rough[0] + 0.1 && rough[0] > 0.0, "{smooth:?} {rough:?}");
        let diffuse = StylizedLighting { output: LightingOutput::Diffuse, ..StylizedLighting::library() };
        assert_eq!(diffuse.shade(sun, albedo, UP, [1.0, 0.5], &material(0.2)), [0.0; 3]);
    }

    /// Cavities darken the lit surface by Diffuse Cavity; edges lighten toward white by Edges Intensity
    #[test]
    fn cavities_darken_and_edges_lighten() {
        let sky = light(45.0, 0.0, 1.0);
        let gray = [0.5; 3];
        let cavities = StylizedLighting { diffuse_cavity: 1.0, ..StylizedLighting::PLAIN };
        assert_eq!(cavities.shade(sky, gray, UP, [1.0, 0.5], &material(0.5)), gray, "flat: no cavity");
        assert_eq!(cavities.shade(sky, gray, UP, [1.0, 0.2], &material(0.5)), [0.0; 3], "a deep cavity");
        let edges = StylizedLighting { edges_intensity: 1.0, ..StylizedLighting::PLAIN };
        assert_eq!(edges.shade(sky, gray, UP, [1.0, 0.9], &material(0.5)), [1.0; 3], "a sharp edge");
    }
}
