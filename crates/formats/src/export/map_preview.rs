use super::Dest;

/// One output map, small: its channels in file order and 8-bit values as the file stores them (colour sRGB-encoded)
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapPreview {
    pub width: u32,
    pub height: u32,
    pub channels: Vec<Dest>,
    /// `channels.len()` values per pixel, row by row
    pub pixels: Vec<u8>,
}
