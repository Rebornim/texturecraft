/// A UsdPreviewSurface input read from a texture file: the input, the file, the channels it reads and how
#[derive(Clone, Debug, PartialEq)]
pub struct UsdTextureInput {
    /// Index into the preview surface inputs [`super::usd::SLOTS`]
    pub slot: usize,
    pub file: String,
    /// "rgb", or one channel: "r", "g", "b" or "a"
    pub output: &'static str,
    pub srgb: bool,
    /// A normal written green-down (DirectX), which the shader flips back
    pub directx_normal: bool,
}

impl UsdTextureInput {
    /// Slot `slot` read from `file` the way the preview surface textures are written (its own channels, sRGB for colours)
    pub(crate) fn of_slot(slot: usize, file: String) -> Option<UsdTextureInput> {
        let &(_, _, _, output, srgb) = super::usd::SLOTS.get(slot)?;
        Some(UsdTextureInput { slot, file, output, srgb, directx_normal: false })
    }
}
