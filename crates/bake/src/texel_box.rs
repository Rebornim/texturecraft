/// A rectangle of output texels, from `min` (inclusive) to `max` (exclusive), in atlas pixels
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TexelBox {
    pub min: (u32, u32),
    pub max: (u32, u32),
}

impl TexelBox {
    /// Whether texel (`x`, `y`) lies inside the box
    pub fn contains(&self, x: usize, y: usize) -> bool {
        (self.min.0 as usize..self.max.0 as usize).contains(&x) && (self.min.1 as usize..self.max.1 as usize).contains(&y)
    }

    /// The box grown by `texels` on every side, kept inside a `width` x `height` image
    pub fn grown(&self, texels: u32, width: u32, height: u32) -> TexelBox {
        TexelBox {
            min: (self.min.0.saturating_sub(texels), self.min.1.saturating_sub(texels)),
            max: (self.max.0.saturating_add(texels).min(width), self.max.1.saturating_add(texels).min(height)),
        }
    }
}

/// Whether a region bake gives `map` a whole bake's pixels: not for maps scaled by their whole range or read from neighbours
pub fn region_keeps_pixels(map: texturecraft_core::BakedMap) -> bool {
    use texturecraft_core::BakedMap;
    !matches!(map, BakedMap::Curvature | BakedMap::Height | BakedMap::Thickness)
}
