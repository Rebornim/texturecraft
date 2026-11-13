//! The Bevel Smooth filter's settings and the ramp it draws from a shape's edge

use serde::{Deserialize, Serialize};

use crate::BevelDirection;

/// A smooth ramp from the edge of the shapes (values at or above `threshold`), outwards, inwards or across it
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BevelSmooth {
    pub direction: BevelDirection,
    /// How far the ramp reaches from the edge, a fraction of the UV tile (0..0.3)
    pub distance: f32,
    /// How much the ramp is blurred (0..10, [`BevelSmooth::SMOOTHING_TEXELS`] texels at 2K each)
    pub smoothing: f32,
    /// Moves the ramp up or down before its curve (−1..1)
    pub curve_offset: f32,
    /// Bends the ramp: below 0 it rises later, above 0 sooner (−1..1)
    pub curve_shape: f32,
    /// Values at or above it are inside the shapes (Mask Threshold, 0..1)
    pub threshold: f32,
    /// Scales the distance (Distance Map Multiplier, 0..1)
    pub multiplier: f32,
    /// Reads past the texture's edges from the far side (Bevel Wrap)
    pub wrap: bool,
}

impl BevelSmooth {
    /// The Bevel Smooth filter as the library adds it
    pub const LIBRARY: BevelSmooth = BevelSmooth {
        direction: BevelDirection::Out,
        distance: 0.1,
        smoothing: 1.0,
        curve_offset: 0.0,
        curve_shape: 0.0,
        threshold: 0.5,
        multiplier: 1.0,
        wrap: false,
    };

    /// Texels at 2K a unit of Smoothing blurs over
    pub const SMOOTHING_TEXELS: f32 = 2.0;

    /// Whether `value` is inside the shapes
    pub fn inside(&self, value: f32) -> bool {
        let threshold = if self.threshold.is_finite() { self.threshold.clamp(0.0, 1.0) } else { 0.5 };
        value >= threshold - f32::EPSILON
    }

    /// How many texels the ramp reaches at `res` texels across a UV tile
    pub fn reach(&self, res: u32) -> f32 {
        let unit = |x: f32| if x.is_finite() { x.clamp(0.0, 1.0) } else { 0.0 };
        unit(self.distance) * unit(self.multiplier) * res as f32
    }

    /// How many texels the ramp is blurred over at `res` texels across a UV tile
    pub fn smoothing_texels(&self, res: u32) -> f32 {
        let smoothing = if self.smoothing.is_finite() { self.smoothing.clamp(0.0, 10.0) } else { 0.0 };
        smoothing * Self::SMOOTHING_TEXELS * res as f32 / 2048.0
    }

    /// The ramp at a texel `edge` texels from the shapes' edge, inside them or not
    pub fn value(&self, inside: bool, edge: f32, reach: f32) -> f32 {
        let along = if reach > 0.0 && edge.is_finite() { (edge / reach).clamp(0.0, 1.0) } else { 1.0 };
        let offset = if self.curve_offset.is_finite() { self.curve_offset.clamp(-1.0, 1.0) } else { 0.0 };
        let shape = if self.curve_shape.is_finite() { self.curve_shape.clamp(-1.0, 1.0) } else { 0.0 };
        let curved = (along + offset).clamp(0.0, 1.0).powf(4f32.powf(-shape));
        match (self.direction, inside) {
            (BevelDirection::Out, true) => 1.0,
            (BevelDirection::Out, false) => 1.0 - curved,
            (BevelDirection::In, true) => curved,
            (BevelDirection::In, false) => 0.0,
            (BevelDirection::InOut, true) => 0.5 + 0.5 * curved,
            (BevelDirection::InOut, false) => 0.5 - 0.5 * curved,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ramp_lies_on_its_direction_side() {
        let out = BevelSmooth::LIBRARY;
        assert_eq!(out.reach(2048), 204.8);
        assert!(out.inside(0.5) && !out.inside(0.49));
        assert_eq!((out.value(true, 3.0, 10.0), out.value(false, 0.0, 10.0), out.value(false, 5.0, 10.0), out.value(false, 20.0, 10.0)), (1.0, 1.0, 0.5, 0.0));
        let inward = BevelSmooth { direction: BevelDirection::In, ..out };
        assert_eq!((inward.value(false, 3.0, 10.0), inward.value(true, 5.0, 10.0), inward.value(true, 20.0, 10.0)), (0.0, 0.5, 1.0));
        let both = BevelSmooth { direction: BevelDirection::InOut, ..out };
        assert_eq!(
            (both.value(true, 0.0, 10.0), both.value(false, 0.0, 10.0), both.value(true, 10.0, 10.0), both.value(false, 10.0, 10.0)),
            (0.5, 0.5, 1.0, 0.0)
        );
    }

    #[test]
    fn curve_offset_and_shape_bend_the_ramp() {
        let inward = BevelSmooth { direction: BevelDirection::In, ..BevelSmooth::LIBRARY };
        assert_eq!(BevelSmooth { curve_offset: 0.25, ..inward }.value(true, 5.0, 10.0), 0.75);
        assert!(BevelSmooth { curve_shape: 0.5, ..inward }.value(true, 5.0, 10.0) > 0.5, "rises sooner");
        assert!(BevelSmooth { curve_shape: -0.5, ..inward }.value(true, 5.0, 10.0) < 0.5, "rises later");
        assert_eq!(BevelSmooth { multiplier: 0.5, ..inward }.reach(2048), 102.4);
        let hostile = BevelSmooth { distance: f32::NAN, curve_offset: f32::NAN, curve_shape: f32::INFINITY, threshold: f32::NAN, ..inward };
        assert_eq!((hostile.reach(2048), hostile.value(true, 1.0, 0.0)), (0.0, 1.0));
    }
}
