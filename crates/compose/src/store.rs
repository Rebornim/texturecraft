//! Pixel data owned by a document: painted layer channels, painted mask effects and baked mesh
//! maps. Everything is copy-on-write, so cloning the store for undo is cheap.

use std::collections::HashMap;

use texturecraft_core::{Channel, ChannelKind, Filtering, LayerId, Sampling};
use texturecraft_texel::{TileImage, from_u16, from_u16_signed};

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

/// One halved copy of an image (box-filtered, wrapping at the edges)
#[derive(Clone, Debug, PartialEq)]
pub struct MipLevel {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<[f32; 4]>,
}

/// Image resources by name.
pub type ImageStore = HashMap<String, std::sync::Arc<ImageRes>>;

/// Image levels below the full image at most: a 64k image halves 16 times to 1 texel
const MAX_MIP_LEVELS: usize = 16;

/// Texel (`x`, `y`) of `rgba`; an axis that does not `repeat` (across, down) stops at its edge
fn texel_of(rgba: &[[f32; 4]], width: u32, height: u32, x: i64, y: i64, repeat: (bool, bool)) -> [f32; 4] {
    let (w, h) = (width.max(1) as i64, height.max(1) as i64);
    let x = if repeat.0 { x.rem_euclid(w) } else { x.clamp(0, w - 1) };
    let y = if repeat.1 { y.rem_euclid(h) } else { y.clamp(0, h - 1) };
    let i = (y * w + x) as usize;
    rgba.get(i).copied().unwrap_or([0.0; 4])
}

/// Bilinear sample of `rgba` (texture coordinates, v down), wrapping around on the axes that `repeat`
fn bilinear(rgba: &[[f32; 4]], width: u32, height: u32, u: f32, v: f32, repeat: (bool, bool)) -> [f32; 4] {
    let fx = u * width as f32 - 0.5;
    let fy = v * height as f32 - 0.5;
    let (x0, y0) = (fx.floor() as i64, fy.floor() as i64);
    let (tx, ty) = (fx - fx.floor(), fy - fy.floor());
    let texel = |x, y| texel_of(rgba, width, height, x, y, repeat);
    let (a, b, c, d) = (texel(x0, y0), texel(x0 + 1, y0), texel(x0, y0 + 1), texel(x0 + 1, y0 + 1));
    let mut out = [0.0; 4];
    for k in 0..4 {
        let top = a[k] + (b[k] - a[k]) * tx;
        let bot = c[k] + (d[k] - c[k]) * tx;
        out[k] = top + (bot - top) * ty;
    }
    out
}

/// `rgba` halved: each texel averages the 2×2 texels it covers (odd sizes wrap)
fn halve(rgba: &[[f32; 4]], width: u32, height: u32) -> MipLevel {
    let (w, h) = ((width / 2).max(1), (height / 2).max(1));
    let mut out = Vec::with_capacity(w as usize * h as usize);
    for y in 0..i64::from(h) {
        for x in 0..i64::from(w) {
            let mut sum = [0.0f32; 4];
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let t = texel_of(rgba, width, height, x * 2 + dx, y * 2 + dy, (true, true));
                for k in 0..4 {
                    sum[k] += t[k] * 0.25;
                }
            }
            out.push(sum);
        }
    }
    MipLevel { width: w, height: h, rgba: out }
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

    /// The halved levels, built the first time they are asked for
    fn mips(&self) -> &[MipLevel] {
        self.mips.get_or_init(|| {
            let mut levels: Vec<MipLevel> = Vec::new();
            let (mut w, mut h) = (self.width, self.height);
            while (w > 1 || h > 1) && levels.len() < MAX_MIP_LEVELS {
                let next = match levels.last() {
                    Some(l) => halve(&l.rgba, l.width, l.height),
                    None => halve(&self.rgba, w, h),
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
                let level = |i: usize| match i.checked_sub(1) {
                    None => full(u, v),
                    Some(m) => self.mips().get(m).or(self.mips().last()).map_or_else(|| full(u, v), |l| bilinear(&l.rgba, l.width, l.height, u, v, repeat)),
                };
                let lo = lod.floor() as usize;
                let t = lod - lod.floor();
                let a = level(lo);
                if t <= 0.0 {
                    return a;
                }
                let b = level(lo + 1);
                [0, 1, 2, 3].map(|k| a[k] + (b[k] - a[k]) * t)
            }
        }
    }
}
