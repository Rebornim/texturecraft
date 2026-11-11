use texturecraft_engine::formats::Dest;

/// What a hovered channel box says about its output besides the pictures
pub(super) struct PreviewText {
    /// The file written, or None when the texture set writes none
    pub(super) file: Option<String>,
    /// "2048 x 2048 · png 8 bits"
    pub(super) details: String,
    /// Each channel's source, in the file's channel order
    pub(super) sources: Vec<(Dest, String)>,
}
