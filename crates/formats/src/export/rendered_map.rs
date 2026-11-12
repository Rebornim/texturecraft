use texturecraft_texel::ColorSpace;

/// One output map's texels, ready to encode
pub(super) struct RenderedMap {
    /// Interleaved linear values (sRGB is applied when encoding to integers), `component_count` per texel
    pub data: Vec<f32>,
    pub component_count: usize,
    /// (width, height)
    pub size: (u32, u32),
    /// Per component: whether it is encoded with the sRGB transfer function
    pub srgb: Vec<bool>,
    /// The colour space written into the file, for a colour map whose space the user chose
    pub embed: Option<ColorSpace>,
    /// Dither when quantizing to 8 bits
    pub dithered: bool,
}
