//! One choice offered by a resource picker

use texturecraft_engine::compose::ImageStore;
use texturecraft_engine::fill_alpha::library_alpha_of;

use crate::i18n::tr;
use crate::tile_picture::TilePicture;

/// A resource the picker offers: its id, translated label, the dim sub-caption it sits under, and its picture
pub(crate) struct ResourcePickerItem<K> {
    pub id: K,
    pub label: String,
    pub caption: Option<&'static str>,
    pub picture: TilePicture,
}

impl<K> ResourcePickerItem<K> {
    /// A picker's Textures tiles: Import... first, then the project's images sorted by name, library alphas left out; `image` gives an image's id. A thumbnail is made only once its tile is drawn, so opening the picker does not resample every image
    pub fn textures(images: &ImageStore, import: K, image: impl Fn(&str) -> K) -> Vec<Self> {
        let mut named: Vec<_> = images.iter().filter(|(name, _)| library_alpha_of(name).is_none()).collect();
        named.sort_by(|(left, _), (right, _)| left.cmp(right));

        let mut tiles = vec![Self { id: import, label: tr("Import...").to_owned(), caption: None, picture: TilePicture::Import }];
        tiles.extend(named.into_iter().map(|(name, resource)| Self {
            id: image(name),
            label: name.clone(),
            caption: None,
            picture: TilePicture::Image(name.clone(), resource.clone()),
        }));
        tiles
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    /// An item labelled `label`
    fn item(label: &str) -> ResourcePickerItem<()> {
        ResourcePickerItem { id: (), label: label.to_owned(), caption: None, picture: TilePicture::Import }
    }

    #[test]
    fn a_search_finds_a_label_by_any_part_of_it_whatever_its_case() {
        assert!(item("Fine Cracks").matches("crack"), "mid-label, an uppercase label found by a lowercase query");
        assert!(item("Fine Cracks").matches("fine c"), "across a space");
        assert!(!item("Fine Cracks").matches("cracked"), "past the label's end");
        assert!(!item("Rust").matches("dust"));
    }

    #[test]
    fn an_empty_search_keeps_every_item() {
        assert!(item("Rust").matches(""));
        assert!(item("").matches(""));
    }

    #[test]
    fn a_search_lowercases_a_label_beyond_ascii() {
        assert!(item("ÜBERZUG Grün").matches("überzug"));
        assert!(item("ÜBERZUG Grün").matches("grün"));
        assert!(item("Ποτάμι").matches("ποτ"));
        assert!(!item("Grün").matches("grun"), "an accent is part of the letter");
    }
}
