//! Gradient's Input Transformation: how a texel becomes the position it reads on the ramp

use serde::{Deserialize, Serialize};

use crate::grayscale_mode::GrayscaleMode;

/// Full contrast stays a finite step instead of a division by zero
const MIN_STEEPNESS: f32 = 1e-3;

/// The gray, blurred, balanced, contrasted and maybe inverted value a gradient looks up
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GradientInput {
    /// How a color becomes gray (masks and scalar channels are gray already)
    pub mode: GrayscaleMode,
    /// Blurs the gray before the lookup, in texels at 2K (0..16)
    pub blur: f32,
    /// Shifts the gray: above 0.5 lighter, below darker
    pub balance: f32,
    /// Steepens the gray around its middle: 1 is a hard step
    pub contrast: f32,
    pub invert: bool,
}

impl Default for GradientInput {
    /// What gradients read in projects saved before the Input Transformation: luma, untouched
    fn default() -> Self {
        GradientInput { mode: GrayscaleMode::Luma, blur: 0.0, balance: 0.5, contrast: 0.0, invert: false }
    }
}

impl GradientInput {
    /// The library's starting transformation: the average of the components, untouched
    pub const LIBRARY: GradientInput = GradientInput { mode: GrayscaleMode::Average, blur: 0.0, balance: 0.5, contrast: 0.0, invert: false };

    /// Whether it reads gradients as projects saved before it did (skipped when saving)
    pub fn is_unchanged(&self) -> bool {
        *self == GradientInput::default()
    }

    /// The ramp position of gray `x`
    pub fn shape(&self, x: f32) -> f32 {
        shape(x, self.balance, self.contrast, self.invert)
    }
}

/// Gray `x` balanced (0.5 keeps it), contrasted around mid gray (0 keeps it, 1 is a step at 0.5) and maybe inverted, in 0..1
pub fn shape(x: f32, balance: f32, contrast: f32, invert: bool) -> f32 {
    let unit = |v: f32| if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 };
    let balanced = unit(x) + unit(balance) - 0.5;
    let steep = (1.0 - unit(contrast)).max(MIN_STEEPNESS);
    let y = unit((balanced - 0.5) / steep + 0.5);
    if invert { 1.0 - y } else { y }
}

/// The color at `t` on a ramp of `colors` placed at `positions` (each 0..1, evenly spaced where missing), blended in linear or sRGB
pub fn ramp(colors: &[[f32; 3]], positions: &[f32], linear: bool, t: f32) -> Option<[f32; 3]> {
    let last = colors.len().checked_sub(1)?;
    let even = |index: usize| if last == 0 { 0.0 } else { index as f32 / last as f32 };
    let mut stops: Vec<(f32, [f32; 3])> = colors
        .iter()
        .enumerate()
        .map(|(index, color)| (positions.get(index).copied().filter(|p| p.is_finite()).unwrap_or_else(|| even(index)).clamp(0.0, 1.0), *color))
        .collect();
    stops.sort_by(|a, b| a.0.total_cmp(&b.0));
    let t = if t.is_finite() { t.clamp(0.0, 1.0) } else { 0.0 };
    let after = stops.partition_point(|(position, _)| *position <= t);
    let (Some(&(p0, c0)), Some(&(p1, c1))) = (stops.get(after.saturating_sub(1)), stops.get(after.min(last))) else {
        return None;
    };

    let share = if p1 > p0 { ((t - p0) / (p1 - p0)).clamp(0.0, 1.0) } else { 0.0 };
    Some(if linear {
        std::array::from_fn(|k| c0[k] + (c1[k] - c0[k]) * share)
    } else {
        std::array::from_fn(|k| srgb_to_linear(linear_to_srgb(c0[k]) + (linear_to_srgb(c1[k]) - linear_to_srgb(c0[k])) * share))
    })
}

/// sRGB transfer of a linear component
pub(crate) fn linear_to_srgb(v: f32) -> f32 {
    let v = v.clamp(0.0, 1.0);
    if v <= 0.003_130_8 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 }
}

/// Linear value of an sRGB component
pub(crate) fn srgb_to_linear(v: f32) -> f32 {
    let v = v.clamp(0.0, 1.0);
    if v <= 0.040_45 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
}
