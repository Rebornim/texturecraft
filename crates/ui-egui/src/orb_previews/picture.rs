use egui::TextureHandle;

/// A cached picture
pub(super) struct Picture {
    pub(super) texture: Option<TextureHandle>,
    pub(super) full: bool,
    /// The pass it was last drawn in
    pub(super) drawn: u64,
}
