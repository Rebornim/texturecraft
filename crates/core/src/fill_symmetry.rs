//! Symmetry of a fill's 3D projection: mirrored or radial copies of it on the model

use serde::{Deserialize, Serialize};

use crate::SymmetryKind;

/// Copies of a fill's projection: off for every fill saved before symmetry existed
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FillSymmetry {
    pub on: bool,
    pub kind: SymmetryKind,
    /// 0 = X, 1 = Y, 2 = Z: the mirror plane's normal, or the axis radial copies turn round
    pub axis: u8,
    /// Whether the copies' images are turned over across (U) and down (V)
    pub flip: [bool; 2],
    /// Where the mirror plane or the turning axis passes, in mesh radii from the mesh's centre
    pub position: [f32; 3],
    /// How many radial copies, the original included
    pub count: u32,
    /// The degrees the radial copies spread over; a full turn spaces them evenly round it
    pub span: f32,
}

impl FillSymmetry {
    /// Off; switched on, a mirror across X through the mesh's centre, or six radial copies over a full turn
    pub const OFF: FillSymmetry = FillSymmetry { on: false, kind: SymmetryKind::Mirror, axis: 0, flip: [false; 2], position: [0.0; 3], count: 6, span: 360.0 };
    /// The fewest and most radial copies
    pub const COUNTS: std::ops::RangeInclusive<u32> = 2..=64;
    /// How far the axis position reaches from the mesh's centre, in mesh radii
    pub const REACH: f32 = 1.0;

    /// The degrees between radial copies: a full turn spaces `count` copies evenly, a smaller span puts the first and last at its ends
    pub fn step(&self) -> f32 {
        let count = self.count.clamp(*Self::COUNTS.start(), *Self::COUNTS.end()) as f32;
        let span = self.span.clamp(0.0, 360.0);
        if span >= 360.0 { span / count } else { span / (count - 1.0) }
    }

    /// The radial copies there are, the original included
    pub fn copies(&self) -> u32 {
        match self.kind {
            SymmetryKind::Mirror => 2,
            SymmetryKind::Radial => self.count.clamp(*Self::COUNTS.start(), *Self::COUNTS.end()),
        }
    }

    /// Whether this is the symmetry a new fill starts with, so saves leave it out
    pub fn is_default(&self) -> bool {
        *self == FillSymmetry::default()
    }
}

impl Default for FillSymmetry {
    fn default() -> Self {
        FillSymmetry::OFF
    }
}
