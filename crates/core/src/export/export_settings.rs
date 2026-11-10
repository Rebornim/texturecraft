use serde::{Deserialize, Serialize};

use super::ExportParameters;

/// The export window's global settings, saved in the project
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ExportSettings {
    /// Output directory; empty uses the app's default
    pub dir: String,
    pub parameters: ExportParameters,
    /// openGl or directX for every normal output; empty follows the template and project
    pub normal_format: String,
    /// Colour space name of the colour maps; empty picks by file format
    pub color_space: String,
    /// Also write a USD asset: the mesh with materials reading the exported textures
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub usd: bool,
    /// Also write the shader's parameters and the texture sets using it, as JSON
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub shader_parameters: bool,
}
