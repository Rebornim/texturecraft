use serde::{Deserialize, Serialize};

use super::resolution_mode::ResolutionMode;

/// The resolution a generator or pattern computes its output at, before it is sampled onto the texture set.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputResolution {
    pub mode: ResolutionMode,
    /// Texels per side for [`ResolutionMode::Custom`].
    pub custom: u32,
    /// Power of two applied to the base size (−3 = 1/8 … 3 = ×8; 0 = the same as the context).
    pub factor: i8,
}

impl OutputResolution {
    pub const FACTORS: std::ops::RangeInclusive<i8> = -3..=3;
    pub const CUSTOM_SIZES: [u32; 6] = [128, 256, 512, 1024, 2048, 4096];
    pub const MAX_SIZE: u32 = 8192;

    pub fn is_default(&self) -> bool {
        *self == OutputResolution::default()
    }

    /// Texels per side of the output when the context (texture set or UV tile) is `context` texels per side.
    pub fn output_size(&self, context: u32) -> u32 {
        let base = match self.mode {
            ResolutionMode::Auto | ResolutionMode::TextureSet | ResolutionMode::UvTile => context,
            ResolutionMode::Custom => self.custom,
        };
        let factor = self.factor.clamp(*OutputResolution::FACTORS.start(), *OutputResolution::FACTORS.end());
        let scaled = if factor >= 0 { base.saturating_mul(1 << factor) } else { base >> -factor };
        scaled.clamp(1, OutputResolution::MAX_SIZE)
    }

    /// How many context texels one output texel covers per side (1 when the output is as fine or finer).
    pub fn cell(&self, context: u32) -> u32 {
        (context / self.output_size(context)).max(1)
    }
}

impl Default for OutputResolution {
    fn default() -> Self {
        OutputResolution { mode: ResolutionMode::Auto, custom: 2048, factor: 0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_follows_the_context() {
        let resolution = OutputResolution::default();
        assert_eq!(resolution.output_size(1024), 1024);
        assert_eq!(resolution.cell(1024), 1);
    }

    #[test]
    fn factors_and_custom_sizes_scale_the_output() {
        let eighth = OutputResolution { factor: -3, ..OutputResolution::default() };
        assert_eq!((eighth.output_size(2048), eighth.cell(2048)), (256, 8));

        let custom = OutputResolution { mode: ResolutionMode::Custom, custom: 512, factor: 1 };
        assert_eq!((custom.output_size(2048), custom.cell(2048)), (1024, 2));

        let finer = OutputResolution { factor: 3, ..OutputResolution::default() };
        assert_eq!((finer.output_size(2048), finer.cell(2048)), (8192, 1), "a finer output than the texture set keeps one texel per texel");

        let hostile = OutputResolution { mode: ResolutionMode::Custom, custom: u32::MAX, factor: i8::MAX };
        assert_eq!(hostile.output_size(2048), OutputResolution::MAX_SIZE);
    }
}
