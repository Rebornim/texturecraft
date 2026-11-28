use egui::Color32;

/// How a channel swatch is drawn.
pub(super) struct SwatchLook {
    /// The source map's colour (None: empty or a constant).
    pub(super) fill: Option<Color32>,
    /// The underline in the colour of the channel the swatch's letters name (None: a constant)
    pub(super) bar: Option<Color32>,
    pub(super) inverted: bool,
}
