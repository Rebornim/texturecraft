use serde::{Deserialize, Serialize};

/// The output template a project exports with until another is picked
pub const DEFAULT_EXPORT_PRESET: &str = "Document channels + Normal + AO";

/// The parameters a texture set can override, in the export command's terms
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ExportParameters {
    /// Output template name
    pub preset: String,
    /// File extension; empty takes each map's format from the template
    pub format: String,
    /// 0 takes the template's (or the format's) bit depth
    pub bit_depth: u8,
    /// Output size in pixels; 0 keeps each texture set's size
    pub size: u32,
    /// none, dilation, dilationDefault or infinite
    pub padding: String,
    /// Dilation distance in pixels
    pub dilation: u32,
}

impl Default for ExportParameters {
    fn default() -> Self {
        ExportParameters { preset: DEFAULT_EXPORT_PRESET.into(), format: String::new(), bit_depth: 0, size: 0, padding: "infinite".into(), dilation: 16 }
    }
}
