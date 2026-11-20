//! What a source slot card edits: one channel of a fill's material, or a mask fill's one grayscale source

use serde_json::{Value, json};
use texturecraft_engine::core::{Channel, ChannelKind};

use crate::i18n::tr;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum SourceSlot {
    Channel(Channel),
    /// A mask fill's source, read as gray 0..1
    Grayscale,
}

impl SourceSlot {
    /// The slot's name on its card
    pub(crate) fn name(self) -> &'static str {
        match self {
            SourceSlot::Channel(channel) => tr(channel.label()),
            SourceSlot::Grayscale => tr("Grayscale"),
        }
    }

    /// What the slot's values are
    pub(crate) fn kind(self) -> ChannelKind {
        match self {
            SourceSlot::Channel(channel) => channel.kind(),
            SourceSlot::Grayscale => ChannelKind::Scalar,
        }
    }

    /// The value Uniform and the slot's × give it: a new mask fill's white for a grayscale
    pub(crate) fn default_value(self) -> [f32; 3] {
        match self {
            SourceSlot::Channel(channel) => channel.default_value(),
            SourceSlot::Grayscale => [1.0; 3],
        }
    }

    /// The range a uniform value or a procedural's two ends run over
    pub(crate) fn range(self) -> std::ops::RangeInclusive<f32> {
        if self.kind() == ChannelKind::Signed { -1.0..=1.0 } else { 0.0..=1.0 }
    }

    /// A procedural's two ends when it is picked: a gray fill spans the whole range, a channel a softer part of it
    pub(crate) fn procedural_ends(self) -> ([f32; 3], [f32; 3]) {
        match self {
            SourceSlot::Grayscale => ([0.0; 3], [1.0; 3]),
            SourceSlot::Channel(Channel::BaseColor) => ([0.15; 3], [0.85; 3]),
            SourceSlot::Channel(Channel::Height) => ([-0.05; 3], [0.05; 3]),
            SourceSlot::Channel(_) => ([0.2; 3], [0.8; 3]),
        }
    }

    /// The fill command's fields naming the slot (a grayscale has no channel)
    pub(crate) fn fields(self) -> Value {
        match self {
            SourceSlot::Channel(channel) => json!({"channel": channel.id()}),
            SourceSlot::Grayscale => json!({}),
        }
    }

    /// A key for the slot's own widget state
    pub(crate) fn key(self) -> &'static str {
        match self {
            SourceSlot::Channel(channel) => channel.id(),
            SourceSlot::Grayscale => "grayscale",
        }
    }
}
