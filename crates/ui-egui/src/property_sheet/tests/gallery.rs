//! What the gallery's controls last set

use super::Filtering;

/// The gallery's controls, with the values they last set
pub(super) struct Gallery {
    pub(super) scale: usize,
    pub(super) filtering: Filtering,
    pub(super) hardness: f32,
    pub(super) hardness_merges: Vec<bool>,
    pub(super) offset: f32,
    pub(super) rotation: f32,
    pub(super) position: [f32; 3],
    pub(super) position_merges: Vec<bool>,
    pub(super) linked: bool,
    pub(super) wrap: bool,
    pub(super) crop: bool,
    pub(super) depth_culling: bool,
    pub(super) backface_culling: bool,
    pub(super) channel_on: bool,
    pub(super) channels: [bool; 5],
    pub(super) channel_value: f32,
    pub(super) seed: u32,
    pub(super) randomized: u32,
    pub(super) long_choice: usize,
}

impl Default for Gallery {
    fn default() -> Self {
        Gallery { filtering: Filtering::BilinearHq, position: [0.0, 1.25, -0.5], position_merges: Vec::new(), linked: true }
    }
}
