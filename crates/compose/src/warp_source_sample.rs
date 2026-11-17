//! A warp's or slope blur's source as read around one texel

use texturecraft_core::{WarpSource, WarpSourceMode, lum};

use crate::eval::{MIN_ALPHA, texel_noise};
use crate::warp_texel::{Input, MIN_READ_WEIGHT, Sample};

/// A warp's or slope blur's source at one texel's neighbourhood
pub(crate) struct WarpSourceSample<'a> {
    pub(crate) source: &'a WarpSource,
    /// The default noise's seed
    pub(crate) seed: u32,
    pub(crate) res: u32,
    pub(crate) comps: usize,
    pub(crate) sample: Sample<'a>,
    pub(crate) input: Input<'a>,
}

impl WarpSourceSample<'_> {
    /// The source at texel position `(x, y)` after its blur, Balance and Contrast
    pub(crate) fn at(&self, x: f32, y: f32) -> f32 {
        let half = 0.5 * self.source.blur_texels(self.res);
        let value = if half > 0.0 {
            let corners = [(-half, -half), (half, -half), (-half, half), (half, half)];
            let (sum, count) = corners.iter().filter_map(|(dx, dy)| self.raw(x + dx, y + dy)).fold((0.0f32, 0u8), |(sum, count), read| (sum + read, count + 1));
            if count == 0 { 0.5 } else { sum / f32::from(count) }
        } else {
            self.raw(x, y).unwrap_or(0.5)
        };
        self.source.shape(value)
    }

    /// The source at texel position `(x, y)` before it is shaped; None where the values it filters cover nothing
    fn raw(&self, x: f32, y: f32) -> Option<f32> {
        match self.source.mode {
            WarpSourceMode::DefaultNoise => Some(WarpSource::stretch_noise(texel_noise(x, y, [0.0; 2], self.res, self.source.noise_scale(), self.seed))),
            WarpSourceMode::PreviousInput => {
                let mut read = [0.0f32; 5];
                (self.sample)(x, y, &mut read);
                match (self.comps, read) {
                    // Content stays premultiplied by its own coverage
                    (4, [r, g, b, alpha, _]) if alpha > MIN_ALPHA => Some(lum([r / alpha, g / alpha, b / alpha])),
                    (1, [value, .., weight]) if weight > MIN_READ_WEIGHT => Some(value / weight),
                    _ => None,
                }
            }

            // Mid gray (its default) moves nothing
            WarpSourceMode::CustomNoise => (self.input)(x, y),
        }
    }
}
