//! Color Match's settings: the colors moved so a source color lands on a target color

use serde::{Deserialize, Serialize};

use crate::{SourceColorMode, TargetColorMode, oklab};

/// A Color Match filter: every color moved, in OKLab, by the step from its source color to its target color
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColorMatch {
    /// Source Color Mode: the color moved from
    pub source_mode: SourceColorMode,
    /// Source Color (linear), read in Source Color Mode Parameter
    pub source_color: [f32; 3],
    /// Target Color Mode: the color moved to
    pub target_mode: TargetColorMode,
    /// Target Color (linear), read in Target Color Mode Parameter
    pub target_color: [f32; 3],
    /// Custom Color Variation: the colors keep at most Hue, Chroma and Luma of their difference from the source
    pub custom_variation: bool,
    /// Hue (degrees): how far a hue may stay from the target's
    pub hue: f32,
    /// Chroma: how far a chroma may stay from the target's
    pub chroma: f32,
    /// Luma: how far a lightness may stay from the target's
    pub luma: f32,
}

impl ColorMatch {
    /// Mid gray (128 sRGB) in linear
    const GRAY: [f32; 3] = [0.215_860_5; 3];

    /// A new Color Match
    pub const LIBRARY: ColorMatch = ColorMatch {
        source_mode: SourceColorMode::Average,
        source_color: Self::GRAY,
        target_mode: TargetColorMode::Parameter,
        target_color: Self::GRAY,
        custom_variation: true,
        hue: 0.0,
        chroma: 0.0,
        luma: 0.0,
    };

    /// Linear `color` moved by the step from `source` to `target`; without Custom Color Variation it keeps all of its difference from the source, with it at most Hue, Chroma and Luma of it (all 0: the target color itself). Hostile values give the color back
    pub fn apply(&self, color: [f32; 3], source: [f32; 3], target: [f32; 3]) -> [f32; 3] {
        let [lightness, a, b] = oklab::from_linear(color.map(|v| v.max(0.0)));
        let [source_lightness, source_a, source_b] = oklab::from_linear(source.map(|v| v.max(0.0)));
        let [target_lightness, target_a, target_b] = oklab::from_linear(target.map(|v| v.max(0.0)));
        let moved = if self.custom_variation {
            let limit = |value: f32, max: f32| if value.is_finite() { value.clamp(0.0, max) } else { 0.0 };
            let keep = |difference: f32, max: f32| difference.clamp(-max, max);
            let (chroma, hue) = (a.hypot(b), b.atan2(a));
            let (source_chroma, source_hue) = (source_a.hypot(source_b), source_b.atan2(source_a));
            let (target_chroma, target_hue) = (target_a.hypot(target_b), target_b.atan2(target_a));
            let turn = (hue - source_hue + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
            let out_lightness = target_lightness + keep(lightness - source_lightness, limit(self.luma, 1.0));
            let out_chroma = (target_chroma + keep(chroma - source_chroma, limit(self.chroma, 1.0))).max(0.0);
            let out_hue = target_hue + keep(turn, limit(self.hue, 180.0).to_radians());
            [out_lightness, out_chroma * out_hue.cos(), out_chroma * out_hue.sin()]
        } else {
            [target_lightness + lightness - source_lightness, target_a + a - source_a, target_b + b - source_b]
        };
        let out = oklab::to_linear(moved);
        if out.iter().all(|v| v.is_finite()) { out.map(|v| v.clamp(0.0, 1.0)) } else { color }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: [f32; 3], b: [f32; 3]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-3)
    }

    #[test]
    fn the_source_color_lands_on_the_target_and_variation_is_kept_as_asked() {
        let red = [0.6, 0.1, 0.05];
        let blue = [0.05, 0.1, 0.6];
        let lighter_red = [0.8, 0.2, 0.1];
        let keep_all = ColorMatch { custom_variation: false, ..ColorMatch::LIBRARY };
        assert!(near(keep_all.apply(red, red, blue), blue), "the source itself becomes the target");
        let moved = keep_all.apply(lighter_red, red, blue);
        assert!(oklab::from_linear(moved)[0] > oklab::from_linear(blue)[0] + 0.05, "a lighter texel stays lighter than the target");
        assert!(moved[2] > moved[0], "and turns blue");

        // Custom Color Variation at 0: every color is the target; wide limits keep the lightness difference again
        let flat = ColorMatch::LIBRARY;
        assert!(near(flat.apply(lighter_red, red, blue), blue));
        let luma = ColorMatch { luma: 1.0, chroma: 1.0, hue: 180.0, ..flat };
        assert!(oklab::from_linear(luma.apply(lighter_red, red, blue))[0] > oklab::from_linear(blue)[0] + 0.05);
        let some = ColorMatch { luma: 0.02, ..flat };
        let limited = oklab::from_linear(some.apply(lighter_red, red, blue))[0] - oklab::from_linear(blue)[0];
        assert!((limited - 0.02).abs() < 1e-3, "lightness kept up to Luma: {limited}");

        let hostile = ColorMatch { hue: f32::NAN, chroma: f32::INFINITY, luma: -1.0, ..flat };
        assert!(hostile.apply([f32::NAN, 0.5, 0.5], red, blue).iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v)));
    }
}
