//! One tile of a fill source picker's Anchor points tab

use egui::TextureId;
use texturecraft_engine::core::LayerId;

/// One tile of the Anchor points tab: the anchor, its layer's name, the channel it is read in and a picture of that channel there
pub(crate) struct ResourcePickerAnchor {
    pub anchor: LayerId,
    /// The anchor's full name ("layer / anchor"), for its tooltip and screen readers
    pub name: String,
    pub layer: String,
    pub channel: &'static str,
    pub picture: Option<TextureId>,
}
