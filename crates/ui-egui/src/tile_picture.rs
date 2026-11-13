//! What a resource picker's tile shows

use egui::{Color32, TextureHandle};

/// What a tile shows: a thumbnail (none while it renders), a colour, a glyph, or the import square
pub(crate) enum TilePicture {
    Texture(Option<TextureHandle>),
    Swatch(Color32),
    Glyph(&'static str),
    Import,
}
