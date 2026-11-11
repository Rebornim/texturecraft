/// One level of the pull pyramid: per texel, its colour components and how much of it is known (0 to 1)
pub(super) struct Level {
    pub(super) width: usize,
    pub(super) height: usize,
    pub(super) color: Vec<f32>,
    pub(super) weight: Vec<f32>,
}
