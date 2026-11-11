//! How a resource picker draws its tiles

use crate::tile_look::TileLook;

/// How a picker draws its tiles
pub(crate) struct PickerLook<'a> {
    pub tile: TileLook,
    /// Outline the current item's tile
    pub outline_current: bool,
    /// Show the group headers
    pub headers: bool,
    /// The items whose result depends on a random seed, marked with a die
    pub random: &'a [&'static str],
}

impl PickerLook<'_> {
    /// The shelf's tiles under group headers, the current item outlined
    pub const SHELF: PickerLook<'static> = PickerLook { tile: TileLook::SHELF, outline_current: true, headers: true, random: &[] };
}
