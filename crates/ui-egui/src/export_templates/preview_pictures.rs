use egui::TextureHandle;
use texturecraft_engine::formats::Dest;

/// One output's pictures: the file as it looks, then each of its channels in grey
pub(super) struct PreviewPictures {
    pub(super) file: TextureHandle,
    pub(super) channels: Vec<(Dest, TextureHandle)>,
    /// Height over width
    pub(super) aspect: f32,
}
