use texturecraft_engine::formats::Dest;

/// One box of an output's channels: the destinations it fills, and its label (RGB, R, G, B, A, Gr)
#[derive(Clone, Debug, PartialEq)]
pub(super) struct ChannelSlot {
    pub(super) dests: Vec<Dest>,
    pub(super) label: &'static str,
}
