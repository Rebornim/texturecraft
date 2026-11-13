//! One choice offered by a resource picker

use crate::tile_picture::TilePicture;

/// A resource the picker offers: its id, translated label, the dim sub-caption it sits under, and its picture
pub(crate) struct ResourcePickerItem<K> {
    pub id: K,
    pub label: String,
    pub caption: Option<&'static str>,
    pub picture: TilePicture,
}

impl<K> ResourcePickerItem<K> {
    /// Whether a search for `query` (lowercase) keeps the item
    pub fn matches(&self, query: &str) -> bool {
        query.is_empty() || self.label.to_lowercase().contains(query)
    }
}
