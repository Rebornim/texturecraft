use serde_json::{Value, json};
use texturecraft_engine::core::{ExportSettings, Project, TextureSetExport};

/// The export window's edits to the project's export settings, saved by Save settings or Export and dropped by Cancel
#[derive(Clone, Debug, PartialEq)]
pub struct ExportDraft {
    pub settings: ExportSettings,
    /// One per texture set
    pub sets: Vec<TextureSetExport>,
    /// The texture set whose settings show; None shows Global settings
    pub selected: Option<usize>,
}

impl ExportDraft {
    /// The project's saved settings, or the defaults writing to `default_dir`
    pub fn from_project(project: &Project, default_dir: &str) -> ExportDraft {
        let mut settings = project.export.clone().unwrap_or_default();
        if settings.dir.trim().is_empty() {
            settings.dir = default_dir.to_string();
        }

        ExportDraft { settings, sets: project.texture_sets.iter().map(|set| set.export.clone()).collect(), selected: None }
    }

    /// Back to the default settings for every texture set, writing to `default_dir`
    pub fn reset(&mut self, default_dir: &str) {
        self.settings = ExportSettings { dir: default_dir.to_string(), ..Default::default() };
        self.sets.iter_mut().for_each(|set| *set = TextureSetExport::default());
    }

    /// The `settings` and `sets` parameters of export.settings.set, export.plan and file.exportTextures
    pub fn params(&self) -> Value {
        json!({"settings": self.settings, "sets": self.sets})
    }

    pub fn is_exported(&self, ts: usize) -> bool {
        self.sets.get(ts).is_some_and(|set| !set.skip)
    }

    /// Include or leave out texture set `ts`
    pub fn set_exported(&mut self, ts: usize, exported: bool) {
        if let Some(set) = self.sets.get_mut(ts) {
            set.skip = !exported;
        }
    }
}
