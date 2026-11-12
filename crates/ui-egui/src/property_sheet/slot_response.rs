/// What a slot card's top row did this frame
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SlotResponse {
    /// The thumbnail or name was clicked: open the source picker
    pub(crate) pick: bool,

    /// The × was clicked: back to a uniform source
    pub(crate) clear: bool,

    /// The ⟲ was clicked: restore the slot's defaults
    pub(crate) reset: bool,

    /// The slot's toggle was flipped
    pub(crate) toggled: bool,
}
