//! Pixel data owned by a document: painted layer channels, painted mask effects and baked mesh
//! maps. Everything is copy-on-write, so cloning the store for undo is cheap.

use std::collections::HashMap;

use texturecraft_core::{Channel, ChannelKind, Filtering, LayerId, Sampling};
use texturecraft_texel::{TileImage, from_u16, from_u16_signed};

use crate::mip_level::MipLevel;

/// Painted data of one paint layer channel: premultiplied value(s) plus coverage, u16 per
/// component (height stored signed).
#[derive(Clone, Debug, Default)]
pub struct PixelStore {
    /// (paint layer or content paint effect, channel) → texels.
    pub layers: HashMap<(LayerId, Channel), TileImage>,
    /// Paint effect of a mask → (premultiplied value, coverage).
    pub masks: HashMap<LayerId, TileImage>,
}

impl PixelStore {
    pub fn layer(&self, id: LayerId, c: Channel) -> Option<&TileImage> {
        self.layers.get(&(id, c))
    }

    /// The image for `(id, c)` at `size` (width, height), created empty if missing or of
    /// another size.
    pub fn layer_mut(&mut self, id: LayerId, c: Channel, size: (u32, u32)) -> &mut TileImage {
        sized(self.layers.entry((id, c)).or_insert_with(|| TileImage::new(size.0, size.1, c.components() + 1)), size)
    }

    pub fn mask_mut(&mut self, id: LayerId, size: (u32, u32)) -> &mut TileImage {
        sized(self.masks.entry(id).or_insert_with(|| TileImage::new(size.0, size.1, 2)), size)
    }

    /// Drop pixels of layers that no longer exist.
    pub fn retain(&mut self, alive: impl Fn(LayerId) -> bool) {
        self.layers.retain(|(id, _), _| alive(*id));
        self.masks.retain(|id, _| alive(*id));
    }

    pub fn bytes(&self) -> usize {
        self.layers.values().chain(self.masks.values()).map(TileImage::bytes).sum()
    }
}

/// `img`, replaced by an empty image of its components when it is not `size`.
fn sized(img: &mut TileImage, size: (u32, u32)) -> &mut TileImage {
    if (img.width(), img.height()) != size {
        *img = TileImage::new(size.0, size.1, img.comps());
    }
    img
}

/// Decode a stored component of channel kind `kind` to its float value.
#[inline]
pub fn decode(kind: ChannelKind, v: u16) -> f32 {
    if kind == ChannelKind::Signed { from_u16_signed(v) } else { from_u16(v) }
}

pub use crate::baked_maps::BakedMaps;

/// A decoded image resource sampled by fills (values in the file's encoding; see `srgb`).
#[derive(Clone, Debug, PartialEq)]
pub struct ImageRes {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<[f32; 4]>,
    /// 8/16-bit images are sRGB-encoded; color channels linearize them.
    pub srgb: bool,
    /// Halved copies of the image, built on first use by [`Filtering::BilinearHq`]
    pub mips: std::sync::OnceLock<Vec<MipLevel>>,
}

/// Image resources by name.
pub type ImageStore = HashMap<String, std::sync::Arc<ImageRes>>;

/// Image levels below the full image at most: a 64k image halves 16 times to 1 texel
const MAX_MIP_LEVELS: usize = 16;

/// Texel (`x`, `y`) of `rgba`; an axis that does not `repeat` (across, down) stops at its edge
pub(crate) fn texel_of(rgba: &[[f32; 4]], width: u32, height: u32, x: i64, y: i64, repeat: (bool, bool)) -> [f32; 4] {
    let (w, h) = (width.max(1) as i64, height.max(1) as i64);
    let x = if repeat.0 { x.rem_euclid(w) } else { x.clamp(0, w - 1) };
    let y = if repeat.1 { y.rem_euclid(h) } else { y.clamp(0, h - 1) };
    let i = (y * w + x) as usize;
    rgba.get(i).copied().unwrap_or([0.0; 4])
}

