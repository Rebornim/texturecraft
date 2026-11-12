//! A generator's texture (Use Texture): where it is read, how it turns gray and how it blends over the signal

use texturecraft_core::{TextureBlend, TextureLayer};
use texturecraft_geom::glam::{Vec2, Vec3};

/// Exponent of the triplanar weights per unit of `1 / (1 − triplanar contrast)`: contrast 0 weighs the axes by the normal itself, 0.5 by its cube
const TRIPLANAR_SHARPNESS_POWER: f32 = 1.58;

/// Smallest normal component still dividing the weights, so a zero normal weighs the axes evenly
const MIN_NORMAL_COMPONENT: f32 = 1e-6;

/// Where the texture is read in UV: scaled about the centre of the UV square
pub(crate) fn uv_coordinates(texture: &TextureLayer, uv: Vec2) -> Vec2 {
    (uv - Vec2::splat(0.5)) * texture.scale + Vec2::splat(0.5)
}

/// Where the texture is read on each of the three projections (facing X, Y and Z) for `position` across the mesh's bounds (0..1), scaled about their centre
pub(crate) fn triplanar_coordinates(texture: &TextureLayer, position: Vec3) -> [Vec2; 3] {
    let scaled = |u: f32, v: f32| (Vec2::new(u, v) - Vec2::splat(0.5)) * texture.scale + Vec2::splat(0.5);
    [scaled(-position.z, position.y), scaled(position.x, position.z), scaled(position.x, position.y)]
}

/// How much each projection (facing X, Y and Z) counts for a surface facing `normal`, summing to 1
pub(crate) fn triplanar_weights(texture: &TextureLayer, normal: Vec3) -> [f32; 3] {
    let contrast = texture.triplanar_contrast.clamp(*TextureLayer::TRIPLANAR_CONTRAST_RANGE.start(), *TextureLayer::TRIPLANAR_CONTRAST_RANGE.end());
    let sharpness = (1.0 - contrast).max(f32::EPSILON).powf(-TRIPLANAR_SHARPNESS_POWER);
    let facing = normal.abs();
    let largest = facing.max_element().max(MIN_NORMAL_COMPONENT);
    let weights = (facing / largest).powf(sharpness);
    let total = weights.x + weights.y + weights.z;
    if total > 0.0 { (weights / total).to_array() } else { [1.0 / 3.0; 3] }
}

/// The texture's gray from its color: converted, inverted, contrasted, then brightened
pub(crate) fn texture_gray(texture: &TextureLayer, rgb: [f32; 3]) -> f32 {
    let gray = texture.grayscale.gray(rgb).clamp(0.0, 1.0);
    let gray = if texture.invert { 1.0 - gray } else { gray };

    let contrast = texture.contrast.clamp(-1.0, 1.0);
    let gray = if contrast > 0.0 {
        let range = (1.0 - contrast).max(f32::EPSILON);
        ((gray - 0.5) / range + 0.5).clamp(0.0, 1.0)
    } else {
        (gray - 0.5) * (1.0 + contrast) + 0.5
    };

    let brightness = texture.brightness.clamp(-1.0, 1.0);
    if brightness > 0.0 { gray + brightness * (1.0 - gray) } else { gray * (1.0 + brightness) }
}

