use serde::{Deserialize, Serialize};

/// How a Tile Generator sizes its shapes
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TileSizeMode {
    /// Each shape fills its tile minus the interstice (the gap between shapes)
    #[default]
    NormalInterstice,
    /// Each shape takes a size relative to its tile, whatever the gap
    NormalSize,
    /// Like the normal size, keeping the shape's width-to-height ratio when the tile is not square
    KeepRatio,
    /// A size relative to the whole pattern, whatever the tile count
    Absolute,
    /// A size in pixels of the pattern's output
    Pixel,
}

impl TileSizeMode {
    pub const ALL: [TileSizeMode; 5] =
        [TileSizeMode::NormalInterstice, TileSizeMode::NormalSize, TileSizeMode::KeepRatio, TileSizeMode::Absolute, TileSizeMode::Pixel];

    pub fn label(self) -> &'static str {
        match self {
            TileSizeMode::NormalInterstice => "Normal - interstice",
            TileSizeMode::NormalSize => "Normal - size",
            TileSizeMode::KeepRatio => "Keep ratio",
            TileSizeMode::Absolute => "Absolute",
            TileSizeMode::Pixel => "Pixel",
        }
    }
}
