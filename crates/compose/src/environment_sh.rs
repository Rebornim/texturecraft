//! The viewport environment's diffuse light as spherical harmonics, worked out once per environment, and Baked Lighting Environment's shading with it

use std::sync::{Arc, Mutex};

use texturecraft_core::project::DisplaySettings;
use texturecraft_core::{Channel, mix3};
use texturecraft_geom::glam::Vec3;

use crate::environment_sky::{radiance, sh_basis, sh9};
use crate::store::{ImageRes, ImageStore};

/// The width of the grid a built-in environment is sampled on (half as tall): its light varies slowly, so this is plenty for 9 coefficients
const BUILT_IN_WIDTH: u32 = 64;

/// The width an imported environment image is read at (every texel past it is skipped, not averaged: diffuse light is smooth)
const IMAGE_WIDTH: u32 = 128;

/// The reflectance of non-metals at normal incidence
const DIELECTRIC_REFLECTANCE: f32 = 0.04;

/// The diffuse light of the viewport environment and the turn the viewport gives it
pub struct EnvironmentSh {
    coefficients: [[f32; 3]; 9],
    /// The viewport's environment rotation (radians)
    rotation: f32,
}

/// The environment the last coefficients were worked out for: a built-in by name, or an imported image (held, so it is not a different image at the same address)
type CacheEntry = (String, Option<Arc<ImageRes>>, Arc<[[f32; 3]; 9]>);

static CACHE: Mutex<Option<CacheEntry>> = Mutex::new(None);

impl EnvironmentSh {
    /// The diffuse light of the environment `display` shows (an imported image from `images`), worked out once per environment
    pub fn of(display: &DisplaySettings, images: &ImageStore) -> EnvironmentSh {
        let image = display.environment_resource().and_then(|name| images.get(name)).cloned();
        let same = |entry: &CacheEntry| match (&entry.1, &image) {
            (Some(held), Some(image)) => Arc::ptr_eq(held, image),
            (None, None) => entry.0 == display.environment,
            _ => false,
        };
        let rotation = display.env_rotation.to_radians();

        // Worked out with the cache locked: tiles asking for the same light wait for it instead of working it out again
        let mut cache = CACHE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(entry) = cache.as_ref().filter(|entry| same(entry)) {
            return EnvironmentSh { coefficients: *entry.2, rotation };
        }

        let coefficients = match &image {
            Some(image) => image_sh(image),
            None => sh9(BUILT_IN_WIDTH, BUILT_IN_WIDTH / 2, |x, y| {
                let d = crate::environment_sky::dir_of((x as f32 + 0.5) / BUILT_IN_WIDTH as f32, (y as f32 + 0.5) / (BUILT_IN_WIDTH / 2) as f32);
                radiance(&display.environment, d)
            }),
        };
        *cache = Some((display.environment.clone(), image, Arc::new(coefficients)));
        EnvironmentSh { coefficients, rotation }
    }

    /// The diffuse light reaching a surface facing world direction `nrm` with the environment turned `horizontal` and tilted `vertical` (radians) past the viewport's turn
    pub fn irradiance(&self, nrm: Vec3, horizontal: f32, vertical: f32) -> Vec3 {
        let (sv, cv) = vertical.sin_cos();
        let tilted = Vec3::new(nrm.x, cv * nrm.y - sv * nrm.z, sv * nrm.y + cv * nrm.z);

        // Turned as the viewport turns its environment
        let (s, c) = (self.rotation + horizontal).sin_cos();
        let d = Vec3::new(c * tilted.x - s * tilted.z, tilted.y, s * tilted.x + c * tilted.z);
        let light = sh_basis(d).iter().zip(&self.coefficients).fold(Vec3::ZERO, |sum, (y, c)| sum + Vec3::from(*c) * *y);
        light.max(Vec3::ZERO)
    }

    /// Baked Lighting Environment's lit base color: linear `albedo` facing `nrm`, its metal read from `channels` (straight values by [`Channel::index`]), in `light` (the environment's diffuse light at the surface, exposure applied), darkened by ambient occlusion `ao` by `[diffuse, specular]` occlusion. The viewer looks straight at the surface, so what it reflects is the light around the normal
    pub fn shade(albedo: [f32; 3], light: Vec3, ao: f32, [diffuse_occlusion, specular_occlusion]: [f32; 2], channels: &[[f32; 3]]) -> [f32; 3] {
        let unit = |x: f32| if x.is_finite() { x.clamp(0.0, 1.0) } else { 0.0 };
        let ao = if ao.is_finite() { ao.clamp(0.0, 1.0) } else { 1.0 };
        let metal = unit(channels.get(Channel::Metallic.index()).map_or(0.0, |m| m[0]));
        let f0 = mix3([DIELECTRIC_REFLECTANCE; 3], albedo, metal);
        let diffuse = 1.0 + (ao - 1.0) * unit(diffuse_occlusion);
        let specular = 1.0 + (ao - 1.0) * unit(specular_occlusion);
        let light = light.to_array();
        std::array::from_fn(|k| unit(albedo[k] * (1.0 - metal) * light[k] * diffuse + f0[k] * light[k] * specular))
    }
}

