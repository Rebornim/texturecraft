use serde::{Deserialize, Serialize};

use super::ExportFileType;

/// The output template a project exports with until another is picked
pub const DEFAULT_EXPORT_PRESET: &str = "Document channels + Normal + AO";

/// The parameters a texture set can override, in the export command's terms
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ExportParameters {
    /// Output template name
    pub preset: String,
    pub file_type: ExportFileType,
    /// Output size in pixels; 0 keeps each texture set's size
    pub size: u32,
    /// none, dilation, dilationDefault, infinite or diffusion
    pub padding: String,
    /// Dilation distance in pixels
    pub dilation: u32,
}

impl Default for ExportParameters {
    fn default() -> Self {
        ExportParameters { preset: DEFAULT_EXPORT_PRESET.into(), file_type: ExportFileType::default(), size: 0, padding: "infinite".into(), dilation: 16 }
    }
}
