use texturecraft_engine::texture_export::TextureExport;

/// The export the Export textures window runs a texture set per frame, and its console
pub struct ExportRun {
    pub export: TextureExport,
    /// Console lines, each with whether it reports a success
    pub log: Vec<(String, bool)>,
    pub cancelled: bool,
    pub finished: bool,
    /// The frame that last ran a step: a frame can run more than one pass, and each step is one frame's work
    pub stepped_frame: Option<u64>,
}

impl ExportRun {
    pub fn new(export: TextureExport) -> ExportRun {
        ExportRun { export, log: Vec::new(), cancelled: false, finished: false, stepped_frame: None }
    }

    /// Done steps over all steps, 0 to 1
    pub fn fraction(&self) -> f32 {
        let (done, total) = self.export.progress();
        if total == 0 { 1.0 } else { done as f32 / total as f32 }
    }
}