/// The coefficients of an imported equirect image, read every few texels
fn image_sh(image: &ImageRes) -> [[f32; 3]; 9] {
    let (w, h) = (image.width.clamp(1, IMAGE_WIDTH), image.height.clamp(1, IMAGE_WIDTH / 2));
    let linear = |v: f32| {
        if image.srgb {
            texturecraft_texel::srgb_to_linear(v.clamp(0.0, 1.0))
        } else if v.is_finite() {
            v.max(0.0)
        } else {
            0.0
        }
    };
    sh9(w, h, |x, y| {
        let (ix, iy) = (x * image.width / w, y * image.height / h);
        let texel = image.rgba.get((iy as usize) * image.width as usize + ix as usize).copied().unwrap_or([0.0; 4]);
        Vec3::new(linear(texel[0]), linear(texel[1]), linear(texel[2]))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A uniform environment lights every direction alike, whatever the turn; brighter skies light more
    #[test]
    fn a_uniform_image_lights_every_way_alike() {
        let image = |v: f32| ImageRes { width: 32, height: 16, rgba: vec![[v, v, v, 1.0]; 32 * 16], srgb: false, mips: Default::default() };
        let sh = EnvironmentSh { coefficients: image_sh(&image(0.5)), rotation: 1.0 };
        for nrm in [Vec3::Y, Vec3::X, Vec3::NEG_Y, Vec3::new(0.6, 0.0, 0.8)] {
            let light = sh.irradiance(nrm, 0.3, 0.7);
            assert!((light - Vec3::splat(0.5)).abs().max_element() < 0.02, "{nrm}: {light}");
        }
    }

    /// The studio's key light is up and to one side: a surface facing up gets more light than one facing down, and turning the environment moves the light around
    #[test]
    fn a_built_in_lights_by_where_the_surface_faces_and_turns() {
        let display = DisplaySettings { environment: "Studio Soft".into(), ..DisplaySettings::default() };
        let sh = EnvironmentSh::of(&display, &ImageStore::default());
        assert!(sh.irradiance(Vec3::Y, 0.0, 0.0).element_sum() > 2.0 * sh.irradiance(Vec3::NEG_Y, 0.0, 0.0).element_sum());
        let side = Vec3::new(0.7, 0.0, 0.7).normalize();
        let (here, turned) = (sh.irradiance(side, 0.0, 0.0), sh.irradiance(side, std::f32::consts::PI, 0.0));
        assert!((here - turned).abs().max_element() > 0.05, "{here} vs {turned}");
        let night = EnvironmentSh::of(&DisplaySettings { environment: "Night".into(), ..DisplaySettings::default() }, &ImageStore::default());
        assert!(night.irradiance(Vec3::Y, 0.0, 0.0).element_sum() < sh.irradiance(Vec3::Y, 0.0, 0.0).element_sum(), "the cache follows the environment");
    }

    /// Diffuse occlusion darkens non-metals by the ambient occlusion; metals take their light from the reflections, which only specular occlusion darkens
    #[test]
    fn occlusion_darkens_the_diffuse_and_the_reflections_apart() {
        let mut channels: Vec<[f32; 3]> = Channel::ALL.iter().map(|c| c.default_value()).collect();
        let light = Vec3::splat(1.0);
        let dielectric = |occlusion| EnvironmentSh::shade([0.5; 3], light, 0.2, occlusion, &channels)[0];
        assert!(dielectric([1.0, 0.0]) < dielectric([0.0, 0.0]) * 0.5);
        channels[Channel::Metallic.index()] = [1.0; 3];
        let metal = |occlusion| EnvironmentSh::shade([0.5; 3], light, 0.2, occlusion, &channels)[0];
        assert_eq!(metal([1.0, 0.0]), metal([0.0, 0.0]));
        assert!(metal([0.0, 1.0]) < metal([0.0, 0.0]) * 0.5);
    }
}
