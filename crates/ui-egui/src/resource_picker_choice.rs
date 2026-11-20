//! What a fill source picker's click picked

use texturecraft_engine::core::LayerId;

/// What a fill source picker's click picked
#[derive(Clone, PartialEq)]
pub(crate) enum ResourcePickerChoice {
    Resource(String),
    Anchor(LayerId),
}
