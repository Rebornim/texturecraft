//! An image a filter reads besides the stack it filters (its image inputs), as a source on the filter's effect

use crate::{Filter, LayerId, Source, WarpSourceMode};

/// One image input of a filter: the key its source is stored under on the effect, its name and the gray it reads until a source is set
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImageInput {
    pub id: &'static str,
    pub label: &'static str,
    /// The gray the input reads while no source is set (what the filter did before it had the input)
    pub default: f32,
    /// Added by a mode of the filter (shown under that mode), not one of its Image inputs
    pub by_mode: bool,
}

impl ImageInput {
    /// Bevel Smooth's and Directional Distance's Distance Map: scales the distance per texel
    pub const DISTANCE_MAP: ImageInput = ImageInput { id: "distanceMap", label: "Distance Map", default: 1.0, by_mode: false };

    /// Warp's Custom Noise (Source Mode Custom Noise): the source its texels move by; mid gray moves nothing
    pub const CUSTOM_NOISE: ImageInput = ImageInput { id: "customNoise", label: "Custom Noise", default: 0.5, by_mode: true };

    /// The image inputs `filter` reads now (a mode's input only while that mode is on)
    pub fn of(filter: &Filter) -> Vec<ImageInput> {
        match filter {
            Filter::BevelSmooth(_) | Filter::DirectionalDistance(_) => vec![Self::DISTANCE_MAP],
            Filter::SourceWarp(warp) if warp.source.mode == WarpSourceMode::CustomNoise => vec![Self::CUSTOM_NOISE],
            _ => Vec::new(),
        }
    }

    /// The input of `filter` stored under `id`, also while its mode is off
    pub fn named(filter: &Filter, id: &str) -> Option<ImageInput> {
        let inputs = match filter {
            Filter::BevelSmooth(_) | Filter::DirectionalDistance(_) => &[Self::DISTANCE_MAP][..],
            Filter::SourceWarp(_) => &[Self::CUSTOM_NOISE][..],
            _ => &[],
        };
        inputs.iter().find(|input| input.id == id).copied()
    }

    /// The source the input reads while none is set
    pub fn default_source(&self) -> Source {
        Source::Uniform([self.default; 3])
    }

    /// Whether a source set on the input reads anchor point `anchor` (any anchor point when None)
    pub fn reads(source: &Source, anchor: Option<LayerId>) -> bool {
        source.anchor().is_some_and(|found| anchor.is_none_or(|wanted| wanted == found))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BevelSmooth, SourceWarp, WarpSource};

    #[test]
    fn a_warp_has_its_custom_noise_only_in_that_source_mode() {
        let warp = SourceWarp::LIBRARY;
        assert!(ImageInput::of(&Filter::SourceWarp(warp)).is_empty());
        let custom = SourceWarp { source: WarpSource { mode: WarpSourceMode::CustomNoise, ..warp.source }, ..warp };
        assert_eq!(ImageInput::of(&Filter::SourceWarp(custom)), vec![ImageInput::CUSTOM_NOISE]);
        assert_eq!(ImageInput::named(&Filter::SourceWarp(warp), "customNoise"), Some(ImageInput::CUSTOM_NOISE), "kept while the mode is off");
        assert_eq!(ImageInput::of(&Filter::BevelSmooth(BevelSmooth::LIBRARY)), vec![ImageInput::DISTANCE_MAP]);
        assert_eq!(ImageInput::named(&Filter::BevelSmooth(BevelSmooth::LIBRARY), "customNoise"), None);
        assert_eq!(ImageInput::DISTANCE_MAP.default_source(), Source::Uniform([1.0; 3]));
    }
}
