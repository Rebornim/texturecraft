//! Grayscale Conversion's settings: which gray it reads and how it shapes it

use serde::{Deserialize, Serialize};

use crate::ConversionMode;
use crate::blend::lum;
use crate::gradient_input::shape;

/// A Grayscale Conversion filter: the gray of the current channel (or of its Custom Input), balanced, contrasted and maybe inverted
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrayscaleConversion {
    /// Channel Input Custom Input: the gray of the filter's Custom Input instead of the channel's
    pub custom_input: bool,
    pub invert: bool,
    pub balance: f32,
    pub contrast: f32,
    pub mode: ConversionMode,
    /// Channels Weights' Red, Green, Blue and Alpha
    pub weights: [f32; 4],
}

impl GrayscaleConversion {
    /// A new Grayscale Conversion: a third of each color component, alpha left out
    pub const LIBRARY: GrayscaleConversion = GrayscaleConversion {
        custom_input: false,
        invert: false,
        balance: 0.5,
        contrast: 0.0,
        mode: ConversionMode::ChannelsWeights,
        weights: [0.33, 0.33, 0.33, 0.0],
    };

    /// The settings a Grayscale filter saved before them takes on its first edit: its luminance weights, so it looks the same
    pub fn of_luminance() -> GrayscaleConversion {
        let [red, green, blue] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]].map(lum);
        GrayscaleConversion { weights: [red, green, blue, 0.0], ..Self::LIBRARY }
    }

    /// The shaped gray of straight color `rgb` with coverage `alpha`
    pub fn gray(&self, rgb: [f32; 3], alpha: f32) -> f32 {
        shape(self.mode.gray(rgb, alpha, self.weights), self.balance, self.contrast, self.invert)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_saved_grayscale_keeps_its_luminance() {
        let color = [0.8, 0.3, 0.1];
        assert!((GrayscaleConversion::of_luminance().gray(color, 1.0) - lum(color)).abs() < 1e-5);
    }

    #[test]
    fn weights_balance_contrast_and_invert_shape_the_gray() {
        let only_red = GrayscaleConversion { weights: [1.0, 0.0, 0.0, 0.0], ..GrayscaleConversion::LIBRARY };
        assert!((only_red.gray([0.6, 1.0, 1.0], 1.0) - 0.6).abs() < 1e-6);
        let alpha = GrayscaleConversion { weights: [0.0, 0.0, 0.0, 1.0], ..GrayscaleConversion::LIBRARY };
        assert!((alpha.gray([1.0; 3], 0.25) - 0.25).abs() < 1e-6, "Alpha weighs the coverage");
        let inverted = GrayscaleConversion { invert: true, mode: ConversionMode::Max, ..GrayscaleConversion::LIBRARY };
        assert!((inverted.gray([0.2, 0.7, 0.1], 1.0) - 0.3).abs() < 1e-6);
        let contrasted = GrayscaleConversion { contrast: 1.0, mode: ConversionMode::Average, ..GrayscaleConversion::LIBRARY };
        assert_eq!(contrasted.gray([0.6; 3], 1.0), 1.0);
    }
}
