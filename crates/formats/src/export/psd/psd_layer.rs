/// One layer of a PSD: an output map's texels, in the groups its file name's folders make
pub struct PsdLayer {
    /// The groups it sits in, outermost first, then the layer's own name
    pub path: Vec<String>,
    /// 1 gray, 2 gray + alpha, 3 RGB, 4 RGBA
    pub component_count: usize,
    /// Interleaved samples, 0-255 for an 8-bit document or 0-65535 for a 16-bit one
    pub samples: Vec<u16>,
}

impl PsdLayer {
    pub fn has_alpha(&self) -> bool {
        self.component_count.is_multiple_of(2)
    }

    /// Component `color` (0 red, 1 green, 2 blue) of texel `texel`; a gray layer gives its gray for each
    pub fn color(&self, texel: usize, color: usize) -> u16 {
        let index = if self.component_count >= 3 { color } else { 0 };
        self.samples.get(texel * self.component_count + index).copied().unwrap_or(0)
    }

    /// The alpha of texel `texel`, `max` (opaque) for a layer without alpha
    pub fn alpha(&self, texel: usize, max: u16) -> u16 {
        if self.has_alpha() { self.samples.get(texel * self.component_count + self.component_count - 1).copied().unwrap_or(max) } else { max }
    }
}
