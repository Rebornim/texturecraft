/// An output map written as a layer of a texture set's PSD (see [`super::list_outputs`])
#[derive(Clone, Debug, PartialEq)]
pub struct PlannedLayer {
    /// Index into the preset's maps
    pub map: usize,
    /// The map's file name, a `/` separating the groups the layer sits in
    pub name: String,
    /// 8-bit samples written with dithering
    pub dither: bool,
}
