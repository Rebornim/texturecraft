use serde::{Deserialize, Serialize};

/// An output map's settings beyond its file type: how the template editor shows it, and dithering
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MapOptions {
    /// R, G and B are three channels of their own (the template editor's R+G+B) rather than one colour
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub separate_rgb: bool,
    /// Dither the 8-bit file against banding
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub dithering: bool,
    /// This map's own output size (power of two), instead of the export's
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u32>,
}

impl MapOptions {
    pub fn is_default(&self) -> bool {
        *self == MapOptions::default()
    }
}