/// Bilinear sample of `rgba` (texture coordinates, v down), wrapping around on the axes that `repeat`
fn bilinear(rgba: &[[f32; 4]], width: u32, height: u32, u: f32, v: f32, repeat: (bool, bool)) -> [f32; 4] {
    // Texel centres sit half a texel in from the texel's corner
    let texel_x = u * width as f32 - 0.5;
    let texel_y = v * height as f32 - 0.5;
    let (left, top) = (texel_x.floor() as i64, texel_y.floor() as i64);
    let (across, down) = (texel_x - texel_x.floor(), texel_y - texel_y.floor());

    // A huge coordinate saturates to the last texel instead of overflowing past it
    let (right, bottom) = (left.saturating_add(1), top.saturating_add(1));
    let texel = |x, y| texel_of(rgba, width, height, x, y, repeat);
    let (top_left, top_right) = (texel(left, top), texel(right, top));
    let (bottom_left, bottom_right) = (texel(left, bottom), texel(right, bottom));
    let mut blended = [0.0; 4];
    for k in 0..4 {
        let upper = top_left[k] + (top_right[k] - top_left[k]) * across;
        let lower = bottom_left[k] + (bottom_right[k] - bottom_left[k]) * across;
        blended[k] = upper + (lower - upper) * down;
    }
    blended
}

impl ImageRes {
    pub fn new(width: u32, height: u32, rgba: Vec<[f32; 4]>, srgb: bool) -> ImageRes {
        ImageRes { width, height, rgba, srgb, mips: std::sync::OnceLock::new() }
    }

    fn texel(&self, x: i64, y: i64, repeat: (bool, bool)) -> [f32; 4] {
        texel_of(&self.rgba, self.width, self.height, x, y, repeat)
    }

    /// Bilinear sample with wrap-around (texture coordinates, v down).
    pub fn sample(&self, u: f32, v: f32) -> [f32; 4] {
        if !u.is_finite() || !v.is_finite() {
            return [0.0; 4];
        }
        bilinear(&self.rgba, self.width, self.height, u, v, (true, true))
    }

    /// The texel under (`u`, `v`) as stored, wrapping (an ID map's colors must not blend)
    pub fn sample_nearest(&self, u: f32, v: f32) -> [f32; 4] {
        if !u.is_finite() || !v.is_finite() {
            return [0.0; 4];
        }

        self.texel((u * self.width as f32).floor() as i64, (v * self.height as f32).floor() as i64, (true, true))
    }

    /// The halved levels, built the first time they are asked for
    fn mips(&self) -> &[MipLevel] {
        self.mips.get_or_init(|| {
            let mut levels: Vec<MipLevel> = Vec::new();
            let (mut w, mut h) = (self.width, self.height);
            while (w > 1 || h > 1) && levels.len() < MAX_MIP_LEVELS {
                let next = match levels.last() {
                    Some(level) => MipLevel::halve(&level.rgba, level.width, level.height),
                    None => MipLevel::halve(&self.rgba, w, h),
                };
                (w, h) = (next.width, next.height);
                levels.push(next);
            }
            levels
        })
    }

    /// Sample at (`u`, `v`) with `sampling`; `footprint` = image texels per surface texel (1 or less = not shrunk)
    pub fn sample_filtered(&self, u: f32, v: f32, sampling: Sampling, footprint: f32) -> [f32; 4] {
        if !u.is_finite() || !v.is_finite() {
            return [0.0; 4];
        }

        let repeat = sampling.wrap.repeats();
        let full = |u: f32, v: f32| bilinear(&self.rgba, self.width, self.height, u, v, repeat);
        match sampling.filtering {
            Filtering::Nearest => self.texel((u * self.width as f32).floor() as i64, (v * self.height as f32).floor() as i64, repeat),
            Filtering::BilinearSharp => full(u, v),
            Filtering::BilinearHq => {
                let lod = if footprint.is_finite() && footprint > 1.0 { footprint.log2() } else { 0.0 };

                // Level 0 is the full image, level n the nth mip (the smallest past the last)
                let level = |index: usize| match index.checked_sub(1) {
                    None => full(u, v),
                    Some(mip_index) => self
                        .mips()
                        .get(mip_index)
                        .or(self.mips().last())
                        .map_or_else(|| full(u, v), |mip| bilinear(&mip.rgba, mip.width, mip.height, u, v, repeat)),
                };
                let finer_level = lod.floor() as usize;
                let toward_coarser = lod - lod.floor();
                let finer = level(finer_level);
                if toward_coarser <= 0.0 {
                    return finer;
                }

                let coarser = level(finer_level + 1);
                [0, 1, 2, 3].map(|component| finer[component] + (coarser[component] - finer[component]) * toward_coarser)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_huge_texture_coordinate_samples_without_overflowing() {
        let image = ImageRes::new(2, 2, vec![[0.5; 4]; 4], false);

        let sampled = image.sample(1e30, -1e30);

        assert_eq!(sampled, [0.5; 4]);
    }
}
