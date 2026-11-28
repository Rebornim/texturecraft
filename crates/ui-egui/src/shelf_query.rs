//! One Assets panel's query: search, type, folder and favorites only

use serde::{Deserialize, Serialize};

/// One Assets panel's search, type and folder, and whether it shows only favorites
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShelfQuery {
    /// The search text
    pub filter: String,
    /// The picked type: an index into the shelf's types, or past them for every type
    pub tab: usize,
    /// The library folder: a library id, then folder names (empty: all libraries)
    pub path: Vec<String>,
    /// Only favorites (the Favorites saved search)
    pub favorites_only: bool,
}

impl Default for ShelfQuery {
    /// Every asset of every type
    fn default() -> Self {
        Self { filter: String::new(), tab: crate::shelf_tabs::NO_TYPE, path: Vec::new(), favorites_only: false }
    }
}
