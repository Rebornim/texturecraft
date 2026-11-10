use serde::{Deserialize, Serialize};

/// A file type to write: format, bit depth and whether 8-bit files are dithered
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ExportFileType {
    /// File extension; empty takes each map's format from the template
    pub format: String,
    /// 0 takes the template's (or the format's) bit depth
    pub bit_depth: u8,
    /// Dither 8-bit files against banding in smooth gradients
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub dithering: bool,
}

impl ExportFileType {
    pub fn new(format: impl Into<String>, bit_depth: u8) -> ExportFileType {
        ExportFileType { format: format.into(), bit_depth, dithering: false }
    }
}