/// `gray` (the texture) blended over `signal` by the texture's blending mode and opacity
pub(crate) fn blended_over(texture: &TextureLayer, signal: f32, gray: f32) -> f32 {
    let blend = match texture.blend {
        TextureBlend::Normal => gray,
        TextureBlend::Add => signal + gray,
        TextureBlend::Subtract => signal - gray,
        TextureBlend::Multiply => signal * gray,
        TextureBlend::AddSub => signal + 2.0 * gray - 1.0,
        TextureBlend::Max => signal.max(gray),
        TextureBlend::Min => signal.min(gray),
        TextureBlend::Overlay => {
            if signal < 0.5 {
                2.0 * signal * gray
            } else {
                1.0 - 2.0 * (1.0 - signal) * (1.0 - gray)
            }
        }
        TextureBlend::Screen => 1.0 - (1.0 - signal) * (1.0 - gray),
        TextureBlend::SoftLight => {
            if gray < 0.5 {
                2.0 * signal * gray + signal * signal * (1.0 - 2.0 * gray)
            } else {
                2.0 * signal * (1.0 - gray) + signal.max(0.0).sqrt() * (2.0 * gray - 1.0)
            }
        }
    };
    let opacity = texture.opacity.clamp(0.0, 1.0);
    (signal + (blend - signal) * opacity).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use texturecraft_core::GrayscaleConversion;

    fn texture() -> TextureLayer {
        TextureLayer { enabled: true, opacity: 1.0, ..TextureLayer::default() }
    }

    #[test]
    fn overlay_at_full_opacity_darkens_darks_and_lightens_lights() {
        let texture = texture();
        assert!((blended_over(&texture, 0.25, 0.5) - 0.25).abs() < 1e-6, "mid gray leaves a dark signal unchanged");
        assert!(blended_over(&texture, 0.25, 0.2) < 0.25);
        assert!(blended_over(&texture, 0.75, 0.8) > 0.75);
    }

    #[test]
    fn opacity_zero_leaves_the_signal() {
        let texture = TextureLayer { opacity: 0.0, blend: TextureBlend::Normal, ..texture() };
        assert_eq!(blended_over(&texture, 0.3, 0.9), 0.3);
    }

    #[test]
    fn every_blend_stays_in_range() {
        for blend in TextureBlend::ALL {
            let texture = TextureLayer { blend, ..texture() };
            for signal in [0.0, 0.3, 0.5, 1.0] {
                for gray in [0.0, 0.4, 1.0] {
                    let value = blended_over(&texture, signal, gray);
                    assert!((0.0..=1.0).contains(&value), "{blend:?} {signal} {gray} -> {value}");
                }
            }
        }
    }

    #[test]
    fn gray_is_converted_then_inverted_contrasted_and_brightened() {
        let red = [1.0, 0.0, 0.0];
        assert_eq!(texture_gray(&TextureLayer { grayscale: GrayscaleConversion::Red, ..texture() }, red), 1.0);
        assert_eq!(texture_gray(&TextureLayer { grayscale: GrayscaleConversion::Green, ..texture() }, red), 0.0);
        assert!((texture_gray(&TextureLayer { grayscale: GrayscaleConversion::Desaturation, ..texture() }, red) - 0.5).abs() < 1e-6);
        let inverted = TextureLayer { grayscale: GrayscaleConversion::Red, invert: true, ..texture() };
        assert_eq!(texture_gray(&inverted, red), 0.0);
        let contrasted = TextureLayer { contrast: 0.5, ..texture() };
        assert!((texture_gray(&contrasted, [0.6; 3]) - 0.7).abs() < 1e-5, "contrast 0.5 doubles the distance from the middle");
        let flattened = TextureLayer { contrast: -1.0, ..texture() };
        assert!((texture_gray(&flattened, [0.9; 3]) - 0.5).abs() < 1e-6, "contrast −1 is flat gray");
        let brightened = TextureLayer { brightness: 0.5, ..texture() };
        assert!((texture_gray(&brightened, [0.4; 3]) - 0.7).abs() < 1e-6);
    }

    #[test]
    fn triplanar_weights_follow_the_normal_and_sharpen_with_contrast() {
        let along_y = triplanar_weights(&texture(), Vec3::Y);
        assert!((along_y[1] - 1.0).abs() < 1e-6);
        let diagonal = Vec3::new(1.0, 0.5, 0.0).normalize();
        let soft = triplanar_weights(&TextureLayer { triplanar_contrast: 0.001, ..texture() }, diagonal);
        let hard = triplanar_weights(&TextureLayer { triplanar_contrast: 0.9, ..texture() }, diagonal);
        assert!(hard[0] > soft[0] && hard[1] < soft[1]);
        assert!((soft.iter().sum::<f32>() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn scale_is_about_the_centre() {
        let texture = TextureLayer { scale: 2.0, ..texture() };
        assert_eq!(uv_coordinates(&texture, Vec2::splat(0.5)), Vec2::splat(0.5));
        assert_eq!(uv_coordinates(&texture, Vec2::new(0.75, 0.25)), Vec2::new(1.0, 0.0));
    }
}
