use serde::{Deserialize, Serialize};

/// One output map of a texture set: left out, or written in its own format and bit depth
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MapExportOverride {
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub skip: bool,
    /// File extension and bit depth
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_type: Option<(String, u8)>,
}

impl MapExportOverride {
    pub fn is_empty(&self) -> bool {
        *self == MapExportOverride::default()
    }
}
