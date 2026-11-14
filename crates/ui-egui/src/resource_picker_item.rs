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
    /// Whether a search for `query` (lowercase) keeps the item; compares as it lowercases, so the open picker builds no lowercase copy of every label each frame
    pub fn matches(&self, query: &str) -> bool {
        query.is_empty() || self.label.char_indices().any(|(start, _)| self.label.get(start..).is_some_and(|rest| starts_with_lowercase(rest, query)))
    }
}

/// Whether `text`, lowercased, starts with `query`
fn starts_with_lowercase(text: &str, query: &str) -> bool {
    let mut lowered = text.chars().flat_map(char::to_lowercase);
    query.chars().all(|wanted| lowered.next() == Some(wanted))
}
