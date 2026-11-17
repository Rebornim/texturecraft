use serde::{Deserialize, Serialize};

/// The UV Random Color generator's controls: one random value per UV island, picked at random or along a gradient
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UvRandomColorParameters {
    /// Pick each island's color along [`UvRandomColorParameters::gradient`] instead of at random
    pub custom_gradient: bool,
    /// The gradient's colors, evenly spaced (linear RGB, [`UvRandomColorParameters::MAX_GRADIENT_COLORS`] at most)
    pub gradient: Vec<[f32; 3]>,
}

impl UvRandomColorParameters {
    pub const MAX_GRADIENT_COLORS: usize = 16;

    /// Every color brought into 0..1 (non-finite components black), at most [`UvRandomColorParameters::MAX_GRADIENT_COLORS`] of them, and at least one
    pub fn clamped(self) -> UvRandomColorParameters {
        let unit = |value: f32| if value.is_finite() { value.clamp(0.0, 1.0) } else { 0.0 };
        let gradient: Vec<[f32; 3]> = self.gradient.into_iter().take(UvRandomColorParameters::MAX_GRADIENT_COLORS).map(|color| color.map(unit)).collect();
        UvRandomColorParameters { gradient: if gradient.is_empty() { UvRandomColorParameters::default().gradient } else { gradient }, ..self }
    }
}

impl Default for UvRandomColorParameters {
    /// Random colors; the gradient black to white
    fn default() -> UvRandomColorParameters {
        UvRandomColorParameters { custom_gradient: false, gradient: vec![[0.0; 3], [1.0; 3]] }
    }
}
