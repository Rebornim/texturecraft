//! A library alpha as a fill source: the source picker's Alphas put the alpha's picture in the project as an image resource the channel reads

use texturecraft_paint::alphas::{self, LibraryAlpha};

use crate::resource_cmds::decoded;
use crate::{EngineError, Result, Session};

/// Resource names of library alphas start with this, so the picker and the slot can tell them from imported images
const PREFIX: &str = "alpha_";
const EXTENSION: &str = ".png";
/// Samples per pixel side when the alpha is rendered, for smooth edges
const SUPERSAMPLING: usize = 2;

/// The resource name a library alpha takes in a project
pub fn resource_name(alpha: &LibraryAlpha) -> String {
    texturecraft_formats::sanitize_resource_name(&format!("{PREFIX}{}{EXTENSION}", alpha.id))
}

/// The library alpha resource `name` holds, if it is one
pub fn library_alpha_of(name: &str) -> Option<&'static LibraryAlpha> {
    name.starts_with(PREFIX).then(|| alphas::library().iter().find(|alpha| resource_name(alpha) == name)).flatten()
}

impl Session {
    /// Puts library alpha `alpha` in the project as an image resource unless it is there already, inside the caller's edit; returns its name
    pub(crate) fn add_library_alpha(&mut self, alpha: &LibraryAlpha) -> Result<String> {
        let name = resource_name(alpha);
        if self.doc.resources.contains_key(&name) {
            return Ok(name);
        }

        let bytes = png(alpha)?;
        let image = texturecraft_formats::decode_image(&bytes).map_err(|error| EngineError::Other(format!("{name}: {error}")))?;
        self.doc.resources.insert(name.clone(), std::sync::Arc::new(bytes));
        self.images.insert(name.clone(), decoded(image));
        Ok(name)
    }
}

/// The alpha's picture as a 16-bit grayscale PNG at the library's base size
fn png(alpha: &LibraryAlpha) -> Result<Vec<u8>> {
    let size = alphas::BASE;
    let side = u32::try_from(size).map_err(|_| EngineError::Other("alpha size out of range".into()))?;
    texturecraft_formats::mcmat::mesh_map_png(side, 1, &alpha.render(size, SUPERSAMPLING))
        .map_err(|error| EngineError::Other(format!("alpha {}: {error}", alpha.id)))
}
