use serde::{Deserialize, Serialize};

use super::{ExportFileType, ExportParameters};

/// A texture set's own export parameters; None keeps the global value
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ExportOverrides {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_type: Option<ExportFileType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u32>,
    /// Padding mode and dilation distance are one override
    #[serde(skip_serializing_if = "Option::is_none")]
    pub padding: Option<(String, u32)>,
}

impl ExportOverrides {
    pub fn is_empty(&self) -> bool {
        *self == ExportOverrides::default()
    }

    /// `global` with these overrides applied
    pub fn apply(&self, global: &ExportParameters) -> ExportParameters {
        let mut parameters = global.clone();
        if let Some(preset) = &self.preset {
            parameters.preset.clone_from(preset);
        }
        if let Some(file_type) = &self.file_type {
            parameters.file_type.clone_from(file_type);
        }
        if let Some(size) = self.size {
            parameters.size = size;
        }
        if let Some((padding, dilation)) = &self.padding {
            parameters.padding.clone_from(padding);
            parameters.dilation = *dilation;
        }

        parameters
    }
}
