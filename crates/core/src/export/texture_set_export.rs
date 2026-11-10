use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{ExportOverrides, MapExportOverride};

/// A texture set's export choices: left out, its overrides, and its output maps' overrides
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextureSetExport {
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub skip: bool,
    #[serde(skip_serializing_if = "ExportOverrides::is_empty")]
    pub overrides: ExportOverrides,
    /// Keyed by the output map's file name template with `$channel` filled in
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub maps: BTreeMap<String, MapExportOverride>,
}

impl TextureSetExport {
    pub fn is_empty(&self) -> bool {
        *self == TextureSetExport::default()
    }

    /// Whether anything differs from the global settings (the set list marks these)
    pub fn overrides_anything(&self) -> bool {
        !self.overrides.is_empty() || self.maps.values().any(|map| map.file_type.is_some())
    }
}
