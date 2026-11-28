use texturecraft_texel::ColorSpace;

/// One output map's linear samples at its export size, ready to encode
pub(super) struct RenderedMap {
    /// Interleaved L, LA, RGB or RGBA samples
    pub data: Vec<f32>,
    pub size: (u32, u32),
    pub component_count: usize,
    /// Per component: whether encoding applies the sRGB transfer
    pub srgb: Vec<bool>,
    /// The colour space recorded in the file, when the user chose one for a colour map
    pub embed: Option<ColorSpace>,
}
