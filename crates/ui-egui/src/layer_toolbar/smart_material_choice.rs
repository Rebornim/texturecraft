//! A pick of the Layers toolbar's smart material picker

use std::path::PathBuf;

/// A built-in smart material by id, or one the user saved, by file
#[derive(Clone, PartialEq)]
pub(super) enum SmartMaterialChoice {
    BuiltIn(&'static str),
    Saved(PathBuf),
}
