//! A texture export run a step at a time, so a window can show its progress and stop it between texture sets

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use serde_json::Value;
use texturecraft_formats::{ExportOptions, UsdTextureInput};

use crate::commands::bad;
use crate::{EngineError, Result, Session};

/// One export: a step per texture set, then the USD asset and the shader parameters when asked for
pub struct TextureExport {
    dir: PathBuf,
    jobs: VecDeque<(Vec<usize>, ExportOptions)>,
    steps: usize,
    materialx: bool,
    usd_inputs: Option<Vec<Vec<UsdTextureInput>>>,
    shader_parameters: bool,
    exported_sets: Vec<usize>,
    /// Every file written so far
    pub written: Vec<PathBuf>,
    pub warnings: Vec<String>,
}

impl TextureExport {
    /// An export with file.exportTextures' parameters `p` into `dir` (created if needed)
    pub fn start(s: &Session, p: &Value, dir: &str, cmd: &str) -> Result<TextureExport> {
        let jobs = crate::export_settings_cmds::export_jobs(s, p, cmd)?;
        let shader_parameters = crate::export_settings_cmds::writes_shader_parameters(s, p, cmd)?;
        let mut warnings = Vec::new();
        let usd_inputs = if crate::export_settings_cmds::writes_usd(s, p, cmd)? { Some(s.usd_asset_inputs(&jobs, &mut warnings)?) } else { None };
        std::fs::create_dir_all(dir).map_err(|e| bad(cmd, format!("cannot create {dir}: {e}")))?;

        Ok(TextureExport {
            dir: PathBuf::from(dir),
            steps: jobs.len(),
            exported_sets: jobs.iter().flat_map(|(sets, _)| sets.iter().copied()).collect(),
            jobs: jobs.into(),
            materialx: p.get("materialX").and_then(Value::as_bool).unwrap_or(false),
            usd_inputs,
            shader_parameters,
            written: Vec::new(),
            warnings,
        })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// How many steps the export takes, and how many are done
    pub fn progress(&self) -> (usize, usize) {
        (self.steps - self.jobs.len(), self.steps)
    }

    /// The texture sets the next step exports
    pub fn next_sets(&self) -> Option<&[usize]> {
        self.jobs.front().map(|(sets, _)| sets.as_slice())
    }

    /// Export the next texture set and return the files it wrote; None once every set is done
    pub fn step(&mut self, s: &mut Session) -> Result<Option<Vec<PathBuf>>> {
        let Some((sets, options)) = self.jobs.pop_front() else { return Ok(None) };

        let files = s.export_files(&sets, &options)?;
        self.warnings.extend(files.iter().flat_map(|f| f.warnings.iter().cloned()));
        let mut written = texturecraft_formats::write_files(&files, &self.dir).map_err(|e| EngineError::Other(e.to_string()))?;
        if self.materialx {
            written.extend(crate::file_cmds::write_materialx(s, &sets, &options, &self.dir, &mut self.warnings)?);
        }
        self.written.extend(written.iter().cloned());
        Ok(Some(written))
    }

    /// Write the USD asset and the shader parameters, after the last step; returns what they wrote
    pub fn finish(&mut self, s: &Session, cmd: &str) -> Result<Vec<PathBuf>> {
        let stem = texturecraft_formats::export::sanitize_file_stem(&s.doc.project.name);
        let mut written = Vec::new();
        if let Some(inputs) = self.usd_inputs.take() {
            let path = self.dir.join(format!("{stem}.usda"));
            let layer = texturecraft_formats::usd_asset(&s.mesh, &s.doc.project.name, &inputs).map_err(|e| EngineError::Other(e.to_string()))?;
            std::fs::write(&path, layer).map_err(|e| bad(cmd, format!("cannot write {}: {e}", path.display())))?;
            written.push(path);
        }
        if std::mem::take(&mut self.shader_parameters) {
            let path = self.dir.join(crate::shader_parameters::shader_parameters_file(&s.doc.project.name));
            let text =
                serde_json::to_string_pretty(&crate::shader_parameters::shader_parameters(s, &self.exported_sets)).map_err(|e| bad(cmd, e.to_string()))?;
            std::fs::write(&path, text).map_err(|e| bad(cmd, format!("cannot write {}: {e}", path.display())))?;
            written.push(path);
        }

        self.written.extend(written.iter().cloned());
        Ok(written)
    }
}
