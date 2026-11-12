//! The Material section's orb: a lit sphere in the fill's base colour, metallic and roughness, rendered once per look and size

use egui::{Color32, ColorImage, Painter, Rect, TextureHandle, TextureOptions, pos2};
use texturecraft_engine::texel::linear_to_srgb;

use crate::theme::Tokens;

/// Largest orb rendered, in physical pixels
const MAX_SIDE_PX: usize = 256;

/// The sphere's radius in halves of the square
const SPHERE_RADIUS: f32 = 0.84;

/// Where the light comes from: upper left, in front (normalised below)
const LIGHT: [f32; 3] = [-0.55, 0.6, 0.58];

/// Light falling on the sphere and the ambient share that fills its dark side
const LIGHT_STRENGTH: f32 = 2.6;
const AMBIENT: f32 = 0.18;

/// Reflectance of a non-metal seen face on
const DIELECTRIC_F0: f32 = 0.04;

/// Samples per pixel along each side
const SUPERSAMPLE: usize = 2;

/// What the orb shows: linear base colour, metallic and roughness
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct OrbLook {
    pub(crate) base: [f32; 3],
    pub(crate) metallic: f32,
    pub(crate) roughness: f32,
}

/// Draws the orb for `look` into `rect` on the panel's darkest backdrop
pub(crate) fn material_orb(painter: &Painter, rect: Rect, look: OrbLook, tokens: &Tokens) {
    let ctx = painter.ctx();
    let side = ((rect.width() * ctx.pixels_per_point()).round() as usize).clamp(1, MAX_SIDE_PX);
    let key = format!("{side}{look:?}{:?}", tokens.chrome);
    let id = egui::Id::unique(("material_orb", side));
    // The picture and the key it was rendered for
    let cached = ctx.data(|data| data.get_temp::<(String, TextureHandle)>(id)).filter(|(rendered, _)| *rendered == key);
    let texture = match cached {
        Some((_, texture)) => texture,
        None => {
            let texture = ctx.load_texture("material-orb", render(look, side, tokens.chrome), TextureOptions::LINEAR);
            ctx.data_mut(|data| data.insert_temp(id, (key, texture.clone())));
            texture
        }
    };
    painter.image(texture.id(), rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
}

fn render(look: OrbLook, side: usize, backdrop: Color32) -> ColorImage {
    let light = normalised(LIGHT);
    let base = look.base.map(|c| c.clamp(0.0, 1.0));
    let metallic = look.metallic.clamp(0.0, 1.0);
    let roughness = look.roughness.clamp(0.04, 1.0);
    let diffuse = base.map(|c| c * (1.0 - metallic));
    let f0 = base.map(|c| DIELECTRIC_F0 + (c - DIELECTRIC_F0) * metallic);
    let back = [backdrop.r(), backdrop.g(), backdrop.b()].map(|c| f32::from(c) / 255.0);
    let samples = (SUPERSAMPLE * SUPERSAMPLE) as f32;
    let mut pixels = Vec::with_capacity(side * side);
    for y in 0..side {
        for x in 0..side {
            let mut sum = [0.0_f32; 3];
            for sy in 0..SUPERSAMPLE {
                for sx in 0..SUPERSAMPLE {
                    let u = ((x * SUPERSAMPLE + sx) as f32 + 0.5) / (side * SUPERSAMPLE) as f32 * 2.0 - 1.0;
                    let v = 1.0 - ((y * SUPERSAMPLE + sy) as f32 + 0.5) / (side * SUPERSAMPLE) as f32 * 2.0;
                    let shade = shade_at(u / SPHERE_RADIUS, v / SPHERE_RADIUS, light, diffuse, f0, roughness);
                    let color = shade.map_or(back, |linear| linear.map(|c| linear_to_srgb(c / (1.0 + c))));
                    for (total, part) in sum.iter_mut().zip(color) {
                        *total += part;
                    }
                }
            }
            let [r, g, b] = sum.map(|c| (c / samples * 255.0).round().clamp(0.0, 255.0) as u8);
            pixels.push(Color32::from_rgb(r, g, b));
        }
    }
    ColorImage::new([side, side], pixels)
}

/// The linear light leaving the sphere at (`u`, `v`) in sphere radii (None off the sphere): lambert diffuse, a GGX highlight and an ambient fill
fn shade_at(u: f32, v: f32, light: [f32; 3], diffuse: [f32; 3], f0: [f32; 3], roughness: f32) -> Option<[f32; 3]> {
    let z2 = 1.0 - u * u - v * v;
    if z2 <= 0.0 {
        return None;
    }
    let normal = [u, v, z2.sqrt()];
    let n_dot_l = dot(normal, light).max(0.0);
    let half = normalised([light[0], light[1], light[2] + 1.0]);
    let n_dot_h = dot(normal, half).max(0.0);
    let alpha = roughness * roughness;
    let alpha2 = alpha * alpha;
    let denominator = n_dot_h * n_dot_h * (alpha2 - 1.0) + 1.0;
    let highlight = alpha2 / (std::f32::consts::PI * denominator * denominator) * 0.25;
    // The sky above lights the upper half more than the ground lights the lower
    let sky = AMBIENT * (0.6 + 0.4 * normal[1]);
    let mut out = [0.0; 3];
    for (k, value) in out.iter_mut().enumerate() {
        *value = diffuse[k] * (n_dot_l * LIGHT_STRENGTH / std::f32::consts::PI + sky) + f0[k] * (highlight * n_dot_l * LIGHT_STRENGTH + sky * 2.0);
    }
    Some(out)
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn normalised(v: [f32; 3]) -> [f32; 3] {
    let length = dot(v, v).sqrt().max(f32::EPSILON);
    v.map(|c| c / length)
}

#[cfg(test)]
mod tests {
    use super::*;

    const COPPER: OrbLook = OrbLook { base: [0.8, 0.35, 0.18], metallic: 1.0, roughness: 0.35 };

    #[test]
    fn the_orb_sits_on_its_backdrop_and_shows_its_colour() {
        let backdrop = Color32::from_rgb(27, 31, 39);
        let image = render(COPPER, 64, backdrop);
        assert_eq!(image.pixels[0], backdrop, "the corner is off the sphere");
        let middle = image.pixels[32 * 64 + 32];
        assert!(middle.r() > middle.b(), "copper is warm: {middle:?}");
    }

    #[test]
    fn rougher_materials_spread_their_highlight() {
        let peak = |roughness: f32| {
            let image = render(OrbLook { roughness, ..COPPER }, 64, Color32::BLACK);
            image.pixels.iter().map(|pixel| pixel.r()).max().unwrap_or(0)
        };
        assert!(peak(0.2) > peak(0.9), "a polished orb has a brighter hot spot");
    }
}
