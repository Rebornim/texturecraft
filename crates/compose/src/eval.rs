//! The layer-stack evaluator: composites a texture set's stack into one image per channel,
//! tile by tile in parallel. This is the reference implementation (the oracle) for the viewport
//! and for export.

pub(crate) mod thumbnail;

use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use rayon::prelude::*;
use texturecraft_core::{
    AnisotropicKuwahara, BakedMap, Bevel, BevelSmooth, BlendMode, BlockScales, Channel, ChannelKind, ColorMatch, CompareSource, ContentsOffset,
    CurvatureDetail, CurvatureMode, Effect, EffectKind, Fill, Filter, Filtering, Generator, GeneratorKind, HeightMask, HeightToNormal, HighpassSpace,
    ImageInput, Layer, LayerId, LayerKind, Mask, MaskBackground, MaskOutline, Mirror, NormalFormat, Project, Projection, RepeatMode, SIZE_REFERENCE_RES,
    Sampling, ShapeCrop, Source, SourceColorMode, SourceSlopeBlur, SourceWarp, StylizedLighting, TargetColorMode, TextureSet, TileGrid, TileTransform, blend1,
    blend3, lum, mix3, per_unit, posterize,
};
use texturecraft_geom::glam::{Vec2, Vec3};
use texturecraft_geom::{Mesh, NO_TRI, Surface, UvGBuffer};
use texturecraft_texel::{DilationMap, TILE, TILE_AREA, TileImage, from_u16, noise, to_u16, to_u16_signed};

use crate::below::{Band, BelowCache, Level};
use crate::curvature_sizes::CurvatureSizes;
use crate::effect_histogram::EffectHistogram;
use crate::environment_sh::EnvironmentSh;
use crate::fill_copies::FillCopies;
use crate::fill_copy::FillCopy;
use crate::mask_gray::MaskGray;
use crate::pattern::{Coords, image_samples, mesh_frame, procedural_sized, world_noise};
use crate::projector_cover::{framed_cover, projector_cover};
use crate::projector_depth::ProjectorDepth;
use crate::shape_distance::distance_filter;
use crate::slope_texel::slope_texel;
use crate::store::{BakedMaps, ImageRes, ImageStore, PixelStore, decode};
use crate::warp_texel::warp_texel;

const NCH: usize = Channel::ALL.len();

/// Coverage past which a symmetric fill's remaining copies would not show under the ones above
const COVERED: f32 = 0.9999;

/// Share of an image's longer side a step to the next texel may jump before it counts as a UV seam
const SEAM_JUMP_SHARE: f32 = 0.25;

/// Stack values being composited, per channel.
type Acc = [[f32; 3]; NCH];

/// The baked curvature of a flat surface (below: cavities, above: edges)
const FLAT_CURVATURE: f32 = 0.5;

/// How tall a baked height of 1 stands, as a fraction of a UV tile, when Baked Lighting Stylized tilts normals by its slope
const HEIGHT_RELIEF: f32 = 1.0 / 32.0;

/// The Height Intensity of Baked Lighting Environment that tilts the normals as Baked Lighting Stylized's Height to Normal Intensity 1 does (its default)
const ENVIRONMENT_HEIGHT_UNIT: f32 = 4.0;

/// The most Baked Lighting Environment's Exposure brightens or darkens, in stops
const EXPOSURE_STOPS: f32 = 5.0;

/// What a live stroke paints into.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaintTarget {
    /// A paint layer's channels.
    Layer(LayerId),
    /// A paint effect inside a mask.
    Mask(LayerId),
}

/// A screen-pinned image painted through the camera (projection tool / stencil).
#[derive(Clone, Debug)]
pub struct Stencil {
    pub image: std::sync::Arc<crate::store::ImageRes>,
    /// Camera view-projection captured when the stroke started.
    pub view_proj: texturecraft_geom::glam::Mat4,
    /// Image placement in normalized device coordinates: center, half extents, rotation.
    pub center: [f32; 2],
    pub half: [f32; 2],
    pub rotation: f32,
    /// Repeat the image beyond its bounds (otherwise nothing is painted outside it).
    pub tile: bool,
}

impl Stencil {
    /// Linear color and alpha of the stencil at world point `p` (None outside the image).
    pub fn sample_world(&self, p: Vec3) -> Option<([f32; 3], f32)> {
        let c = self.view_proj * p.extend(1.0);
        if c.w <= 1e-6 {
            return None;
        }
        let (dx, dy) = (c.x / c.w - self.center[0], c.y / c.w - self.center[1]);
        let (sn, cs) = (-self.rotation).sin_cos();
        let lx = (dx * cs - dy * sn) / self.half[0].max(1e-6);
        let ly = (dx * sn + dy * cs) / self.half[1].max(1e-6);
        if !self.tile && (lx.abs() > 1.0 || ly.abs() > 1.0) {
            return None;
        }
        let s = self.image.sample(lx * 0.5 + 0.5, 0.5 - ly * 0.5);
        let rgb = if self.image.srgb { [s[0], s[1], s[2]].map(texturecraft_texel::srgb_to_linear) } else { [s[0], s[1], s[2]] };
        Some((rgb, s[3].clamp(0.0, 1.0)))
    }
}

/// The stroke being drawn, shown before it is committed.
#[derive(Clone, Debug)]
pub struct StrokePreview {
    pub target: PaintTarget,
    /// Stroke alpha per texel (1 component, 0..1).
    pub alpha: TileImage,
    /// Stroke opacity cap.
    pub opacity: f32,
    pub erase: bool,
    /// Values painted per channel (layer targets).
    pub values: BTreeMap<Channel, [f32; 3]>,
    /// Gray painted into a mask
    pub mask_gray: MaskGray,
    /// Projection tool: base color (and alpha) come from a screen-pinned image.
    pub stencil: Option<Stencil>,
    /// Clone and smudge: straight values per channel carried by the stroke (replace `values`).
    pub carried: BTreeMap<Channel, TileImage>,
}

/// Everything the evaluator reads.
pub struct Inputs<'a> {
    pub project: &'a Project,
    pub ts: usize,
    pub mesh: &'a Mesh,
    pub gbuf: &'a UvGBuffer,
    pub pixels: &'a PixelStore,
    pub baked: &'a BakedMaps,
    pub block_scales: BlockScales,
    pub preview: Option<&'a StrokePreview>,
    /// Imported image resources (fill sources).
    pub images: &'a ImageStore,
    /// Mesh bounds, for scale-independent procedural coordinates.
    pub bounds: (Vec3, Vec3),
}

impl Inputs<'_> {
    /// The UV tile grid of a UV-tiled set (the mesh's UVs for this set are in atlas space).
    fn tile_grid(&self) -> Option<TileGrid> {
        self.project.texture_set(self.ts).ok().and_then(TextureSet::tile_grid)
    }

    /// Texels per atlas block across (the whole width for a classic set): filter radii scale
    /// with it, and with the tile's share of its block.
    fn tile_res(&self) -> u32 {
        self.gbuf.width / self.tile_grid().map_or(1, |g| g.cols.max(1))
    }
}

/// The composited channels of one texture set.
#[derive(Clone, Debug, PartialEq)]
pub struct Composite {
    /// Width in texels (the resolution of a classic, square set).
    pub resolution: u32,
    pub height: u32,
    pub channels: BTreeMap<Channel, TileImage>,
    /// The set's [`TextureSet::block_scales`]: height slopes on the display follow each tile.
    pub block_scales: BlockScales,
}

impl Composite {
    /// A square composite.
    pub fn new(channels: &[Channel], resolution: u32) -> Composite {
        Composite::with_size(channels, resolution, resolution)
    }

    pub fn with_size(channels: &[Channel], width: u32, height: u32) -> Composite {
        Composite {
            resolution: width,
            height,
            channels: channels.iter().map(|c| (*c, TileImage::new(width, height, c.components()))).collect(),
            block_scales: BlockScales::default(),
        }
    }

    /// Channel value at a texel (the channel default when absent).
    pub fn value(&self, c: Channel, x: u32, y: u32) -> [f32; 3] {
        let Some(img) = self.channels.get(&c) else { return c.default_value() };
        let raw = img.get(x, y);
        let k = c.kind();
        if !img.is_allocated(x / TILE, y / TILE) {
            return c.default_value();
        }
        if c.components() == 3 { [decode(k, raw[0]), decode(k, raw[1]), decode(k, raw[2])] } else { [decode(k, raw[0]); 3] }
    }
}

/// Every tile of a `width × height` image.
pub fn all_tiles(width: u32, height: u32) -> Vec<(u32, u32)> {
    let (nx, ny) = (width.div_ceil(TILE), height.div_ceil(TILE));
    (0..ny).flat_map(|y| (0..nx).map(move |x| (x, y))).collect()
}

/// Tiles that hold at least one covered texel.
pub fn covered_tiles(gbuf: &UvGBuffer) -> Vec<(u32, u32)> {
    let (tw, th) = (gbuf.width.div_ceil(TILE), gbuf.height.div_ceil(TILE));
    let mut out = Vec::new();
    for ty in 0..th {
        for tx in 0..tw {
            let any = (ty * TILE..((ty + 1) * TILE).min(gbuf.height))
                .any(|y| (tx * TILE..((tx + 1) * TILE).min(gbuf.width)).any(|x| gbuf.tri.get((y * gbuf.width + x) as usize).is_some_and(|t| *t != NO_TRI)));
            if any {
                out.push((tx, ty));
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Per-tile plan: the stack flattened with each layer's tile slices looked up once per tile.

struct ChanPlan<'a> {
    ch: usize,
    kind: ChannelKind,
    comps: usize,
    blend: BlendMode,
    opacity: f32,
    data: Option<Cow<'a, [u16]>>,
    /// Visible content effects, holding this channel's tile for paint effects (empty, and
    /// allocation free, for layers without content effects).
    effects: Vec<EffectPlan<'a>>,
    /// A content neighbourhood filter precomputed over the region: continue the effects after
    /// effect `.0` starting from the field's value.
    start: Option<(usize, &'a Field)>,
}

enum Content<'a> {
    Paint {
        preview: bool,
    },
    /// A fill (with the Scale and Rotation of the folders around it laid on), with its projector's depth map when it culls by depth
    Fill(Box<Cow<'a, Fill>>, Option<Arc<ProjectorDepth>>),
    Group(Vec<Node<'a>>),
}

struct EffectPlan<'a> {
    /// The effect's id (an anchor point records under it)
    id: LayerId,
    /// The effect, a fill's with the Scale and Rotation of the folders around its layer laid on
    kind: Cow<'a, EffectKind>,
    blend: BlendMode,
    opacity: f32,
    data: Option<Cow<'a, [u16]>>,
    preview: bool,
    /// The curvature detail sizes a generator with detail reads.
    curvature_sizes: Option<Arc<CurvatureSizes>>,
    /// The projector's depth map of a fill or material effect that culls by depth
    depth: Option<Arc<ProjectorDepth>>,
    /// The viewport environment's light, for Baked Lighting Environment
    environment: Option<EnvironmentSh>,
    /// The source a point filter's image input reads (Grayscale Conversion's Custom Input)
    input: Option<Source>,
}

struct MaskPlan<'a> {
    background: f32,
    effects: Vec<EffectPlan<'a>>,
    /// The layer's own content in the channels its compare effects read (planned only when one reads the stack)
    layer: Option<(Content<'a>, Vec<ChanPlan<'a>>)>,
    /// A neighbourhood filter (blur, sharpen) precomputed over the region: continue the stack
    /// after effect `.0` starting from the field's value.
    start: Option<(usize, &'a Field)>,
}

/// A layer's mask (one value per texel) or one content channel (straight value and coverage)
/// after its last neighbourhood filter, over a texel region.
pub struct Field {
    x0: u32,
    y0: u32,
    w: u32,
    h: u32,
    comps: usize,
    data: Vec<f32>,
}

impl Field {
    fn get(&self, x: u32, y: u32) -> Option<&[f32]> {
        if x < self.x0 || y < self.y0 || x >= self.x0 + self.w || y >= self.y0 + self.h {
            return None;
        }
        let i = ((y - self.y0) * self.w + (x - self.x0)) as usize * self.comps;
        self.data.get(i..i + self.comps)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FieldKey {
    Mask(LayerId),
    Content(LayerId, Channel),
}

pub type Fields = HashMap<FieldKey, Field>;

/// How a neighbourhood filter reads around a texel
#[derive(Clone, Copy)]
enum Op {
    /// Box blurs (≈ Gaussian), sharpening by the amount when given
    Blur(Option<f32>),
    /// The value minus its box blur, around the channel's mid value: colors in `space`, the coverage too when `alpha`
    HighPass { space: HighpassSpace, alpha: bool },
    /// Values ramped down towards their shape's edge, from the box blur
    Bevel,
    /// The value plus its box blur times the amount
    Glow(f32),
    /// The value at the center of its cell (`r` texels across); the texel's own coverage kept unless `alpha`
    Mosaic { alpha: bool },
    /// Stair-stepped edges blended across (FXAA); the texel's own coverage kept unless `alpha`
    Fxaa { alpha: bool },
    /// The mean of the calmest sector of an ellipse along the local direction (see [`kuwahara_texel`])
    Kuwahara(AnisotropicKuwahara),
    /// Average along a direction (unit vector in texels)
    Line([f32; 2]),
    /// Read at an offset given by a noise vector field, or by noise along a direction
    Warp { scale: f32, seed: u32, dir: Option<[f32; 2]> },
    /// Gradient magnitude from central differences
    Edge,
    /// Average downhill along a noise height field
    Slope { scale: f32, seed: u32 },
    /// Box blurs, then the effect's own point filter on the blurred value (a gradient's Blur Intensity)
    PreBlur,
    /// A halo from the box blur of the shapes, laid around (or inside) them
    Halo,
    /// The shapes moved by `offset` (whole texels), box blurred and laid under them
    Shadow { offset: [i32; 2] },
    /// Each UV tile's texels read from where `mirror` sends them in their tile (`block` texels across the set's atlas blocks)
    Mirror { mirror: Mirror, block: u32 },
    /// Each UV tile's texels read from where `transform` sends them, repeating round the tile or empty past it
    Transform { transform: TileTransform, block: u32 },
    /// A band along the edge of the shapes past a threshold, from the box blur of the shapes
    Outline(MaskOutline),
    /// Drawn from the distance to the shapes' edge (see [`distance_filter`])
    Distance,
    /// Read from where a warp's source sends each texel (see [`warp_texel`])
    SourceWarp(SourceWarp),
    /// The values downhill of each texel along a source's slope, blended (see [`slope_texel`])
    SourceSlopeBlur(SourceSlopeBlur),
    /// The normal of the slopes of the layer's Height (see [`height_to_normal_texel`])
    HeightToNormal(HeightToNormal),
    /// Colors moved from a source color to a target color; the source the whole set's mean in Source Color Mode Average (see [`color_match`])
    ColorMatch(ColorMatch),
}

/// The last neighbourhood filter of an effect stack (see [`neighbourhood_filters`])
fn neighbourhood(effects: &[Effect], res: u32) -> Option<(usize, f32, Op)> {
    neighbourhood_filters(effects, res).last()
}

/// The neighbourhood filters of an effect stack, bottom → top: index among the visible effects, radius in texels (`res` texels per UV tile across) and what it does
fn neighbourhood_filters(effects: &[Effect], res: u32) -> impl Iterator<Item = (usize, f32, Op)> + '_ {
    let texels = move |size: f32| (size * res as f32 / SIZE_REFERENCE_RES).max(0.5);
    effects.iter().filter(|effect| effect.visible).enumerate().filter_map(move |(index, effect)| match &effect.kind {
        EffectKind::Filter(Filter::Blur { radius, .. }) if *radius > 0.0 => Some((index, texels(*radius), Op::Blur(None))),
        EffectKind::Filter(Filter::Sharpen { amount }) => Some((index, (1.5 * res as f32 / 2048.0).max(0.75), Op::Blur(Some(*amount)))),
        EffectKind::Filter(Filter::DirectionalBlur { radius, angle }) if *radius > 0.0 => Some((index, texels(*radius), Op::Line(direction(*angle)))),
        EffectKind::Filter(Filter::Warp { intensity, scale, seed, divider }) if *intensity > 0.0 => {
            Some((index, texels(divided(*intensity, *divider)), Op::Warp { scale: *scale, seed: *seed, dir: None }))
        }
        EffectKind::Filter(Filter::DirectionalWarp { intensity, angle, scale, seed, divider }) if *intensity > 0.0 => {
            Some((index, texels(divided(*intensity, *divider)), Op::Warp { scale: *scale, seed: *seed, dir: Some(direction(*angle)) }))
        }
        EffectKind::Filter(Filter::SourceWarp(warp)) if warp.move_texels(res) > 0.0 => Some((index, warp.reach(res).max(0.5), Op::SourceWarp(*warp))),
        EffectKind::Filter(Filter::SourceSlopeBlur(slope)) if slope.move_texels(res) > 0.0 => {
            Some((index, slope.reach(res).max(0.5), Op::SourceSlopeBlur(*slope)))
        }
        EffectKind::Filter(Filter::HighPass { radius, space, alpha }) => Some((index, texels(*radius), Op::HighPass { space: *space, alpha: *alpha })),
        EffectKind::Filter(Filter::EdgeDetect { width }) => Some((index, texels(*width), Op::Edge)),
        // Three box passes ramp an edge over about twice their radius
        EffectKind::Filter(Filter::Bevel { width, .. }) => Some((index, texels(*width * 0.5), Op::Bevel)),
        EffectKind::Filter(Filter::Glow { radius, amount }) => Some((index, texels(*radius), Op::Glow(*amount))),
        EffectKind::Filter(Filter::Mosaic { size, alpha }) => Some((index, texels(*size), Op::Mosaic { alpha: *alpha })),
        EffectKind::Filter(Filter::Fxaa { alpha }) => Some((index, FXAA_SEARCH as f32 + 1.0, Op::Fxaa { alpha: *alpha })),
        // The ellipse reaches up to twice the radius along a clear direction, and the direction is read around its reach
        EffectKind::Filter(Filter::AnisotropicKuwahara(kuwahara)) if kuwahara.radius > 0.0 => {
            Some((index, texels(kuwahara.radius.min(AnisotropicKuwahara::MAX_RADIUS)) * (2.0 + kuwahara.tensor_reach()) + 1.0, Op::Kuwahara(*kuwahara)))
        }

        // The slopes come from the heights right around a texel
        EffectKind::Filter(Filter::HeightToNormal(filter)) => Some((index, 1.0, Op::HeightToNormal(*filter))),
        // Average reads the mean color of the whole set; the other modes read the texel alone
        EffectKind::Filter(Filter::ColorMatch(matcher)) => {
            Some((index, if matcher.source_mode == SourceColorMode::Average { WHOLE_SET } else { 1.0 }, Op::ColorMatch(*matcher)))
        }

        // A mirror reads anywhere in its UV tile
        EffectKind::Filter(Filter::Mirror(mirror)) => Some((index, res as f32, Op::Mirror { mirror: *mirror, block: res })),
        // A transform reads anywhere in its UV tile; one that changes nothing is skipped
        EffectKind::Filter(Filter::Transform(transform)) if !transform.is_identity() => {
            Some((index, res as f32, Op::Transform { transform: *transform, block: res }))
        }

        // Three box passes reach the band's width
        EffectKind::Filter(Filter::MaskOutline(outline)) => {
            Some((index, (outline.width * res as f32 / BLUR_BOX_PASSES as f32).max(0.5), Op::Outline(*outline)))
        }

        // Three box passes' reach covers the distance and its smoothing
        EffectKind::Filter(filter @ (Filter::DistanceBevel(_) | Filter::BevelSmooth(_) | Filter::DirectionalDistance(_))) => {
            Some((index, distance_reach(filter, res) / BLUR_BOX_PASSES as f32 + 1.0, Op::Distance))
        }
        EffectKind::Filter(Filter::HaloGlow { width, .. }) => Some((index, texels(width.abs() * Filter::HALO_REACH), Op::Halo)),
        EffectKind::Filter(Filter::DropShadow { angle, distance, size, .. }) => {
            // Cast away from the light: `distance` of a UV tile, against the light's direction
            let [dx, dy] = direction(*angle);
            let reach = distance.clamp(-0.5, 0.5) * res as f32;
            let offset = [(-dx * reach).round() as i32, (-dy * reach).round() as i32];
            Some((index, texels(size * Filter::HALO_REACH), Op::Shadow { offset }))
        }
        EffectKind::Filter(Filter::GradientMap { input, debug: false, .. }) if input.blur > 0.0 => Some((index, texels(input.blur), Op::PreBlur)),
        EffectKind::Filter(Filter::SlopeBlur { radius, scale, seed, divider }) if *radius > 0.0 => {
            Some((index, texels(divided(*radius, *divider)), Op::Slope { scale: *scale, seed: *seed }))
        }
        _ => None,
    })
}

/// A neighbourhood radius that reaches the whole set ([`filter_extent`] caps it at the set's longest side)
const WHOLE_SET: f32 = 65_536.0;

/// How many texels around a texel distance filter `filter` reads at `res` texels across a UV tile: its reach and its smoothing's three box passes
fn distance_reach(filter: &Filter, res: u32) -> f32 {
    match filter {
        Filter::DistanceBevel(bevel) => bevel.reach(res) + BLUR_BOX_PASSES as f32 * bevel.smoothing_texels(res),
        Filter::BevelSmooth(smooth) => smooth.reach(res) + BLUR_BOX_PASSES as f32 * smooth.smoothing_texels(res),
        Filter::DirectionalDistance(trail) => trail.reach(res),
        _ => 0.0,
    }
}

/// `size` over its intensity divider (a divider of 0 from a hostile file counts as 1)
fn divided(size: f32, divider: u16) -> f32 {
    size / f32::from(divider.max(1))
}

/// Unit texel-space vector of `angle` degrees. Texel rows run down: positive angles turn
/// counterclockwise on screen.
pub(crate) fn direction(angle: f32) -> [f32; 2] {
    let (sn, cs) = angle.to_radians().sin_cos();
    [cs, -sn]
}

/// Gain on the curvature map's own deviation from flat (±0.5 reaches black and white).
const MAP_GAIN: f32 = 2.0;

/// Gain per texel of a detail size: blurring spreads an edge over about twice its radius, so wider sizes need proportionally more to reach full strength.
const GAIN_PER_DETAIL_TEXEL: f32 = 2.0 * std::f32::consts::SQRT_2;

/// Samples along a directional or slope blur at most (beyond, samples spread out).
pub(crate) const LINE_SAMPLES: i32 = 24;

/// Fractal noise over texel position `(x, y)` (`res` texels per UV unit, `scale` features per
/// UV unit), shifted by `d` in noise space.
pub(crate) fn texel_noise(x: f32, y: f32, d: [f32; 2], res: u32, scale: f32, seed: u32) -> f32 {
    let k = if scale.is_finite() { scale.clamp(0.01, 1000.0) } else { 1.0 } / res.max(1) as f32;
    noise::fbm([x * k + d[0], y * k + d[1], 0.0], 3, seed, noise::perlin)
}

/// The neighbourhood filters that gather samples (directional and slope blur, warp) over a field region `(x0, y0, w, h)` of premultiplied `orig` texels (`comps` each) and their `cover`: bilinear samples weighted by coverage, so uncovered texels don't bleed in; positions are absolute texel coordinates, so a texel's result does not depend on the region
#[allow(clippy::too_many_arguments)]
fn gather(
    op: Op,
    (orig, cover, grays): (&[f32], &[f32], &[f32]),
    comps: usize,
    (x0, y0, w, h): (u32, u32, u32, u32),
    r: f32,
    res: u32,
    opacity: f32,
) -> Vec<f32> {
    let mut data = orig.to_vec();
    let (wu, hu) = (w as usize, h as usize);

    // The filter's image input at absolute texel position (fx, fy), bilinear over the covered texels; None off them
    let input = |fx: f32, fy: f32| -> Option<f32> {
        if !(fx.is_finite() && fy.is_finite()) {
            return None;
        }

        let (bx, by) = (fx.floor(), fy.floor());
        let (tx, ty) = (fx - bx, fy - by);
        let (mut sum, mut weight) = (0.0f32, 0.0f32);
        for (dx, dy, k) in [(0, 0, (1.0 - tx) * (1.0 - ty)), (1, 0, tx * (1.0 - ty)), (0, 1, (1.0 - tx) * ty), (1, 1, tx * ty)] {
            let (cx, cy) = (bx as i64 + dx - i64::from(x0), by as i64 + dy - i64::from(y0));
            if k <= 0.0 || cx < 0 || cy < 0 || cx >= wu as i64 || cy >= hu as i64 {
                continue;
            }

            let i = cy as usize * wu + cx as usize;
            if let (Some(covered), Some(gray)) = (cover.get(i), grays.get(i))
                && *covered > 0.0
            {
                sum += gray * k;
                weight += k;
            }
        }
        (weight > 1e-6).then(|| sum / weight)
    };
    // Bilinear sample at absolute texel position (fx, fy) into `acc` (components, then weight).
    let sample = |fx: f32, fy: f32, acc: &mut [f32; 5]| {
        if !(fx.is_finite() && fy.is_finite()) {
            return;
        }
        let (bx, by) = (fx.floor(), fy.floor());
        let (tx, ty) = (fx - bx, fy - by);
        for (dx, dy, k) in [(0, 0, (1.0 - tx) * (1.0 - ty)), (1, 0, tx * (1.0 - ty)), (0, 1, (1.0 - tx) * ty), (1, 1, tx * ty)] {
            let (cx, cy) = (bx as i64 + dx - i64::from(x0), by as i64 + dy - i64::from(y0));
            if k <= 0.0 || cx < 0 || cy < 0 || cx >= wu as i64 || cy >= hu as i64 {
                continue;
            }
            let i = cy as usize * wu + cx as usize;
            let (Some(c), Some(px)) = (cover.get(i), orig.get(i * comps..(i + 1) * comps)) else { continue };
            for (a, v) in acc.iter_mut().zip(px) {
                *a += v * k;
            }
            acc[4] += c * k;
        }
    };
    data.par_chunks_mut((comps * wu).max(1)).enumerate().for_each(|(row, line)| {
        for col in 0..wu {
            if cover.get(row * wu + col).is_none_or(|c| *c <= 0.0) {
                continue;
            }
            let (x, y) = ((x0 as usize + col) as f32, (y0 as usize + row) as f32);
            let mut acc = [0.0f32; 5];
            let n = (r.ceil() as i32).clamp(1, LINE_SAMPLES);
            let step = r / n as f32;
            match op {
                Op::Line(d) => {
                    for s in -n..=n {
                        let t = s as f32 * step;
                        sample(x + d[0] * t, y + d[1] * t, &mut acc);
                    }
                }
                Op::Warp { scale, seed, dir } => {
                    let off = |salt: u32| ((texel_noise(x, y, [0.0; 2], res, scale, seed ^ salt) - 0.5) * 3.0).clamp(-1.0, 1.0) * r;
                    match dir {
                        Some(d) => {
                            let t = off(0);
                            sample(x + d[0] * t, y + d[1] * t, &mut acc);
                        }
                        None => sample(x + off(0), y + off(0x5BD1_E995), &mut acc),
                    }
                }
                Op::Edge => {
                    let center = orig.get((row * wu + col) * comps..(row * wu + col + 1) * comps).unwrap_or(&[]);
                    let at = |dx: f32, dy: f32| {
                        let mut a = [0.0f32; 5];
                        sample(x + dx, y + dy, &mut a);
                        a
                    };
                    let (l, rt, u, d) = (at(-r, 0.0), at(r, 0.0), at(0.0, -r), at(0.0, r));
                    // A side outside the UV island reads the texel itself: seams are not edges.
                    let val = |a: &[f32; 5], k: usize| if a[4] > 1e-6 { a[k] / a[4] } else { center.get(k).copied().unwrap_or(0.0) };
                    let mag = |k: usize| (val(&rt, k) - val(&l, k)).hypot(val(&d, k) - val(&u, k)).min(1.0);
                    // Content stays premultiplied by its own coverage; masks are one value.
                    let cov = if comps == 4 { center.get(3).copied().unwrap_or(0.0) } else { 1.0 };
                    for (k, a) in acc.iter_mut().enumerate().take(comps.min(3)) {
                        *a = mag(k) * cov;
                    }
                    if comps == 4 {
                        acc[3] = cov;
                    }
                    acc[4] = 1.0;
                }
                Op::Slope { scale, seed } => {
                    let e = 0.01;
                    let hgt = |dx: f32, dy: f32| texel_noise(x, y, [dx, dy], res, scale, seed);
                    // Downhill; a flat spot keeps its value.
                    let d = Vec2::new(hgt(-e, 0.0) - hgt(e, 0.0), hgt(0.0, -e) - hgt(0.0, e)).try_normalize().unwrap_or(Vec2::ZERO);
                    for s in 0..=n {
                        let t = s as f32 * step;
                        sample(x + d.x * t, y + d.y * t, &mut acc);
                    }
                }
                Op::Mosaic { alpha } => {
                    let center = |v: f32| ((v / r).floor() + 0.5) * r - 0.5;
                    sample(center(x), center(y), &mut acc);
                    if !alpha {
                        keep_coverage(&mut acc, comps, orig.get((row * wu + col) * comps + 3));
                    }
                }
                Op::Fxaa { alpha } => {
                    let center = orig.get((row * wu + col) * comps..(row * wu + col + 1) * comps).unwrap_or(&[]);
                    fxaa_texel((x, y), comps, center, &mut acc, &sample);
                    if !alpha {
                        keep_coverage(&mut acc, comps, center.get(3));
                    }
                }
                Op::Mirror { mirror, block } => {
                    mirror_texel(&mirror, (x, y), (block, res), comps, orig.get((row * wu + col) * comps + comps - 1), &mut acc, &sample)
                }
                Op::Transform { transform, block } => transform_texel(&transform, (x, y), (block, res), &mut acc, &sample),
                Op::SourceWarp(warp) => warp_texel(&warp, (x, y), (res, comps), &mut acc, (&sample, &input)),
                Op::SourceSlopeBlur(slope) => slope_texel(&slope, (x, y), (res, comps), &mut acc, (&sample, &input)),
                Op::HeightToNormal(filter) => {
                    let center = orig.get((row * wu + col) * comps..(row * wu + col + 1) * comps).unwrap_or(&[]);
                    height_to_normal_texel(&filter, (x, y), (res, center), &mut acc, &input);
                }
                Op::Kuwahara(kuwahara) => {
                    let radius = (kuwahara.radius.min(AnisotropicKuwahara::MAX_RADIUS) * res as f32 / SIZE_REFERENCE_RES).max(0.5);
                    kuwahara_texel(&kuwahara, (x, y), (radius, comps), &mut acc, (&sample, &input));
                }
                Op::ColorMatch(_)
                | Op::Blur(_)
                | Op::HighPass { .. }
                | Op::Bevel
                | Op::Glow(_)
                | Op::PreBlur
                | Op::Halo
                | Op::Shadow { .. }
                | Op::Outline(_)
                | Op::Distance => {}
            }
            if acc[4] > 1e-6
                && let Some(px) = line.get_mut(col * comps..(col + 1) * comps)
            {
                for (slot, a) in px.iter_mut().zip(acc) {
                    *slot += (a / acc[4] - *slot) * opacity;
                }
            }
        }
    });
    data
}

/// The linear colors `source` gives over texel region `(x0, y0, w, h)` (its tiles `tiles`), row by row
fn input_colors(inp: &Inputs, source: &Source, tiles: &[(u32, u32)], (x0, y0, w, h): (u32, u32, u32, u32)) -> Vec<[f32; 3]> {
    let parts: Vec<Vec<(u32, u32, [f32; 3])>> = tiles
        .par_iter()
        .map(|&(tx, ty)| {
            let preview_tile = inp.preview.and_then(|preview| preview.alpha.tile(tx, ty));
            tile_texels(inp.gbuf, tx, ty)
                .map(|(x, y, offset)| {
                    let mut texel = Texel::new(inp, x, y, offset, stroke_at(preview_tile, offset));
                    (x, y, texel.source(source, &Projection::default(), Sampling::default(), ChannelKind::Color))
                })
                .collect()
        })
        .collect();
    let mut colors = vec![[0.0f32; 3]; (w * h) as usize];
    for &(x, y, color) in parts.iter().flatten() {
        if let (Some(dx), Some(dy)) = (x.checked_sub(x0), y.checked_sub(y0))
            && let Some(slot) = colors.get_mut((dy * w + dx) as usize)
        {
            *slot = color;
        }
    }
    colors
}

/// Color Match's per-texel source or target colors (None: one color for the whole region)
type MatchColors = Option<Vec<[f32; 3]>>;

/// Color Match over premultiplied `data` (`comps` values a texel): each covered color moved from its source (the region's mean or its Input color) to its target (Target Color or its Image Input color), mixed in by `opacity`
fn color_match(data: &mut [f32], comps: usize, matcher: &ColorMatch, (sources, targets): (&MatchColors, &MatchColors), opacity: f32) {
    if comps != 4 || opacity <= 0.0 {
        return;
    }

    // The mean of the covered colors (premultiplied, so a texel counts by its coverage)
    let (sum, covered) = data
        .par_chunks(4)
        .fold(
            || ([0.0f64; 3], 0.0f64),
            |(mut sum, mut covered), px| {
                if let [r, g, b, a] = *px
                    && a > 0.0
                    && a.is_finite()
                {
                    for (slot, v) in sum.iter_mut().zip([r, g, b]) {
                        *slot += f64::from(v);
                    }
                    covered += f64::from(a);
                }
                (sum, covered)
            },
        )
        .reduce(|| ([0.0; 3], 0.0), |(a, ca), (b, cb)| ([a[0] + b[0], a[1] + b[1], a[2] + b[2]], ca + cb));
    let mean = if covered > 0.0 { sum.map(|v| (v / covered) as f32) } else { matcher.target_color };
    data.par_chunks_mut(4).enumerate().for_each(|(i, px)| {
        let [r, g, b, a] = *px else { return };
        if a <= 0.0 || !a.is_finite() {
            return;
        }

        let source = match matcher.source_mode {
            SourceColorMode::Average => mean,
            SourceColorMode::Parameter => matcher.source_color,
            SourceColorMode::Input => sources.as_ref().and_then(|colors| colors.get(i)).copied().unwrap_or(matcher.source_color),
        };
        let target = match matcher.target_mode {
            TargetColorMode::Parameter => matcher.target_color,
            TargetColorMode::ImageInput => targets.as_ref().and_then(|colors| colors.get(i)).copied().unwrap_or(matcher.target_color),
        };
        let moved = matcher.apply([r, g, b].map(|v| v / a), source, target);
        for (slot, v) in px.iter_mut().zip(moved) {
            *slot += (v * a - *slot) * opacity;
        }
    });
}

/// Height To Normal at texel `(x, y)` into `acc`: the 3×3 Sobel normal of the heights `height` reads, laid over normal `center` unless it overwrites it
fn height_to_normal_texel(
    filter: &HeightToNormal,
    (x, y): (f32, f32),
    (res, center): (u32, &[f32]),
    acc: &mut [f32; 5],
    height: &dyn Fn(f32, f32) -> Option<f32>,
) {
    let (Some(own), [red, green, blue, cov]) = (height(x, y), center) else { return };
    if *cov <= 0.0 {
        return;
    }

    let at = |dx: f32, dy: f32| height(x + dx, y + dy).unwrap_or(own);

    // Sobel gradients over the kernel's weight sum
    let gx = (at(1.0, -1.0) + 2.0 * at(1.0, 0.0) + at(1.0, 1.0) - at(-1.0, -1.0) - 2.0 * at(-1.0, 0.0) - at(-1.0, 1.0)) / 8.0;
    let gy = (at(-1.0, 1.0) + 2.0 * at(0.0, 1.0) + at(1.0, 1.0) - at(-1.0, -1.0) - 2.0 * at(0.0, -1.0) - at(1.0, -1.0)) / 8.0;
    let made = filter.normal([gx, gy], res).map(|v| v * 0.5 + 0.5);
    let packed = if filter.overwrite { made } else { texturecraft_core::blend::rnm([red, green, blue].map(|v| v / cov), made) };
    for (slot, value) in acc.iter_mut().zip(packed) {
        *slot = value * cov;
    }
    acc[3] = *cov;
    acc[4] = 1.0;
}

/// Gathered sample `acc` (premultiplied, then weight) recoloured over the texel's own coverage `own` (content only; a sample with no coverage leaves it)
fn keep_coverage(acc: &mut [f32; 5], comps: usize, own: Option<&f32>) {
    let Some(&own) = own.filter(|_| comps == 4) else { return };
    if acc[3] <= 1e-6 {
        acc[4] = 0.0;
        return;
    }

    let scale = own * acc[4] / acc[3];
    for component in acc.iter_mut().take(3) {
        *component *= scale;
    }
    acc[3] = own * acc[4];
}

/// Sample rows (and columns) on each side of the middle Anisotropic Kuwahara reads its ellipse with at most
const KUWAHARA_STEPS: i32 = 6;

/// Below this, a structure tensor has no direction
const MIN_TENSOR: f32 = 1e-12;

/// A spread of one in a sector, in 8-bit steps: what Anisotropic Kuwahara's sector weights are measured in
const KUWAHARA_SPREAD_SCALE: f32 = 255.0;

/// Reads a filtered texel at a point into an accumulator
type KuwaharaSample<'s> = dyn Fn(f32, f32, &mut [f32; 5]) + 's;

/// Reads the custom direction input at a point (None: none there)
type KuwaharaInput<'s> = dyn Fn(f32, f32) -> Option<f32> + 's;

/// Anisotropic Kuwahara at texel `(x, y)` (`radius` texels, `comps` components) into `acc`: an ellipse along the gray's direction, its sectors' means weighed by their calm
fn kuwahara_texel(
    kuwahara: &AnisotropicKuwahara,
    (x, y): (f32, f32),
    (radius, comps): (f32, usize),
    acc: &mut [f32; 5],
    (sample, input): (&KuwaharaSample, &KuwaharaInput),
) {
    // A straight value at an offset; None off the UV island
    let read = |fx: f32, fy: f32| -> Option<[f32; 4]> {
        let mut sum = [0.0f32; 5];
        sample(fx, fy, &mut sum);
        (sum[4] > 1e-6).then(|| std::array::from_fn(|k| if k < comps.min(4) { sum[k] / sum[4] } else { 0.0 }))
    };
    let gray = |fx: f32, fy: f32| if kuwahara.custom_input { input(fx, fy) } else { read(fx, fy).map(|px| fxaa_luma(&px[..comps.min(4)])) };

    // Structure tensor: the gray's gradients over a 3×3 grid spread by Tensor Smoothness, the middle weighing most
    let spread = kuwahara.tensor_reach() * radius;
    let (mut xx, mut xy, mut yy) = (0.0f32, 0.0f32, 0.0f32);
    for gy in -1i32..=1 {
        for gx in -1i32..=1 {
            let (px, py) = (x + gx as f32 * spread, y + gy as f32 * spread);
            let own = gray(px, py);
            let at = |dx: f32, dy: f32| gray(px + dx, py + dy).or(own);
            let (Some(left), Some(right), Some(up), Some(down)) = (at(-1.0, 0.0), at(1.0, 0.0), at(0.0, -1.0), at(0.0, 1.0)) else { continue };
            let (dx, dy) = ((right - left) * 0.5, (down - up) * 0.5);
            let weight = (4 >> (gx.abs() + gy.abs())) as f32;
            xx += weight * dx * dx;
            xy += weight * dx * dy;
            yy += weight * dy * dy;
        }
    }
    let root = ((xx - yy).powi(2) + 4.0 * xy * xy).sqrt();
    let clear = if xx + yy > MIN_TENSOR { root / (xx + yy) } else { 0.0 };

    // Strokes run across the steepest change; Anisotropy Angle turns the ellipse counterclockwise on screen
    let along = 0.5 * (2.0 * xy).atan2(xx - yy) + std::f32::consts::FRAC_PI_2 - kuwahara.angle.to_radians();
    let (sin, cos) = if along.is_finite() { along.sin_cos() } else { (0.0, 1.0) };
    let (major, minor) = kuwahara.axes(radius, if clear.is_finite() { clear } else { 0.0 });

    // Each sector's weighted sum, squared sum and weight
    let mut sums = [[0.0f32; 5]; AnisotropicKuwahara::SECTORS];
    let mut squares = [0.0f32; AnisotropicKuwahara::SECTORS];
    let steps = (major.ceil() as i32).clamp(1, KUWAHARA_STEPS);
    let exponent = kuwahara.sector_exponent();
    let colors = comps.min(3);
    for j in -steps..=steps {
        for i in -steps..=steps {
            let (u, v) = (i as f32 / steps as f32, j as f32 / steps as f32);
            let distance = u * u + v * v;
            if distance > 1.0 {
                continue;
            }

            let (ox, oy) = (u * major, v * minor);
            let Some(px) = read(x + ox * cos - oy * sin, y + ox * sin + oy * cos) else { continue };
            let falloff = (-2.0 * distance).exp();
            let heading = v.atan2(u);
            for (k, (sum, square)) in sums.iter_mut().zip(&mut squares).enumerate() {
                // The middle texel belongs to every sector; the others to the sectors they face, fading towards their sides
                let facing = (heading - k as f32 * std::f32::consts::TAU / AnisotropicKuwahara::SECTORS as f32).cos();
                let weight = if distance <= 0.0 {
                    falloff
                } else if facing > 0.0 {
                    falloff * facing.powf(exponent)
                } else {
                    continue;
                };
                for (slot, value) in sum.iter_mut().zip(&px) {
                    *slot += weight * value;
                }
                sum[4] += weight;
                *square += weight * px.iter().take(colors).map(|value| value * value).sum::<f32>();
            }
        }
    }

    // Calmer sectors weigh more, sharper the higher the Sharpness (in logs, so the powers can't overflow)
    let sharpness = kuwahara.sharpness_exponent();
    let logs: Vec<Option<f32>> = sums
        .iter()
        .zip(&squares)
        .map(|(sum, square)| {
            (sum[4] > 1e-6).then(|| {
                let mean = sum.iter().take(colors).map(|value| (value / sum[4]).powi(2)).sum::<f32>();
                let spread = (square / sum[4] - mean).max(0.0).sqrt();
                -sharpness * (1.0 + KUWAHARA_SPREAD_SCALE * spread).ln()
            })
        })
        .collect();
    let Some(top) = logs.iter().flatten().copied().reduce(f32::max) else { return };
    let mut total = [0.0f32; 5];
    for (sum, log) in sums.iter().zip(&logs) {
        let Some(log) = log else { continue };
        let weight = (log - top).exp();
        for (slot, value) in total.iter_mut().zip(sum.iter().take(4)) {
            *slot += weight * value / sum[4];
        }
        total[4] += weight;
    }
    *acc = total;
}

/// Texels an FXAA edge search walks each way along an edge at most
const FXAA_SEARCH: i32 = 8;

/// The least luma contrast around a texel FXAA smooths
const FXAA_EDGE_MIN: f32 = 0.0312;

/// The least luma contrast around a texel FXAA smooths, as a share of its brightest neighbour
const FXAA_EDGE_RELATIVE: f32 = 0.125;

/// How much a lone texel (subpixel aliasing) is blended into its neighbours at most
const FXAA_SUBPIXEL: f32 = 0.75;

/// Luma of premultiplied texel components (a mask's value)
fn fxaa_luma(px: &[f32]) -> f32 {
    match px {
        [value] => *value,
        [r, g, b, ..] => lum([*r, *g, *b]),
        _ => 0.0,
    }
}

/// FXAA (the public luma-edge method) at texel `(x, y)` with components `center` into `acc`: read across its edge by how near the edge's end is
fn fxaa_texel((x, y): (f32, f32), comps: usize, center: &[f32], acc: &mut [f32; 5], sample: &dyn Fn(f32, f32, &mut [f32; 5])) {
    let own = fxaa_luma(center);

    // Luma at an offset from the texel; off the UV island the texel's own, so seams are not edges
    let luma = |dx: f32, dy: f32| {
        let mut read = [0.0f32; 5];
        sample(x + dx, y + dy, &mut read);
        if read[4] <= 1e-6 {
            return own;
        }

        let mut px = [0.0f32; 4];
        for (slot, v) in px.iter_mut().zip(&read).take(comps.min(4)) {
            *slot = v / read[4];
        }
        fxaa_luma(&px[..comps.min(4)])
    };
    let (n, s, w, e) = (luma(0.0, -1.0), luma(0.0, 1.0), luma(-1.0, 0.0), luma(1.0, 0.0));
    let (hi, lo) = (own.max(n).max(s).max(w).max(e), own.min(n).min(s).min(w).min(e));
    let range = hi - lo;
    if !range.is_finite() || range < FXAA_EDGE_MIN.max(hi * FXAA_EDGE_RELATIVE) {
        return;
    }

    let (nw, ne, sw, se) = (luma(-1.0, -1.0), luma(1.0, -1.0), luma(-1.0, 1.0), luma(1.0, 1.0));

    // An edge running across (values changing down the rows) or down (values changing along them)
    let across = 2.0 * (n - 2.0 * own + s).abs() + (nw - 2.0 * w + sw).abs() + (ne - 2.0 * e + se).abs();
    let down = 2.0 * (w - 2.0 * own + e).abs() + (nw - 2.0 * n + ne).abs() + (sw - 2.0 * s + se).abs();
    let horizontal = across >= down;
    let (before, after) = if horizontal { (n, s) } else { (w, e) };
    let (sign, side) = if (before - own).abs() >= (after - own).abs() { (-1.0, before) } else { (1.0, after) };
    let (perp, along) = if horizontal { ([0.0, sign], [1.0, 0.0]) } else { ([sign, 0.0], [0.0, 1.0]) };

    // Walk the edge's midline both ways until its luma leaves the edge's
    let edge = 0.5 * (own + side);
    let enough = 0.25 * (side - own).abs();
    let walk = |way: f32| {
        let mut end = (FXAA_SEARCH as f32, 0.0);
        for step in 1..=FXAA_SEARCH {
            let t = step as f32 * way;
            let delta = luma(perp[0] * 0.5 + along[0] * t, perp[1] * 0.5 + along[1] * t) - edge;
            end = (step as f32, delta);
            if delta.abs() >= enough {
                break;
            }
        }
        end
    };
    let ((back, back_delta), (ahead, ahead_delta)) = (walk(-1.0), walk(1.0));
    let (nearest, delta) = if back < ahead { (back, back_delta) } else { (ahead, ahead_delta) };
    let edge_blend = if (delta < 0.0) != (own < edge) { 0.5 - nearest / (back + ahead) } else { 0.0 };

    // A texel unlike all its neighbours blends in by its own contrast (sides weigh twice the corners)
    let average = (2.0 * (n + s + w + e) + nw + ne + sw + se) / 12.0;
    let contrast = ((average - own).abs() / range).clamp(0.0, 1.0);
    let eased = crate::pattern::smoothstep(0.0, 1.0, contrast);
    let blend = edge_blend.max(eased * eased * FXAA_SUBPIXEL);
    if blend > 0.0 {
        sample(x + perp[0] * blend, y + perp[1] * blend, acc);
    }
}

/// Mirror `mirror` at texel `(x, y)` into `acc`: the texel it reads in its UV tile (`res` texels from its `block`'s top left), or white on a debug axis
fn mirror_texel(
    mirror: &Mirror,
    (x, y): (f32, f32),
    (block, res): (u32, u32),
    comps: usize,
    alpha: Option<&f32>,
    acc: &mut [f32; 5],
    sample: &dyn Fn(f32, f32, &mut [f32; 5]),
) {
    let (block, size) = (block.max(1) as f32, res.max(1) as f32);
    let (origin_x, origin_y) = ((x / block).floor() * block, (y / block).floor() * block);
    let (local_x, local_y) = (x - origin_x, y - origin_y);
    if local_x >= size || local_y >= size {
        return;
    }

    let uv = [(local_x + 0.5) / size, (local_y + 0.5) / size];
    let line = (Mirror::DEBUG_HALF_WIDTH * size / SIZE_REFERENCE_RES).max(0.5) / size;
    if mirror.debug && mirror.on_axis(uv, line) {
        // Content stays premultiplied by its own coverage; masks are one value
        let cover = if comps == 4 { alpha.copied().unwrap_or(0.0) } else { 1.0 };
        for slot in acc.iter_mut().take(comps) {
            *slot = cover;
        }
        acc[4] = 1.0;
        return;
    }
    let [u, v] = mirror.source(uv);

    // Whole texels, so an even mirror copies texels exactly
    let at = |origin: f32, along: f32| origin + (along * size - 0.5).round().clamp(0.0, size - 1.0);
    sample(at(origin_x, u), at(origin_y, v), acc);
}

/// Transform `transform` at texel `(x, y)` into `acc`: bilinear between the four texels round where it reads in its UV tile (`res` texels from its `block`'s top left), each wrapped round the tile; where it does not repeat, a read past the tile counts as empty
fn transform_texel(transform: &TileTransform, (x, y): (f32, f32), (block, res): (u32, u32), acc: &mut [f32; 5], sample: &dyn Fn(f32, f32, &mut [f32; 5])) {
    let (block, size) = (block.max(1) as f32, res.max(1) as f32);
    let (origin_x, origin_y) = ((x / block).floor() * block, (y / block).floor() * block);
    let (local_x, local_y) = (x - origin_x, y - origin_y);
    if local_x >= size || local_y >= size {
        return;
    }

    let (columns, shift) = transform.affine();
    let uv = [(local_x + 0.5) / size, (local_y + 0.5) / size];

    // Texel space of the tile, centres at whole numbers
    let at = [0, 1].map(|axis| (columns[0][axis] * uv[0] + columns[1][axis] * uv[1] + shift[axis]) * size - 0.5);
    if !at.iter().all(|a| a.is_finite()) {
        return;
    }

    let (base_x, base_y) = (at[0].floor(), at[1].floor());
    let (tx, ty) = (at[0] - base_x, at[1] - base_y);
    let repeat = transform.repeat == RepeatMode::HorizontallyAndVertically;
    let mut total = [0.0f32; 5];
    for (dx, dy, k) in [(0.0, 0.0, (1.0 - tx) * (1.0 - ty)), (1.0, 0.0, tx * (1.0 - ty)), (0.0, 1.0, (1.0 - tx) * ty), (1.0, 1.0, tx * ty)] {
        if k <= 0.0 {
            continue;
        }

        let (cx, cy) = (base_x + dx, base_y + dy);
        let inside = (0.0..size).contains(&cx) && (0.0..size).contains(&cy);
        if !repeat && !inside {
            // Empty: no value, full weight
            total[4] += k;
            continue;
        }

        // One whole texel: the sample's own weights, so this matches its bilinear read
        let mut one = [0.0f32; 5];
        sample(origin_x + cx.rem_euclid(size), origin_y + cy.rem_euclid(size), &mut one);
        for (slot, value) in total.iter_mut().zip(one) {
            *slot += value * k;
        }
    }
    *acc = total;
}

/// Fixed-point scale of the blur. Integer running sums make a texel's result independent of
/// the region it is computed in, so tiles composited in any grouping agree exactly.
const FIX: f64 = (1u64 << 24) as f64;

/// Three running-sum box blurs (≈ Gaussian) of a `width × height` image along its rows, then its columns, each in parallel.
pub(crate) fn blur(data: &mut [f32], width: usize, height: usize, radius: usize) {
    let to_fixed = |value: &f32| if value.is_finite() { (f64::from(value.clamp(-1e6, 1e6)) * FIX).round() as i64 } else { 0 };
    let mut fixed: Vec<i64> = data.par_iter().map(to_fixed).collect();
    blur_rows(&mut fixed, width, radius);

    let mut columns = transpose(&fixed, width, height);
    blur_rows(&mut columns, height, radius);
    data.par_iter_mut().zip(transpose(&columns, height, width)).for_each(|(target, blurred)| *target = (blurred as f64 / FIX) as f32);
}

/// The box blurs `blur` runs along each axis
pub(crate) const BLUR_BOX_PASSES: usize = 3;

/// Three box blurs of radius `r` along every row (`w` long) of `data`.
fn blur_rows(data: &mut [i64], w: usize, r: usize) {
    if w == 0 {
        return;
    }
    data.par_chunks_mut(w).for_each(|row| {
        let mut tmp = vec![0i64; row.len()];
        for _ in 0..BLUR_BOX_PASSES {
            box_line(row, &mut tmp, r);
            row.copy_from_slice(&tmp);
        }
    });
}

/// One box blur of radius `r` of a line (texels past the ends count as 0).
fn box_line(src: &[i64], dst: &mut [i64], r: usize) {
    let n = (2 * r + 1) as i64;
    // Rounded division of a window sum.
    let avg = |acc: i64| (2 * acc + n).div_euclid(2 * n);
    let mut acc: i64 = src.iter().take(r + 1).sum();
    for (i, d) in dst.iter_mut().enumerate() {
        *d = avg(acc);
        acc += src.get(i + r + 1).copied().unwrap_or(0);
        if i >= r {
            acc -= src.get(i - r).copied().unwrap_or(0);
        }
    }
}

/// `src` (`w × h`, row-major) transposed to `h × w`.
fn transpose(src: &[i64], w: usize, h: usize) -> Vec<i64> {
    let mut out = vec![0i64; src.len()];
    if h > 0 {
        out.par_chunks_mut(h).enumerate().for_each(|(x, col)| {
            for (y, d) in col.iter_mut().enumerate() {
                *d = src.get(y * w + x).copied().unwrap_or(0);
            }
        });
    }
    out
}

/// Covered texels of tile (tx, ty): (x, y, offset inside the tile).
fn tile_texels(gbuf: &UvGBuffer, tx: u32, ty: u32) -> impl Iterator<Item = (u32, u32, usize)> + '_ {
    let (w, h) = (gbuf.width, gbuf.height);
    (0..TILE).flat_map(|ly| (0..TILE).map(move |lx| (lx, ly))).filter_map(move |(lx, ly)| {
        let (x, y) = (tx * TILE + lx, ty * TILE + ly);
        (x < w && y < h && gbuf.tri.get((y * w + x) as usize).is_some_and(|t| *t != NO_TRI)).then_some((x, y, (ly * TILE + lx) as usize))
    })
}

/// Live stroke alpha at offset `o` of a preview tile.
fn stroke_at(preview: Option<&[u16]>, o: usize) -> f32 {
    preview.and_then(|t| t.get(o)).map(|v| from_u16(*v)).unwrap_or(0.0)
}

/// The full stack of tile (tx, ty) in `channels` and the channels the layer's mask compares, when the layer reads the stack below it (else empty)
fn anchor_plan<'a>(inp: &Inputs<'a>, id: LayerId, channels: &[Channel], tx: u32, ty: u32, fields: Option<&'a Fields>) -> Vec<Node<'a>> {
    let Ok(set) = inp.project.texture_set(inp.ts) else { return Vec::new() };
    if !reads_stack(inp.project, id) {
        return Vec::new();
    }

    let mut planned = channels.to_vec();
    for effect in layer_mask(inp.project, id).iter().flat_map(|m| m.effects.iter()) {
        if let EffectKind::Compare { channel, .. } = effect.kind
            && !planned.contains(&channel)
        {
            planned.push(channel);
        }
    }
    plan(inp, &set.stack, &planned, tx, ty, 0, fields)
}

/// Whether layer `id` (its own mask, or the content it draws) reads the stack below it: an anchor point, or a compare mask's layer sides
pub(crate) fn reads_stack(p: &Project, id: LayerId) -> bool {
    p.layers.get(&id).is_some_and(Layer::reads_stack) || p.content_layer(id).is_some_and(Layer::reads_stack)
}

/// The mask layer `id` draws with: its own, or an instance source's.
fn layer_mask(p: &Project, id: LayerId) -> Option<&Mask> {
    p.layers.get(&p.mask_owner(id))?.mask.as_ref()
}

/// Layers of `list` in drawing order (bottom → top, folder and instanced folder contents after
/// their folder).
fn draw_order(p: &Project, list: &[LayerId], depth: usize, out: &mut Vec<LayerId>) {
    if depth > texturecraft_core::project::MAX_DEPTH {
        return;
    }
    for id in list {
        out.push(*id);
        if let Some(l) = p.content_layer(*id) {
            draw_order(p, l.children(), depth + 1, out);
        }
    }
}

/// What a field holds.
#[derive(Clone, Copy)]
enum Job<'a> {
    Mask(&'a Mask),
    Content(Channel),
}

/// A layer's mask or content channel after every neighbourhood filter of its stack, over `tiles`.
fn field(inp: &Inputs, channels: &[Channel], below: &Fields, id: LayerId, job: Job, tiles: &[(u32, u32)]) -> Option<Field> {
    let effects = match job {
        Job::Mask(mask) => &mask.effects,
        Job::Content(_) => &inp.project.content_layer(id)?.content_effects,
    };
    let filters: Vec<(usize, f32, Op)> = neighbourhood_filters(effects, inp.tile_res()).collect();
    let longest_side = inp.gbuf.width.max(inp.gbuf.height);

    // Each filter's region also holds what every filter above it reads around the tiles
    let mut margins: Vec<u32> = filters.iter().map(|&(_, radius, op)| filter_extent(radius, op, longest_side).1.div_ceil(TILE)).collect();
    for index in (0..margins.len().saturating_sub(1)).rev() {
        margins[index] += margins[index + 1];
    }

    // A wrapping filter reads across the set's edges, so it and every filter under it cover the whole set along an axis it crosses
    let mut wrapping: Vec<bool> = filters.iter().map(|filter| wraps(effects, filter.0)).collect();
    for index in (0..wrapping.len().saturating_sub(1)).rev() {
        wrapping[index] |= wrapping[index + 1];
    }

    let mut previous: Option<(usize, Field)> = None;
    for ((&filter, &margin), &wrap) in filters.iter().zip(&margins).zip(&wrapping) {
        let region = region_around(inp, tiles, margin, wrap)?;
        let start = previous.as_ref().map(|(index, field)| (*index, field));
        let filtered = filter_region(inp, channels, below, id, job, effects, filter, region, start)?;
        previous = Some((filter.0, filtered));
    }
    previous.map(|(_, field)| field)
}

/// The tile-aligned texel region `(x0, y0, w, h)` around `tiles` plus `margin` tiles, clipped to the set; with `wrap`, an axis the margin crosses an edge of spans the whole set (the filter reads the far side there)
fn region_around(inp: &Inputs, tiles: &[(u32, u32)], margin: u32, wrap: bool) -> Option<(u32, u32, u32, u32)> {
    let (first_x, first_y) = tiles.iter().fold((u32::MAX, u32::MAX), |(min_x, min_y), tile| (min_x.min(tile.0), min_y.min(tile.1)));
    let (last_x, last_y) = tiles.iter().fold((0, 0), |(max_x, max_y), tile| (max_x.max(tile.0), max_y.max(tile.1)));
    let axis = |first: u32, last: u32, size: u32| {
        let end = last.saturating_add(1).saturating_add(margin).saturating_mul(TILE);
        if wrap && (first < margin || end > size) {
            return (0, size);
        }

        (first.saturating_sub(margin).saturating_mul(TILE), end.min(size))
    };
    let ((x0, x1), (y0, y1)) = (axis(first_x, last_x, inp.gbuf.width), axis(first_y, last_y, inp.gbuf.height));
    let (w, h) = (x1.saturating_sub(x0), y1.saturating_sub(y0));
    (w > 0 && h > 0).then_some((x0, y0, w, h))
}

/// Whether the neighbourhood filter at `index` among the visible effects reads past the set's edges from the far side
fn wraps(effects: &[Effect], index: usize) -> bool {
    matches!(
        effects.iter().filter(|effect| effect.visible).nth(index).map(|effect| &effect.kind),
        Some(EffectKind::Filter(
            Filter::Blur { wrap: true, .. }
                | Filter::Bevel { wrap: true, .. }
                | Filter::DistanceBevel(Bevel { wrap: true, .. })
                | Filter::BevelSmooth(BevelSmooth { wrap: true, .. })
        ))
    )
}

/// `data` (`w` × `h` texels of `comps` values) with `pad_x` and `pad_y` texels added around it, read from the opposite side
fn wrap_pad(data: &[f32], comps: usize, (w, h): (usize, usize), (pad_x, pad_y): (usize, usize)) -> Vec<f32> {
    let (padded_w, padded_h) = (w + 2 * pad_x, h + 2 * pad_y);
    let mut out = vec![0.0f32; padded_w * padded_h * comps];
    out.par_chunks_mut(padded_w * comps).enumerate().for_each(|(y, row)| {
        let source_y = (y + h * pad_y.div_ceil(h.max(1)) - pad_y) % h.max(1);
        for (x, pixel) in row.chunks_mut(comps).enumerate() {
            let source_x = (x + w * pad_x.div_ceil(w.max(1)) - pad_x) % w.max(1);
            let at = (source_y * w + source_x) * comps;
            if let Some(source) = data.get(at..at + comps) {
                pixel.copy_from_slice(source);
            }
        }
    });
    out
}

/// The `w` × `h` texels inside [`wrap_pad`]'s border
fn unpad(data: &[f32], comps: usize, (w, h): (usize, usize), (pad_x, pad_y): (usize, usize)) -> Vec<f32> {
    let padded_w = w + 2 * pad_x;
    (pad_y..pad_y + h).flat_map(|y| data.get((y * padded_w + pad_x) * comps..(y * padded_w + pad_x + w) * comps).unwrap_or_default()).copied().collect()
}

/// A neighbourhood filter's radius in whole texels and how many texels around a texel it reads.
fn filter_extent(radius: f32, op: Op, longest_side: u32) -> (u32, u32) {
    // Clamped so a hostile radius can't overflow the region arithmetic
    let radius_texels = radius.round().clamp(1.0, longest_side as f32) as u32;

    // Three box passes reach three radii; the gathering filters one radius plus a bilinear texel
    let reach = if is_box_blur(op) { 3 * radius_texels } else { radius_texels + 1 };

    // A shadow also reads the shapes it was moved from
    let moved = match op {
        Op::Shadow { offset: [dx, dy] } => dx.unsigned_abs().max(dy.unsigned_abs()).min(longest_side),
        _ => 0,
    };
    (radius_texels, reach + moved)
}

/// One evaluated texel of a filter's region: its position, its premultiplied value and the gray its image input reads there
type RegionTexel = (u32, u32, [f32; 4], f32);

/// Whether neighbourhood filter `op` is computed from box blurs (the others gather samples)
fn is_box_blur(op: Op) -> bool {
    matches!(op, Op::Blur(_) | Op::HighPass { .. } | Op::Bevel | Op::Glow(_) | Op::PreBlur | Op::Halo | Op::Shadow { .. } | Op::Outline(_) | Op::Distance)
}

/// The stack up to and including neighbourhood filter `filter` over texel `region`, continued from `start` (the filter below's field) when given
#[allow(clippy::too_many_arguments)]
fn filter_region(
    inp: &Inputs,
    channels: &[Channel],
    below: &Fields,
    id: LayerId,
    job: Job,
    effects: &[Effect],
    (index, radius, op): (usize, f32, Op),
    (rx0, ry0, w, h): (u32, u32, u32, u32),
    start: Option<(usize, &Field)>,
) -> Option<Field> {
    let channel = match job {
        Job::Mask(_) => None,
        Job::Content(channel) => Some(channel),
    };
    let opacity = effects.iter().filter(|effect| effect.visible).nth(index).map(|effect| effect.applied_blend(channel).opacity.clamp(0.0, 1.0)).unwrap_or(1.0);

    // Height To Normal makes the Normal only (and reads the layer's Height as its gray); other channels pass
    let heights = matches!(op, Op::HeightToNormal(_)) && channel == Some(Channel::Normal);
    let opacity = if matches!(op, Op::HeightToNormal(_)) && !heights { 0.0 } else { opacity };

    // Color Match moves colors only; other channels pass
    let opacity = if matches!(op, Op::ColorMatch(_)) && channel.map(Channel::kind) != Some(ChannelKind::Color) { 0.0 } else { opacity };
    let (radius_texels, reach) = filter_extent(radius, op, inp.gbuf.width.max(inp.gbuf.height));
    let comps = match job {
        Job::Mask(_) => 1,
        Job::Content(_) => 4,
    };

    // The gray the filter's image input reads (its first; the filters that read one have one)
    let input = effects.iter().filter(|effect| effect.visible).nth(index).and_then(|effect| match &effect.kind {
        EffectKind::Filter(filter) => ImageInput::of(filter).first().map(|input| effect.input(input)),
        _ => None,
    });

    // Evaluate the stack up to the filter, tile by tile (content premultiplied by coverage), and its image input
    let (rx1, ry1) = (rx0 + w, ry0 + h);
    let region_tiles: Vec<(u32, u32)> = (ry0 / TILE..ry1.div_ceil(TILE)).flat_map(|y| (rx0 / TILE..rx1.div_ceil(TILE)).map(move |x| (x, y))).collect();
    let parts: Vec<Vec<RegionTexel>> = region_tiles
        .par_iter()
        .map(|&(tx, ty)| {
            let full = anchor_plan(inp, id, channels, tx, ty, Some(below));
            let preview_tile = inp.preview.and_then(|preview| preview.alpha.tile(tx, ty));
            let height_nodes = if heights { plan(inp, &[id], &[Channel::Height], tx, ty, 0, Some(below)) } else { Vec::new() };
            let mut mask = None;
            let mut nodes = Vec::new();
            match job {
                Job::Mask(layer_mask) => {
                    let Some(mut mask_stack) = mask_plan(inp, id, layer_mask, tx, ty, None, inp.project.contents_around(id)) else { return Vec::new() };

                    mask_stack.effects.truncate(index);
                    mask_stack.start = start;
                    mask = Some(mask_stack);
                }
                Job::Content(channel) => {
                    nodes = plan(inp, &[id], &[channel], tx, ty, 0, Some(below));
                    if let Some(chan) = nodes.first_mut().and_then(|node| node.chans.first_mut()) {
                        chan.effects.truncate(index);
                        chan.start = start;
                    }
                }
            }
            tile_texels(inp.gbuf, tx, ty)
                .map(|(x, y, offset)| {
                    let mut texel = Texel::new(inp, x, y, offset, stroke_at(preview_tile, offset));
                    if !full.is_empty() {
                        texel.anchors_below(&full, id);
                    }
                    let value = match (&mask, nodes.first()) {
                        (Some(mask_stack), _) => [texel.mask(mask_stack), 0.0, 0.0, 0.0],
                        (None, Some(node)) => match node.chans.first().and_then(|chan| texel.content(&node.content, chan)) {
                            Some((color, cov)) => [color[0] * cov, color[1] * cov, color[2] * cov, cov],
                            None => [0.0; 4],
                        },
                        (None, None) => [0.0; 4],
                    };
                    let gray = match height_nodes.first() {
                        // Heights are signed and premultiplied by their coverage, so an edge slopes down to 0
                        Some(node) => {
                            let mut height = Texel::new(inp, x, y, offset, stroke_at(preview_tile, offset));
                            if !full.is_empty() {
                                height.anchors_below(&full, id);
                            }
                            node.chans.first().and_then(|chan| height.content(&node.content, chan)).map_or(0.0, |(value, cov)| value[0] * cov)
                        }
                        None => {
                            let gray =
                                input.as_ref().map_or(1.0, |source| texel.source(source, &Projection::default(), Sampling::default(), ChannelKind::Scalar)[0]);
                            gray.clamp(0.0, 1.0)
                        }
                    };
                    (x, y, value, gray)
                })
                .collect()
        })
        .collect();

    // Scatter into the region, a row of tiles at a time (the region is tile aligned)
    let texel_count = (w * h) as usize;
    let mut orig = vec![0.0f32; texel_count * comps];
    let mut cover = vec![0.0f32; texel_count];
    let mut grays = vec![0.0f32; texel_count];
    let row_tiles = (rx1.div_ceil(TILE) - rx0 / TILE).max(1) as usize;
    let band = (TILE * w) as usize;
    orig.par_chunks_mut(band * comps).zip(cover.par_chunks_mut(band)).zip(grays.par_chunks_mut(band)).zip(parts.par_chunks(row_tiles)).for_each(
        |(((orig, cover), grays), row)| {
            for &(x, y, value, gray) in row.iter().flatten() {
                let texel = (((y - ry0) % TILE) * w + (x - rx0)) as usize;
                if let (Some(covered), Some(read), Some(pixel)) = (cover.get_mut(texel), grays.get_mut(texel), orig.get_mut(texel * comps..(texel + 1) * comps))
                {
                    *covered = 1.0;
                    *read = gray;
                    pixel.copy_from_slice(&value[..comps]);
                }
            }
        },
    );

    // Color Match's Input and Image Input colors over the region, when its modes read them
    let match_inputs = match (op, effects.iter().filter(|effect| effect.visible).nth(index)) {
        (Op::ColorMatch(matcher), Some(effect)) if opacity > 0.0 => {
            let read = |on: bool, input: ImageInput| on.then(|| input_colors(inp, &effect.input(&input), &region_tiles, (rx0, ry0, w, h)));
            [
                read(matcher.source_mode == SourceColorMode::Input, ImageInput::SOURCE_INPUT),
                read(matcher.target_mode == TargetColorMode::ImageInput, ImageInput::TARGET_INPUT),
            ]
        }
        _ => [None, None],
    };

    // High pass centers on the channel's mid value (0 for signed channels like height)
    let mid = match job {
        Job::Content(channel) if channel.kind() == ChannelKind::Signed => 0.0,
        _ => 0.5,
    };

    // A Highpass in sRGB filters its colors encoded (decoded again below); one with Apply To Alpha keeps the colors on the new coverage
    let (space, highpass_alpha) = match (op, job) {
        (Op::HighPass { space, alpha }, Job::Content(channel)) if comps == 4 => {
            (if channel.kind() == ChannelKind::Color { space } else { HighpassSpace::Working }, alpha)
        }
        _ => (HighpassSpace::Working, false),
    };
    let covered = highpass_alpha.then(|| orig.clone());
    recode(&mut orig, comps, |rgb| space.encode(rgb));

    // Wrapping pads an axis the region spans whole by the filter's reach
    let wrap = wraps(effects, index);
    let pad_x = if wrap && rx0 == 0 && w == inp.gbuf.width { reach.min(w) } else { 0 };
    let pad_y = if wrap && ry0 == 0 && h == inp.gbuf.height { reach.min(h) } else { 0 };

    // A smaller UV tile has fewer texels per UV unit, so its filters reach fewer texels
    let filtered = |scale: f32| -> Vec<f32> {
        let res = ((inp.tile_res() as f32 * scale).round() as u32).max(1);
        let scaled = neighbourhood_filters(effects, res).find(|filter| filter.0 == index);
        let scaled_radius = scaled.map_or(radius_texels, |filter| filter.1.round().clamp(1.0, radius_texels as f32) as u32);
        if let Op::ColorMatch(matcher) = op {
            let mut data = orig.clone();
            color_match(&mut data, comps, &matcher, (&match_inputs[0], &match_inputs[1]), opacity);
            return data;
        }

        if !is_box_blur(op) {
            return gather(op, (&orig, &cover, &grays), comps, (rx0, ry0, w, h), scaled_radius as f32, res, opacity);
        }

        // Weighted so uncovered texels don't bleed in; a wrapping filter reads a border copied from the far side
        let pad = (pad_x as usize, pad_y as usize);
        let (orig, cover, grays) = if pad == (0, 0) {
            (Cow::Borrowed(&orig), Cow::Borrowed(&cover), Cow::Borrowed(&grays))
        } else {
            let padded = |values: &[f32], comps: usize| Cow::Owned(wrap_pad(values, comps, (w as usize, h as usize), pad));
            (padded(&orig, comps), padded(&cover, 1), padded(&grays, 1))
        };
        let (width, height, blur_radius) = (w as usize + 2 * pad.0, h as usize + 2 * pad.1, scaled_radius as usize);
        if let (Some((_, _, scaled_op @ (Op::Halo | Op::Shadow { .. }))), Some(Effect { kind: EffectKind::Filter(filter), .. })) =
            (scaled, effects.iter().filter(|effect| effect.visible).nth(index))
        {
            let kind = channel.map_or(ChannelKind::Scalar, Channel::kind);
            let mut data = orig.to_vec();
            halo(&mut data, &cover, comps, (width, height, blur_radius), (scaled_op, filter, kind), opacity);
            return if pad == (0, 0) { data } else { unpad(&data, comps, (w as usize, h as usize), pad) };
        }
        if let (Some((_, _, Op::Distance)), Some(Effect { kind: EffectKind::Filter(filter), .. })) =
            (scaled, effects.iter().filter(|effect| effect.visible).nth(index))
        {
            let mut data = orig.to_vec();
            distance_filter(&mut data, (&cover, &grays), comps, (width, height), filter, res, opacity);
            return if pad == (0, 0) { data } else { unpad(&data, comps, (w as usize, h as usize), pad) };
        }
        if let Some((_, _, Op::Outline(outline))) = scaled {
            let mut data = orig.to_vec();
            mask_outline(&mut data, &cover, comps, (width, height, blur_radius), &outline, opacity);
            return if pad == (0, 0) { data } else { unpad(&data, comps, (w as usize, h as usize), pad) };
        }

        // A pre-blur takes the blur whole; the effect's opacity mixes its mapped result below
        let blurred_opacity = if matches!(op, Op::PreBlur) { 1.0 } else { opacity };
        let mut weights = cover.to_vec();
        blur(&mut weights, width, height, blur_radius);
        let mut data = orig.to_vec();
        for component in 0..comps {
            let mut sums: Vec<f32> = orig.par_chunks(comps).map(|pixel| pixel.get(component).copied().unwrap_or(0.0)).collect();
            blur(&mut sums, width, height, blur_radius);
            data.par_chunks_mut(comps).zip(sums.par_iter()).zip(weights.par_iter()).for_each(|((pixel, sum), weight)| {
                // Content is premultiplied, so the mid value is too (coverage itself is kept)
                let cov = if comps == 4 { pixel.get(3).copied().unwrap_or(0.0) } else { 1.0 };
                if *weight > 1e-6
                    && let Some(slot) = pixel.get_mut(component)
                {
                    let (original, blurred) = (*slot, sum / weight);
                    let result = match op {
                        Op::Blur(Some(amount)) => original + amount * (original - blurred),
                        Op::HighPass { .. } if component < 3 => original - blurred + mid * cov,
                        Op::HighPass { alpha: true, .. } => (original - blurred + 0.5).clamp(0.0, 1.0),
                        // Away from 0 by at most the value itself: 0 at the edge, full inside
                        Op::Bevel if component < 3 => original.signum() * original.abs().min((2.0 * blurred * original.signum() - original.abs()).max(0.0)),
                        Op::Glow(amount) if component < 3 => original + amount * blurred,
                        Op::HighPass { .. } | Op::Bevel | Op::Glow(_) => original,
                        _ => blurred,
                    };
                    *slot = original + (result - original) * blurred_opacity;
                }
            });
        }
        if matches!(op, Op::PreBlur)
            && let Some(Effect { kind: EffectKind::Filter(filter), .. }) = effects.iter().filter(|effect| effect.visible).nth(index)
        {
            let kind = channel.map_or(ChannelKind::Scalar, Channel::kind);
            map_blurred(&mut data, &orig, comps, filter, kind, opacity);
        }
        if pad == (0, 0) { data } else { unpad(&data, comps, (w as usize, h as usize), pad) }
    };

    // Each block keeps the result at its tile's scale (blocks are TILE aligned)
    let scales = inp.project.texture_set(inp.ts).map(TextureSet::block_scales).unwrap_or_default();
    let mut levels: Vec<f32> =
        (ry0..ry1).step_by(TILE as usize).flat_map(|y| (rx0..rx1).step_by(TILE as usize).map(move |x| (x, y))).map(|(x, y)| scales.at(x, y)).collect();
    levels.sort_by(f32::total_cmp);
    levels.dedup();
    let (first, rest) = levels.split_first().map_or((1.0, &[][..]), |(first, rest)| (*first, rest));
    let mut data = filtered(first);
    for &scale in rest {
        let other = filtered(scale);
        let row = w as usize * comps;
        data.par_chunks_mut(row).zip(other.par_chunks(row)).enumerate().for_each(|(y, (dst, src))| {
            for (x, (target, source)) in dst.chunks_mut(comps).zip(src.chunks(comps)).enumerate() {
                if scales.at(rx0 + x as u32, ry0 + y as u32) == scale {
                    target.copy_from_slice(source);
                }
            }
        });
    }

    recode(&mut data, comps, |rgb| space.decode(rgb));
    if let Some(before) = covered {
        recover(&mut data, &before, mid);
    }
    unpremultiply(&mut data, comps);
    Some(Field { x0: rx0, y0: ry0, w, h, comps, data })
}

/// Straight colours a colour space map must leave alone to be the identity
const IDENTITY_PROBE: [f32; 3] = [0.25, 0.5, 0.75];

/// Premultiplied content `data` (four components a texel) with each straight color mapped by `map` (nothing for masks or an identity space)
fn recode(data: &mut [f32], comps: usize, map: impl Fn([f32; 3]) -> [f32; 3] + Sync) {
    if comps != 4 || map(IDENTITY_PROBE) == IDENTITY_PROBE {
        return;
    }

    data.par_chunks_mut(4).for_each(|pixel| {
        if let [r, g, b, a] = pixel
            && *a > 1e-6
        {
            let mapped = map([*r / *a, *g / *a, *b / *a]);
            [*r, *g, *b] = mapped.map(|component| component * *a);
        }
    });
}

/// Premultiplied content `data` whose coverage a Highpass changed: its colors laid on the new coverage (mid gray where there was none, as `before`)
fn recover(data: &mut [f32], before: &[f32], mid: f32) {
    data.par_chunks_mut(4).zip(before.par_chunks(4)).for_each(|(pixel, old)| {
        let (Some(&own), Some(&new)) = (old.get(3), pixel.get(3)) else { return };
        let scale = if own > 1e-6 { new / own } else { 0.0 };
        for component in pixel.iter_mut().take(3) {
            *component = if own > 1e-6 { *component * scale } else { mid * new };
        }
    });
}

/// Lay halo glow or drop shadow `filter` (`op` its move) over premultiplied `data` in the tint of a channel of `kind`, mixed in by `opacity`
fn halo(
    data: &mut [f32],
    cover: &[f32],
    comps: usize,
    (width, height, radius): (usize, usize, usize),
    (op, filter, kind): (Op, &Filter, ChannelKind),
    opacity: f32,
) {
    let (inner, precise, spread, strength, color, mask_white) = match *filter {
        Filter::HaloGlow { on_mask, precise, width, spread, opacity, color } => (width < 0.0, precise, spread, opacity, color, on_mask),
        Filter::DropShadow { spread, opacity, color, .. } => (false, false, spread, opacity, color, true),
        _ => return,
    };

    // Rec. 709 luminance
    let luminance = 0.2126 * color[0] + 0.7152 * color[1] + 0.0722 * color[2];
    let tint = match (comps, kind) {
        (4, ChannelKind::Color) => color,
        (4, _) => [luminance; 3],
        _ if mask_white => [1.0; 3],
        _ => [luminance; 3],
    };

    // The shapes: the content's coverage or the mask's value (the outside of them for an inner glow)
    let shape = |texel: usize| {
        let value = data.get(texel * comps + comps - 1).copied().unwrap_or(0.0);
        let value = if value.is_finite() { value.clamp(0.0, 1.0) } else { 0.0 };
        if inner { 1.0 - value } else { value }
    };
    let offset = match op {
        Op::Shadow { offset } => offset,
        _ => [0, 0],
    };
    let moved = |texel: usize| -> Option<usize> {
        let (x, y) = ((texel % width.max(1)) as i64 - i64::from(offset[0]), (texel / width.max(1)) as i64 - i64::from(offset[1]));
        (x >= 0 && y >= 0 && (x as usize) < width && (y as usize) < height).then(|| y as usize * width + x as usize)
    };
    let texels = width * height;
    let mut weights: Vec<f32> = (0..texels).map(|texel| moved(texel).and_then(|from| cover.get(from).copied()).unwrap_or(0.0)).collect();
    let mut sums: Vec<f32> = (0..texels).map(|texel| moved(texel).map_or(0.0, |from| shape(from) * cover.get(from).copied().unwrap_or(0.0))).collect();
    blur(&mut weights, width, height, radius);
    blur(&mut sums, width, height, radius);

    let hardness = (1.0 - spread.clamp(0.0, 1.0)).max(HALO_SOFTEST);
    let strength = strength.clamp(0.0, 1.0);
    data.par_chunks_mut(comps).zip(sums.par_iter().zip(weights.par_iter())).for_each(|(pixel, (sum, weight))| {
        let blurred = if *weight > 1e-6 { (sum / weight).clamp(0.0, 1.0) } else { 0.0 };
        let shaped = if precise { crate::pattern::smoothstep(0.0, PRECISE_EDGE, blurred) } else { blurred };
        let glow = (shaped / hardness).min(1.0) * strength;
        let cov = pixel.last().copied().unwrap_or(0.0).clamp(0.0, 1.0);
        let mut before = [0.0f32; 4];
        for (was, value) in before.iter_mut().zip(pixel.iter()) {
            *was = *value;
        }
        if inner {
            // Inside the shapes, towards the tint (content premultiplied by its coverage)
            let weight = if comps == 4 { glow } else { glow * cov };
            for (slot, tint) in pixel.iter_mut().zip(tint).take(comps.min(3)) {
                let target = if comps == 4 { tint * cov } else { tint };
                *slot += (target - *slot) * weight;
            }
        } else {
            // Under the shapes: it shows where they leave room
            let under = glow * (1.0 - cov);
            for (slot, tint) in pixel.iter_mut().zip(tint).take(comps.min(3)) {
                *slot += tint * under;
            }
            if comps == 4
                && let Some(alpha) = pixel.get_mut(3)
            {
                *alpha += under;
            }
        }
        for (slot, was) in pixel.iter_mut().zip(before) {
            *slot = was + (*slot - was) * opacity;
        }
    });
}

/// Lay Mask Outline `outline` over premultiplied `data` (shapes of content from its luminance), mixed in by `opacity`
fn mask_outline(data: &mut [f32], cover: &[f32], comps: usize, (width, height, radius): (usize, usize, usize), outline: &MaskOutline, opacity: f32) {
    let value = |pixel: &[f32]| match pixel {
        [r, g, b, alpha] if *alpha > MIN_ALPHA => lum([r / alpha, g / alpha, b / alpha]),
        [value] => *value,
        _ => 0.0,
    };
    let inside: Vec<bool> = data.par_chunks(comps).zip(cover.par_iter()).map(|(pixel, covered)| *covered > 0.0 && outline.inside(value(pixel))).collect();
    let mut weights = cover.to_vec();
    let mut sums: Vec<f32> = inside.par_iter().zip(cover.par_iter()).map(|(inside, covered)| if *inside { *covered } else { 0.0 }).collect();
    blur(&mut weights, width, height, radius);
    blur(&mut sums, width, height, radius);
    data.par_chunks_mut(comps).zip(inside.par_iter()).zip(sums.par_iter().zip(weights.par_iter())).for_each(|((pixel, inside), (sum, weight))| {
        // The shapes' share around a texel is a half at their edge and none (outside) or all (inside) past the band
        let share = if *weight > 1e-6 { (sum / weight).clamp(0.0, 1.0) } else { 0.0 };
        let closeness = if *inside { (1.0 - share) / OUTLINE_EDGE_SHARE } else { share / OUTLINE_EDGE_SHARE };
        let band = outline.band(*inside, closeness);

        // Content is premultiplied by its coverage, which stays
        let cov = if comps == 4 { pixel.get(3).copied().unwrap_or(0.0) } else { 1.0 };
        for slot in pixel.iter_mut().take(comps.min(3)) {
            let before = if comps == 4 { if cov > MIN_ALPHA { *slot / cov } else { 0.0 } } else { *slot };
            let result = outline.apply(before, band);
            *slot = (before + (result - before) * opacity) * cov;
        }
    });
}

/// The shapes' blurred share at their edge
const OUTLINE_EDGE_SHARE: f32 = 0.5;

/// Below this cover a texel is empty and has no straight colour
pub(crate) const MIN_ALPHA: f32 = 1e-5;

/// The blurred value a precise halo is solid from, and how soft a full spread leaves it
const PRECISE_EDGE: f32 = 0.5;
const HALO_SOFTEST: f32 = 0.01;

/// Run point filter `filter` on blurred texels `data` (premultiplied content or masks) and mix the result over the unblurred `orig` by `opacity`
fn map_blurred(data: &mut [f32], orig: &[f32], comps: usize, filter: &Filter, kind: ChannelKind, opacity: f32) {
    let straight = |value: f32, cover: f32| if cover > MIN_ALPHA { value / cover } else { 0.0 };
    data.par_chunks_mut(comps).zip(orig.par_chunks(comps)).for_each(|(pixel, original)| match (pixel, original) {
        ([value], [before]) => {
            let mapped = filter.point(kind, [*value; 3]).map_or(*value, |gray| gray[0]);
            *value = before + (mapped - before) * opacity;
        }
        ([r, g, b, cover], [r0, g0, b0, cover0]) => {
            let before = [straight(*r0, *cover0), straight(*g0, *cover0), straight(*b0, *cover0)];
            let blurred = [straight(*r, *cover), straight(*g, *cover), straight(*b, *cover)];
            let mixed = mix3(before, filter.point(kind, blurred).unwrap_or(before), opacity);
            [*r, *g, *b, *cover] = [mixed[0] * cover0, mixed[1] * cover0, mixed[2] * cover0, *cover0];
        }
        _ => {}
    });
}

/// Clamp filtered masks to 0..1, and turn filtered content back into straight values.
fn unpremultiply(data: &mut [f32], comps: usize) {
    data.par_chunks_mut(comps).for_each(|px| match px {
        [v] => *v = v.clamp(0.0, 1.0),
        [r, g, b, cov] => {
            *cov = cov.clamp(0.0, 1.0);
            let inv = if *cov > 1e-5 { 1.0 / *cov } else { 0.0 };
            *r *= inv;
            *g *= inv;
            *b *= inv;
        }
        _ => {}
    });
}

/// Precompute the neighbourhood-filtered masks and content channels of every layer that needs
/// one, over `tiles` plus the filter's reach. Walks bottom → top so each field sees the fields
/// (and anchors) below it. Cheap when no layer has a blur or sharpen.
pub fn neighbourhood_fields(inp: &Inputs, channels: &[Channel], tiles: &[(u32, u32)]) -> Fields {
    let stack = inp.project.texture_set(inp.ts).map(|s| s.stack.as_slice()).unwrap_or_default();
    fields_of(inp, channels, stack, tiles)
}

/// [`neighbourhood_fields`] of the layers drawn by `list` only.
fn fields_of(inp: &Inputs, channels: &[Channel], list: &[LayerId], tiles: &[(u32, u32)]) -> Fields {
    let mut out = Fields::new();
    if tiles.is_empty() || inp.gbuf.width == 0 {
        return out;
    }

    let working_width = inp.gbuf.width;
    let mut order = Vec::new();
    draw_order(inp.project, list, 0, &mut order);
    prepare_curvature_sizes(inp, &order);

    for id in order {
        let Some(layer) = inp.project.content_layer(id) else { continue };

        if let Some(mask) = layer_mask(inp.project, id).filter(|mask| mask.enabled && neighbourhood(&mask.effects, working_width).is_some())
            && let Some(filtered) = field(inp, channels, &out, id, Job::Mask(mask), tiles)
        {
            out.insert(FieldKey::Mask(id), filtered);
        }

        if layer.is_group() || neighbourhood(&layer.content_effects, working_width).is_none() {
            continue;
        }
        for channel in channels.iter().filter(|channel| layer.channels.contains_key(channel)) {
            if let Some(filtered) = field(inp, channels, &out, id, Job::Content(*channel), tiles) {
                out.insert(FieldKey::Content(id, *channel), filtered);
            }
        }
    }
    out
}

/// Build the curvature detail sizes every visible generator of `layers` reads, once, before tiles composite in parallel.
fn prepare_curvature_sizes(inp: &Inputs, layers: &[LayerId]) {
    let visible = layers.iter().filter(|id| inp.project.layers.get(id).is_some_and(|layer| layer.visible));
    let masks = visible.filter_map(|id| layer_mask(inp.project, *id)).filter(|mask| mask.enabled);
    prepare_mask_curvature_sizes(inp, masks);
}

/// Build the curvature detail sizes the visible generators of `masks` read, once, before tiles composite in parallel.
fn prepare_mask_curvature_sizes<'a>(inp: &Inputs, masks: impl Iterator<Item = &'a Mask>) {
    let mut needed = [false; CurvatureDetail::BLURRED_SIZES];
    for effect in masks.flat_map(|mask| &mask.effects).filter(|effect| effect.visible) {
        if let EffectKind::Generator(Generator { detail: Some(detail), .. }) = &effect.kind {
            for (slot, in_use) in needed.iter_mut().zip(detail.blurred_sizes_in_use()) {
                *slot |= in_use;
            }
        }
    }
    CurvatureSizes::of(inp, &needed);
}

struct Node<'a> {
    id: LayerId,
    content: Content<'a>,
    chans: Vec<ChanPlan<'a>>,
    mask: Option<MaskPlan<'a>>,
    /// Visible anchor points of the layer: recorded after it is applied, or (`Some(k)`, an
    /// anchor with visible effects above it) with its content cut after `k` visible effects.
    anchors: Vec<(LayerId, Option<usize>)>,
    /// The node or one of its children records an anchor.
    has_anchor: bool,
    height_mask: HeightMask,
    /// Geometry mask: whether the layer shows on each mesh submesh (None: everywhere).
    parts: Option<Vec<bool>>,
}

/// Plans of the visible effects of a stack: mask effects (`channel` None) or the content
/// effects of one channel (paint effects then read that channel's pixels).
#[allow(clippy::too_many_arguments)]
fn effect_plans<'a>(
    inp: &Inputs<'a>,
    effects: &'a [Effect],
    tile_x: u32,
    tile_y: u32,
    channel: Option<Channel>,
    around: ContentsOffset,
) -> Vec<EffectPlan<'a>> {
    effects
        .iter()
        .filter(|effect| effect.visible)
        .map(|effect| {
            let (image, target) = match channel {
                Some(channel) => (inp.pixels.layer(effect.id, channel), PaintTarget::Layer(effect.id)),
                None => (inp.pixels.masks.get(&effect.id), PaintTarget::Mask(effect.id)),
            };
            let curvature_sizes = match &effect.kind {
                EffectKind::Generator(Generator { detail: Some(detail), .. }) => CurvatureSizes::of(inp, &detail.blurred_sizes_in_use()),
                _ => None,
            };
            let depth = match &effect.kind {
                EffectKind::Fill { projection, .. } | EffectKind::Material(Fill { projection, .. }) => ProjectorDepth::of(inp.mesh, inp.bounds, projection),
                _ => None,
            };
            let environment = match &effect.kind {
                EffectKind::Filter(Filter::EnvironmentLighting { .. }) => Some(EnvironmentSh::of(&inp.project.display, inp.images)),
                _ => None,
            };
            let blend = effect.applied_blend(channel);
            EffectPlan {
                id: effect.id,
                kind: offset_effect(&effect.kind, around),
                blend: blend.mode,
                opacity: blend.opacity,
                data: if effect.kind.painted() { tile_of(inp, image, tile_x, tile_y, channel) } else { None },
                preview: previews(inp, target),
                curvature_sizes,
                depth,
                environment,
                input: match &effect.kind {
                    EffectKind::Filter(filter) if !filter.is_neighbourhood() => ImageInput::of(filter).first().map(|input| effect.input(input)),
                    _ => None,
                },
            }
        })
        .collect()
}

/// `kind` with the folders' Scale and Rotation `around` laid on its projection (borrowed where that changes nothing)
fn offset_effect(kind: &EffectKind, around: ContentsOffset) -> Cow<'_, EffectKind> {
    match kind {
        EffectKind::Fill { source, projection, sampling } if !around.is_none() => {
            Cow::Owned(EffectKind::Fill { source: source.clone(), projection: around.apply(projection), sampling: *sampling })
        }
        EffectKind::Material(fill) if !around.is_none() => Cow::Owned(EffectKind::Material(offset_fill(fill, around).into_owned())),
        _ => Cow::Borrowed(kind),
    }
}

/// `fill` with the folders' Scale and Rotation `around` laid on its projection (borrowed where that changes nothing)
fn offset_fill(fill: &Fill, around: ContentsOffset) -> Cow<'_, Fill> {
    if around.is_none() {
        return Cow::Borrowed(fill);
    }

    Cow::Owned(Fill { projection: around.apply(&fill.projection), ..fill.clone() })
}

/// Whether the live stroke paints `target` in this composite (a stroke on a layer instanced in a
/// set of another size does not show there).
fn previews(inp: &Inputs, target: PaintTarget) -> bool {
    inp.preview.is_some_and(|p| p.target == target && (p.alpha.width(), p.alpha.height()) == (inp.gbuf.width, inp.gbuf.height))
}

/// Tile (tx, ty) of painted pixels `img` (of `channel`, or a mask when None) at the size of the
/// set being composited: as stored, or resampled (bilinear, premultiplied) when the pixels
/// belong to a layer instanced from a set of another resolution.
fn tile_of<'a>(inp: &Inputs, img: Option<&'a TileImage>, tx: u32, ty: u32, channel: Option<Channel>) -> Option<Cow<'a, [u16]>> {
    let img = img?;
    let (w, h) = (inp.gbuf.width, inp.gbuf.height);
    if (img.width(), img.height()) == (w, h) {
        return img.tile(tx, ty).map(Cow::Borrowed);
    }
    // Empty texels of signed channels hold a signed zero.
    let empty: Vec<u16> = match channel {
        Some(c) if c.kind() == ChannelKind::Signed => [vec![to_u16_signed(0.0); c.components()], vec![0]].concat(),
        _ => Vec::new(),
    };
    img.resampled_tile(w, h, tx, ty, &empty).map(Cow::Owned)
}

/// The plan of layer `id`'s `mask` for tile (tx, ty), its fills under the Scale and Rotation `around` of the folders holding the layer
#[allow(clippy::too_many_arguments)]
fn mask_plan<'a>(inp: &Inputs<'a>, id: LayerId, mask: &'a Mask, tx: u32, ty: u32, fields: Option<&'a Fields>, around: ContentsOffset) -> Option<MaskPlan<'a>> {
    if !mask.enabled {
        return None;
    }
    let background = if mask.background == MaskBackground::White { 1.0 } else { 0.0 };
    let effects = effect_plans(inp, &mask.effects, tx, ty, None, around);
    let start = fields.and_then(|f| f.get(&FieldKey::Mask(id))).and_then(|field| neighbourhood(&mask.effects, inp.gbuf.width).map(|(k, _, _)| (k, field)));
    let mut compared: Vec<Channel> = Vec::new();
    for effect in mask.effects.iter().filter(|e| e.visible) {
        if let EffectKind::Compare { channel, .. } = effect.kind
            && effect.kind.reads_stack()
            && !compared.contains(&channel)
        {
            compared.push(channel);
        }
    }

    // A folder's own content is its children: its compare sides read the stack below it
    let layer = if compared.is_empty() {
        None
    } else {
        inp.project.content_layer(id).and_then(|l| plan_content(inp, id, l, &compared, tx, ty, 0, fields, false, around))
    };
    Some(MaskPlan { background, effects, layer, start })
}

/// The plans of sibling layers `list` (under the Scale and Rotation of the folders holding them)
fn plan<'a>(inp: &Inputs<'a>, list: &[LayerId], channels: &[Channel], tx: u32, ty: u32, depth: usize, fields: Option<&'a Fields>) -> Vec<Node<'a>> {
    let around = list.first().map_or(ContentsOffset::NONE, |id| inp.project.contents_around(*id));
    plan_in(inp, list, channels, tx, ty, depth, fields, around)
}

/// [`plan`] of sibling layers held by folders whose Scale and Rotation make `around`
#[allow(clippy::too_many_arguments)]
fn plan_in<'a>(
    inp: &Inputs<'a>,
    list: &[LayerId],
    channels: &[Channel],
    tx: u32,
    ty: u32,
    depth: usize,
    fields: Option<&'a Fields>,
    around: ContentsOffset,
) -> Vec<Node<'a>> {
    if depth > texturecraft_core::project::MAX_DEPTH {
        return Vec::new();
    }
    list.iter().filter_map(|id| plan_node(inp, *id, channels, tx, ty, depth, fields, true, around)).collect()
}

/// The plan of layer `id` at `depth` (None where it draws nothing), its fills under the Scale and Rotation `around` of the folders holding it; a folder's children are planned when `kids`
#[allow(clippy::too_many_arguments)]
fn plan_node<'a>(
    inp: &Inputs<'a>,
    id: LayerId,
    channels: &[Channel],
    tx: u32,
    ty: u32,
    depth: usize,
    fields: Option<&'a Fields>,
    kids: bool,
    around: ContentsOffset,
) -> Option<Node<'a>> {
    let own: &'a Layer = inp.project.layers.get(&id)?;
    // Instances draw their source's content (a missing source draws nothing).
    let layer: &'a Layer = inp.project.content_layer(id)?;
    if !own.visible {
        return None;
    }
    let (content, chans) = plan_content(inp, id, layer, channels, tx, ty, depth, fields, kids, around)?;
    let mask = layer_mask(inp.project, id).and_then(|m| mask_plan(inp, id, m, tx, ty, fields, around));

    // Anchor points in the mask record even where the layer draws nothing
    let mask_anchor = mask.as_ref().is_some_and(|m| m.effects.iter().any(|e| matches!(*e.kind, EffectKind::Anchor)));

    // A paint layer with nothing painted (and not being painted) or added by its content effects contributes nothing
    if let Content::Paint { preview: false } = content
        && chans.iter().all(|c| c.data.is_none() && c.effects.is_empty())
        && !mask_anchor
    {
        return None;
    }

    if chans.is_empty() && !mask_anchor {
        return None;
    }

    let fx = !layer.is_group() && layer.content_effects.iter().any(|e| e.visible);
    let anchors = if fx { anchor_cuts(&layer.content_effects) } else { Vec::new() };
    let has_anchor = !anchors.is_empty() || mask_anchor || matches!(&content, Content::Group(nodes) if nodes.iter().any(|n| n.has_anchor));

    // The geometry mask is the layer's own (an instance in another set has other parts)
    let parts = own.geometry_mask.as_ref().map(|names| inp.mesh.submeshes.iter().map(|s| names.contains(&s.name)).collect());
    Some(Node { id, content, chans, mask, anchors, has_anchor, height_mask: layer.height_mask, parts })
}

/// The content of `layer` (layer `id`'s own, or its instance source's; None for an unfollowed instance) and its plan per drawn channel of `channels`, its fills under the Scale and Rotation `around` of the folders holding `id`; a folder's children are planned when `kids`
#[allow(clippy::too_many_arguments)]
fn plan_content<'a>(
    inp: &Inputs<'a>,
    id: LayerId,
    layer: &'a Layer,
    channels: &[Channel],
    tx: u32,
    ty: u32,
    depth: usize,
    fields: Option<&'a Fields>,
    kids: bool,
    around: ContentsOffset,
) -> Option<(Content<'a>, Vec<ChanPlan<'a>>)> {
    let paint = matches!(layer.kind, LayerKind::Paint);
    let fx = !layer.is_group() && layer.content_effects.iter().any(|e| e.visible);
    let fx_start = if fx { neighbourhood(&layer.content_effects, inp.gbuf.width).map(|n| n.0) } else { None };
    let chans = channels
        .iter()
        .filter_map(|c| {
            let b = layer.blend(*c)?;
            if b.mode == BlendMode::Disable || b.opacity <= 0.0 {
                return None;
            }
            let data = if paint { tile_of(inp, inp.pixels.layer(layer.id, *c), tx, ty, Some(*c)) } else { None };
            let effects = if fx { effect_plans(inp, &layer.content_effects, tx, ty, Some(*c), around) } else { Vec::new() };
            let start = fx_start.zip(fields.and_then(|f| f.get(&FieldKey::Content(id, *c))));
            Some(ChanPlan { ch: c.index(), kind: c.kind(), comps: c.components(), blend: b.mode, opacity: b.opacity.clamp(0.0, 1.0), data, effects, start })
        })
        .collect::<Vec<_>>();
    let content = match &layer.kind {
        LayerKind::Paint => Content::Paint { preview: previews(inp, PaintTarget::Layer(layer.id)) },
        LayerKind::Fill(f) => Content::Fill(Box::new(offset_fill(f, around)), ProjectorDepth::of(inp.mesh, inp.bounds, &f.projection)),
        LayerKind::Group { children, contents, .. } => {
            Content::Group(if kids { plan_in(inp, children, channels, tx, ty, depth + 1, fields, around.then(*contents)) } else { Vec::new() })
        }
        // `content_layer` already followed instance links.
        LayerKind::Instance { .. } => return None,
    };
    Some((content, chans))
}

/// The visible anchor points of a content stack with where they cut it: after their index among
/// the visible effects, or None for anchors with only anchors above them (they record the
/// layer's full result).
fn anchor_cuts(effects: &[Effect]) -> Vec<(LayerId, Option<usize>)> {
    let visible: Vec<&Effect> = effects.iter().filter(|e| e.visible).collect();
    let last = visible.iter().rposition(|e| !matches!(e.kind, EffectKind::Anchor));
    visible.iter().enumerate().filter(|(_, e)| matches!(e.kind, EffectKind::Anchor)).map(|(i, e)| (e.id, last.filter(|l| *l > i).map(|_| i))).collect()
}

/// One level of the path to the hot layer (see [`Level`]) planned for a tile.
struct LevelPlan<'a> {
    below: Vec<Node<'a>>,
    /// A folder planned without its children, or (last level) the hot layer.
    at: Option<Node<'a>>,
    above: Vec<Node<'a>>,
}

/// Plan the path to the hot layer for tile (tx, ty); the layers before the path are left out
/// on tiles the cache holds (`warm`).
fn plan_levels<'a>(inp: &Inputs<'a>, levels: &[Level], channels: &[Channel], tx: u32, ty: u32, fields: &'a Fields, warm: bool) -> Vec<LevelPlan<'a>> {
    let last = levels.len().saturating_sub(1);
    levels
        .iter()
        .enumerate()
        .map(|(d, l)| LevelPlan {
            below: if warm { Vec::new() } else { plan(inp, &l.below, channels, tx, ty, d, Some(fields)) },
            at: plan_node(inp, l.at, channels, tx, ty, d, Some(fields), d == last, inp.project.contents_around(l.at)),
            above: plan(inp, &l.above, channels, tx, ty, d, Some(fields)),
        })
        .collect()
}

#[inline]
fn blend_chan(c: &ChanPlan, mode: BlendMode, a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    if c.comps == 3 { blend3(mode, a, b) } else { [blend1(mode, a[0], b[0]); 3] }
}

/// Lay value `p` with weight `a` over a straight value and coverage (premultiplied "over").
fn over(c: &ChanPlan, mode: BlendMode, v: [f32; 3], cov: f32, p: [f32; 3], a: f32) -> ([f32; 3], f32) {
    if a <= 0.0 {
        return (v, cov);
    }
    let b = if cov > 0.0 { blend_chan(c, mode, v, p) } else { p };
    let nc = a + cov * (1.0 - a);
    (mix3(v, b, a / nc.max(1e-5)), nc)
}

/// Per-texel context.
struct Texel<'a, 'b> {
    inp: &'b Inputs<'a>,
    x: u32,
    y: u32,
    /// Offset of this texel inside a tile (in texels).
    o: usize,
    res: u32,
    coords: Option<Coords>,
    stroke: f32,
    /// `eval` stops when it reaches this layer.
    stop: Option<LayerId>,
    /// Stack values recorded by the anchor points evaluated so far.
    anchors: Vec<(LayerId, Acc)>,
    /// The stack below the layer whose mask compares its channels (recorded by `eval` there, or where it stops)
    below: Option<Acc>,
    /// The material Baked Lighting Stylized reads its highlights from: the stack with the layer it lights (its other channels) laid on
    lit: Option<Acc>,
    /// Where `eval` takes layer masks and content from.
    tape: Tape<'b>,
    /// The copy of a symmetric fill being sampled: the surface point and normal it sees, and its image's flip
    copy: Option<FillCopy>,
}

/// What a layer contributes (its mask, its content value and coverage per channel): evaluated
/// (`Off`), evaluated and recorded, or replayed from a recording.
enum Tape<'t> {
    Off,
    Record(Vec<f32>),
    Replay(&'t [f32], usize),
}

impl<'t> Tape<'t> {
    fn next(&mut self) -> f32 {
        self.take(1).first().copied().unwrap_or(0.0)
    }

    /// The next `n` replayed values (empty when not replaying).
    fn take(&mut self, n: usize) -> &'t [f32] {
        match self {
            Tape::Replay(t, i) => {
                let t: &'t [f32] = t;
                let end = i.saturating_add(n);
                let s = t.get(*i..end).unwrap_or_default();
                *i = end;
                s
            }
            _ => &[],
        }
    }
}

impl<'a, 'b> Texel<'a, 'b> {
    fn new(inp: &'b Inputs<'a>, x: u32, y: u32, o: usize, stroke: f32) -> Self {
        Texel { inp, x, y, o, res: inp.gbuf.width, coords: None, stroke, stop: None, anchors: Vec::new(), below: None, lit: None, tape: Tape::Off, copy: None }
    }

    /// Run `f` with `tape` as this texel's tape.
    fn taped<R>(&mut self, tape: &mut Tape<'b>, f: impl FnOnce(&mut Self) -> R) -> R {
        std::mem::swap(&mut self.tape, tape);
        let r = f(self);
        std::mem::swap(&mut self.tape, tape);
        r
    }

    /// [`Texel::node_mask`] through the tape.
    fn tape_mask(&mut self, n: &Node) -> f32 {
        if let Tape::Replay(..) = self.tape {
            return self.tape.next();
        }
        let m = self.node_mask(n);
        if let Tape::Record(t) = &mut self.tape {
            t.push(m);
        }
        m
    }

    /// [`Texel::content`] through the tape: the coverage (0 where empty), then the value.
    fn tape_content(&mut self, content: &Content, c: &ChanPlan) -> Option<([f32; 3], f32)> {
        if let Tape::Replay(..) = self.tape {
            let cov = self.tape.next();
            // Recorded as 0 where empty.
            if cov <= 0.0 {
                return None;
            }
            let v = if c.comps == 3 { [self.tape.next(), self.tape.next(), self.tape.next()] } else { [self.tape.next(); 3] };
            return Some((v, cov));
        }
        let r = self.content(content, c);
        if let Tape::Record(t) = &mut self.tape {
            match r {
                Some((v, cov)) => {
                    t.push(cov);
                    let n = if c.comps == 3 { 3 } else { 1 };
                    t.extend_from_slice(&v[..n]);
                }
                None => t.push(0.0),
            }
        }
        r
    }

    fn coords(&mut self) -> Coords {
        if let Some(c) = self.coords {
            return c;
        }

        let g = self.inp.gbuf;
        let c = match self.surface((self.y * g.width + self.x) as usize) {
            Some(s) => self.coords_of(s),
            None => self.tile_uv(Coords {
                uv: Vec2::new((self.x as f32 + 0.5) / g.width as f32, (self.y as f32 + 0.5) / g.height as f32),
                pos: Vec3::ZERO,
                nrm: Vec3::Y,
            }),
        };
        self.coords = Some(c);
        c
    }

    /// The coordinates projections see at surface `s`: its UV, and its position in mesh radii from the mesh's centre
    fn coords_of(&self, s: Surface) -> Coords {
        let (center, radius) = mesh_frame(self.inp.bounds);
        self.tile_uv(Coords { uv: s.uv, pos: (s.pos - center) / radius, nrm: s.nrm })
    }

    /// `c` with its UV moved out of the atlas on a UV-tiled set, so projections see the mesh's own UVs (patterns repeat per tile)
    fn tile_uv(&self, mut c: Coords) -> Coords {
        if let Ok(set) = self.inp.project.texture_set(self.inp.ts)
            && let Some(grid) = set.tile_grid()
        {
            c.uv = Vec2::from(set.from_atlas(&grid, c.uv.to_array()));
        }
        c
    }

    /// The coordinates of texel (`x`, `y`) through `proj`, uncached (None when uncovered)
    fn projected_at(&self, proj: &Projection, x: u32, y: u32) -> Option<Coords> {
        let g = self.inp.gbuf;
        if x >= g.width || y >= g.height {
            return None;
        }

        let s = self.surface((y * g.width + x) as usize)?;
        let c = self.coords_of(s);
        if proj.frame().is_none() {
            return Some(c);
        }

        framed_cover(proj, s.pos, s.nrm).map(|(uv, _)| Coords { uv, ..c })
    }

    /// Image texels one surface texel spans at each of `samples`, from the next texel across and down (1 at a seam)
    fn footprints(&self, proj: &Projection, samples: &[(Vec2, f32); 3], image: &ImageRes) -> [f32; 3] {
        let size = Vec2::new(image.width as f32, image.height as f32);
        let mut out = [1.0f32; 3];
        for (dx, dy) in [(1, 0), (0, 1)] {
            let Some(next) = self.projected_at(proj, self.x + dx, self.y + dy) else { continue };
            for ((footprint, (uv, _)), (next_uv, _)) in out.iter_mut().zip(samples).zip(self.image_uvs(proj, &next)) {
                let span = ((next_uv - *uv) * size).length();

                // A jump across more of the image than this is a UV seam, not the texel's size
                if span.is_finite() && span < size.max_element() * SEAM_JUMP_SHARE {
                    *footprint = footprint.max(span);
                }
            }
        }
        out
    }

    /// World position of this texel as the fill copy being sampled sees it (None when uncovered)
    fn world(&self) -> Option<Vec3> {
        self.surface((self.y * self.inp.gbuf.width + self.x) as usize).map(|s| s.pos)
    }

    /// The surface at G-buffer index `i`, as the fill copy being sampled sees it
    fn surface(&self, i: usize) -> Option<Surface> {
        let mut s = self.inp.gbuf.surface(self.inp.mesh, i)?;
        if let Some(copy) = self.copy {
            s.pos = copy.point(s.pos);
            s.nrm = copy.normal(s.nrm);
            s.geo_nrm = copy.normal(s.geo_nrm);
        }
        Some(s)
    }

    /// Where `proj` reads images at `coords`, turned over as the fill copy being sampled says
    fn image_uvs(&self, proj: &Projection, coords: &Coords) -> [(Vec2, f32); 3] {
        let samples = image_samples(proj, coords);
        match self.copy {
            Some(copy) => samples.map(|(uv, weight)| (copy.uv(uv), weight)),
            None => samples,
        }
    }

    /// A fill's value and coverage through `proj`, from `one` (which samples it once): with `sampling`'s symmetry on, every copy of the projection is sampled and the copies are laid over each other, the texel's own copy on top
    fn symmetric(&mut self, proj: &Projection, sampling: Sampling, mut one: impl FnMut(&mut Self) -> ([f32; 3], f32)) -> ([f32; 3], f32) {
        let symmetry = sampling.symmetry;

        // Most fills have no symmetry: they never pay for the surface lookup
        if !(symmetry.on && proj.takes_symmetry()) {
            return one(self);
        }

        let Some(world) = self.inp.gbuf.surface(self.inp.mesh, (self.y * self.inp.gbuf.width + self.x) as usize).map(|surface| surface.pos) else {
            return one(self);
        };

        let (center, radius) = mesh_frame(self.inp.bounds);
        let cached = self.coords.take();
        let mut value = [0.0f32; 3];
        let mut cover = 0.0f32;
        for copy in FillCopies::new(&symmetry, world, center, radius) {
            self.copy = Some(copy);
            self.coords = None;
            let (copy_value, copy_cover) = one(self);
            let weight = copy_cover.clamp(0.0, 1.0) * (1.0 - cover);
            for (total, part) in value.iter_mut().zip(copy_value) {
                *total += part * weight;
            }
            cover += weight;
            if cover >= COVERED {
                break;
            }
        }
        self.copy = None;
        self.coords = cached;
        if cover <= 0.0 {
            return ([0.0; 3], 0.0);
        }

        (value.map(|v| v / cover), cover)
    }

    /// What Baked Lighting Stylized and Environment read their highlights from (and PBR Validate its metal) in node `n`: the stack below (`acc`) with the layer's other channels on under mask `m`; None where nothing reads it
    fn lit_material(&mut self, n: &Node, acc: &Acc, m: f32) -> Option<Acc> {
        if matches!(self.tape, Tape::Replay(..)) {
            return None;
        }

        let base = Channel::BaseColor.index();
        let lights = |c: &ChanPlan| {
            c.ch == base
                && c.effects.iter().any(|e| match &*e.kind {
                    EffectKind::Filter(f @ Filter::BakedLighting { .. }) => f.stylized_lighting().is_some_and(|(s, _)| s.output.specular()),
                    EffectKind::Filter(Filter::EnvironmentLighting { .. } | Filter::PbrValidate(_)) => true,
                    _ => false,
                })
        };
        if !n.chans.iter().any(lights) {
            return None;
        }

        let mut lit = *acc;
        for c in n.chans.iter().filter(|c| c.ch != base) {
            if let Some((v, cov)) = self.content(&n.content, c) {
                apply(&mut lit, c, v, cov, m);
            }
        }
        Some(lit)
    }

    /// The normal Baked Lighting Stylized lights: the mesh's, or (Mesh + Height + Normal) its [`Texel::detail_normal`]
    fn lit_normal(&mut self, stylized: &StylizedLighting) -> [f32; 3] {
        if stylized.mesh_only {
            return self.coords().nrm.to_array();
        }

        self.detail_normal(stylized.height_to_normal)
    }

    /// The baked world normal (the mesh's without one) tilted by the baked height's slope times `relief`
    fn detail_normal(&mut self, relief: f32) -> [f32; 3] {
        let mesh = self.coords().nrm;
        let detail = self.baked_color(BakedMap::WorldNormal).map(|packed| Vec3::from(packed) * 2.0 - Vec3::ONE).filter(|n| n.length_squared() > 1e-6);
        let nrm = detail.map_or(mesh, Vec3::normalize);
        let i = (self.y * self.inp.gbuf.width + self.x) as usize;
        let (Some(surface), Some(slope)) = (self.surface(i), self.height_slope()) else { return nrm.to_array() };
        let tangent = surface.tangent.truncate();
        let bitangent = nrm.cross(tangent) * surface.tangent.w;
        let tilted = nrm - (tangent * slope[0] + bitangent * slope[1]) * HEIGHT_RELIEF * relief;
        tilted.try_normalize().unwrap_or(nrm).to_array()
    }

    /// The baked height's change per UV unit along U and V at this texel (central differences); None without a height bake
    fn height_slope(&self) -> Option<[f32; 2]> {
        let (width, height) = (self.res, self.inp.gbuf.height);
        let at = |x: u32, y: u32| self.inp.baked.sample_working(BakedMap::Height, x, y, (width, height), &self.inp.block_scales).map(|h| h[0]);
        let (left, right) = (self.x.saturating_sub(1), (self.x + 1).min(width.saturating_sub(1)));
        let (up, down) = (self.y.saturating_sub(1), (self.y + 1).min(height.saturating_sub(1)));
        let du = (at(right, self.y)? - at(left, self.y)?) / (right - left).max(1) as f32 * width as f32;
        let dv = (at(self.x, down)? - at(self.x, up)?) / (down - up).max(1) as f32 * height as f32;
        Some([du, dv]).filter(|slope| slope.iter().all(|s| s.is_finite()))
    }

    fn baked(&self, m: BakedMap) -> Option<f32> {
        self.baked_color(m).map(|value| value[0])
    }

    fn baked_color(&self, map: BakedMap) -> Option<[f32; 3]> {
        self.inp.baked.sample_working(map, self.x, self.y, (self.res, self.inp.gbuf.height), &self.inp.block_scales)
    }

    /// This texel's ID color: from image resource `image` (nearest, as stored), or from the ID mesh map without one
    fn id_color(&self, image: Option<&str>) -> Option<[f32; 3]> {
        let Some(name) = image else { return self.baked_color(BakedMap::Id) };
        let image = self.inp.images.get(name)?;
        let [red, green, blue, _] =
            image.sample_nearest((self.x as f32 + 0.5) / self.res.max(1) as f32, (self.y as f32 + 0.5) / self.inp.gbuf.height.max(1) as f32);
        Some([red, green, blue])
    }

    /// Where `proj` samples this texel: its coordinates, or for a projection from a frame on the surface (placed planar, warp) its place in the frame, and how much backface culling and the shape crop let through
    fn projected(&mut self, proj: &Projection) -> (Coords, f32) {
        let c = self.coords();
        if proj.frame().is_none() {
            return (c, 1.0);
        }

        match self.world().and_then(|p| framed_cover(proj, p, c.nrm)) {
            Some((uv, cover)) => (Coords { uv, ..c }, cover),
            None => (c, 0.0),
        }
    }

    /// How much a fill projected by `proj` covers this texel: [`Texel::shape_cover`], then depth culling through the projector's `depth` map
    fn cover<'s>(&mut self, sources: impl Iterator<Item = &'s Source>, proj: &Projection, sampling: Sampling, depth: Option<&ProjectorDepth>) -> f32 {
        let cover = self.shape_cover(sources, proj, sampling);
        match depth.zip(proj.depth_culling()) {
            Some((depth, culling)) if cover > 0.0 => cover * self.world().map_or(1.0, |p| depth.cover(p, culling.hardness)),
            _ => cover,
        }
    }

    /// How much a fill projected by `proj` covers this texel: wrap, shape crop and backface culling, and a cropped framed projection's image alpha
    fn shape_cover<'s>(&mut self, mut sources: impl Iterator<Item = &'s Source>, proj: &Projection, sampling: Sampling) -> f32 {
        let (Projection::Planar { projector, placed: Some(_), .. } | Projection::Warp { projector, .. }) = proj else {
            let coords = self.coords();
            return self.tile_cover(proj, &coords, sampling) * projector_cover(proj, &coords);
        };

        let (c, cover) = self.projected(proj);
        if projector.crop == ShapeCrop::ExtendsOutside {
            return cover * self.tile_cover(proj, &c, sampling);
        }

        // Cropped to its shape, a framed projection takes the shape of its image too
        let image = sources.find_map(|s| match s {
            Source::Image { name, .. } => self.inp.images.get(name),
            _ => None,
        });
        let [(uv, _), ..] = self.image_uvs(proj, &c);
        let alpha = image.map_or(1.0, |img| img.sample_filtered(uv.x, uv.y, sampling, 1.0)[3].clamp(0.0, 1.0));
        cover * alpha * self.tile_cover(proj, &c, sampling)
    }

    /// 1 where a fill placed by `proj` at `coords` lands inside its 0..1 tile on every axis `sampling.wrap` does not repeat, else 0
    fn tile_cover(&self, proj: &Projection, coords: &Coords, sampling: Sampling) -> f32 {
        let (across, down) = sampling.wrap.repeats();
        if (across && down) || matches!(proj, Projection::Triplanar { .. }) {
            return 1.0;
        }

        let [(uv, _), ..] = self.image_uvs(proj, coords);
        let inside = |t: f32, repeats: bool| repeats || (0.0..1.0).contains(&t);
        if inside(uv.x, across) && inside(uv.y, down) { 1.0 } else { 0.0 }
    }

    /// The value `source` gives this texel through `projection`, read as a channel of `kind`
    fn source(&mut self, source: &Source, projection: &Projection, sampling: Sampling, kind: ChannelKind) -> [f32; 3] {
        match source {
            Source::Uniform(value) => *value,
            Source::Procedural { pattern, low, high } => {
                let (coords, _) = self.projected(projection);
                let amount = procedural_sized(pattern, projection, &coords, self.inp.tile_res());
                mix3(*low, *high, amount)
            }
            Source::Image { name, normal_format } => {
                let Some(image) = self.inp.images.get(name) else { return [0.5; 3] };

                let (coords, _) = self.projected(projection);
                let samples = self.image_uvs(projection, &coords);
                let footprints = match sampling.filtering {
                    Filtering::BilinearHq => self.footprints(projection, &samples, image),
                    Filtering::BilinearSharp | Filtering::Nearest => [1.0; 3],
                };
                let mut sum = [0.0f32; 4];
                for ((uv, weight), footprint) in samples.into_iter().zip(footprints).filter(|((_, weight), _)| *weight > 0.0) {
                    for (total, value) in sum.iter_mut().zip(image.sample_filtered(uv.x, uv.y, sampling, footprint)) {
                        *total += value * weight;
                    }
                }

                let rgb = [sum[0], sum[1], sum[2]];
                match kind {
                    ChannelKind::Color if image.srgb => rgb.map(texturecraft_texel::srgb_to_linear),
                    // Normal channels are OpenGL (+Y up the image): a DirectX image's green flips
                    ChannelKind::NormalMap if *normal_format == NormalFormat::DirectX => [rgb[0], 1.0 - rgb[1], rgb[2]],
                    ChannelKind::Color | ChannelKind::NormalMap => rgb,
                    // A one-value channel or mask reads the image's gray, so a color image works as a mask
                    ChannelKind::Scalar => [lum(rgb); 3],
                    ChannelKind::Signed => [lum(rgb) * 2.0 - 1.0; 3],
                }
            }
            Source::Anchor { anchor, channel } => {
                // An anchor not evaluated yet (above this layer, hidden) reads the channel default
                let anchored = self.anchors.iter().rev().find(|(id, _)| id == anchor).and_then(|(_, values)| values.get(channel.index()).copied());
                let value = anchored.unwrap_or_else(|| channel.default_value());
                let gray = matches!(channel.kind(), ChannelKind::Color | ChannelKind::NormalMap) && matches!(kind, ChannelKind::Scalar | ChannelKind::Signed);
                if gray { [lum(value); 3] } else { value }
            }
        }
    }

    fn generator(&mut self, generator: &Generator, curvature_sizes: Option<&CurvatureSizes>) -> f32 {
        let coords = self.coords();
        let signal = match (generator.kind, &generator.detail) {
            (GeneratorKind::Curvature | GeneratorKind::EdgeWear, Some(detail)) => self.curvature_detail(detail, curvature_sizes),
            (GeneratorKind::EdgeWear, None) => (self.baked(BakedMap::Curvature).unwrap_or(0.5) - 0.5) * 2.0,
            (GeneratorKind::Curvature, None) => self.baked(BakedMap::Curvature).unwrap_or(0.5),
            (GeneratorKind::Dirt, _) => 1.0 - self.baked(BakedMap::AmbientOcclusion).unwrap_or(1.0),
            (GeneratorKind::AmbientOcclusion, _) => self.baked(BakedMap::AmbientOcclusion).unwrap_or(1.0),
            (GeneratorKind::ThinAreas, _) => 1.0 - self.baked(BakedMap::Thickness).unwrap_or(1.0),
            (GeneratorKind::Position, _) => coords.pos.dot(Vec3::from(generator.direction).try_normalize().unwrap_or(Vec3::Y)) * 0.5 + 0.5,
            (GeneratorKind::Direction, _) => coords.nrm.dot(Vec3::from(generator.direction).try_normalize().unwrap_or(Vec3::Y)) * 0.5 + 0.5,
        };
        let signal = if generator.noise_amount > 0.0 {
            let noise = world_noise(coords.pos, generator.noise_scale, generator.seed);
            if let (GeneratorKind::Curvature | GeneratorKind::EdgeWear, Some(detail)) = (generator.kind, &generator.detail) {
                // Break up curvature without inventing edges or cavities on a flat surface
                let neutral = if detail.mode == CurvatureMode::Raw { 0.5 } else { 0.0 };
                neutral + (signal - neutral) * (1.0 - noise * generator.noise_amount).max(0.0)
            } else {
                signal + (noise - 0.5) * generator.noise_amount
            }
        } else {
            signal
        };

        // A smooth step from black to white over 2 × softness around 1 − level, then the offset
        generator.shape(signal)
    }

    /// The curvature of `detail`'s mode at this texel: each detail size's edges and cavities, weighted and combined like a screen blend.
    fn curvature_detail(&self, detail: &CurvatureDetail, curvature_sizes: Option<&CurvatureSizes>) -> f32 {
        let combine = |edges: f32, cavities: f32| match detail.mode {
            CurvatureMode::Edges => edges,
            CurvatureMode::Cavities => cavities,
            CurvatureMode::Both => 1.0 - (1.0 - edges) * (1.0 - cavities),
            CurvatureMode::Raw => 0.5 + 0.5 * (edges - cavities),
        };

        // Without curvature to read the texel is flat: no edges or cavities
        let Some(curvature) = self.baked(BakedMap::Curvature) else { return combine(0.0, 0.0) };

        let working_size = (self.res, self.inp.gbuf.height);
        let texel_uv = [(self.x as f32 + 0.5) / working_size.0.max(1) as f32, (self.y as f32 + 0.5) / working_size.1.max(1) as f32];
        let Some(baked_uv) = self.inp.block_scales.bake_uv(texel_uv, working_size) else { return combine(0.0, 0.0) };

        let baked_size = (self.inp.baked.resolution, self.inp.baked.height(BakedMap::Curvature));
        let (baked_x, baked_y) = ((baked_uv[0] * baked_size.0 as f32) as u32, (baked_uv[1] * baked_size.1 as f32) as u32);
        let blurred = |index: usize| {
            curvature_sizes.and_then(|sizes| sizes.levels.get(index)).and_then(Option::as_deref).map_or(0.0, |level| level.sample(baked_x, baked_y))
        };
        let (mut edges, mut cavities) = (0.0f32, 0.0f32);
        for (index, (size, weight)) in CurvatureDetail::SIZES.iter().zip(detail.weights).enumerate() {
            let deviation = if index == 0 { curvature - 0.5 } else { blurred(index - 1) };

            let response = deviation * if *size == 0.0 { MAP_GAIN } else { GAIN_PER_DETAIL_TEXEL * size };
            let weight = weight.clamp(0.0, 1.0);
            edges = 1.0 - (1.0 - edges) * (1.0 - weight * response.clamp(0.0, 1.0));
            cavities = 1.0 - (1.0 - cavities) * (1.0 - weight * (-response).clamp(0.0, 1.0));
        }

        combine(edges, cavities)
    }

    fn mask(&mut self, plan: &MaskPlan) -> f32 {
        let mut value = plan.background;
        let mut skip = 0;
        if let Some((filtered_index, field)) = plan.start
            && let Some(filtered) = field.get(self.x, self.y).and_then(|texel| texel.first().copied())
        {
            value = filtered;
            skip = filtered_index + 1;
        }

        // Anchor points under the precomputed filter: the mask up to the last of them, only to record them
        if let Some(last) = plan.effects.iter().take(skip).rposition(|e| matches!(*e.kind, EffectKind::Anchor)) {
            self.mask_effects(plan, plan.background, 0, last + 1);
        }
        self.mask_effects(plan, value, skip, plan.effects.len())
    }

    /// Mask effects `from..to` of `plan` over `value`, recording the anchor points among them
    fn mask_effects(&mut self, plan: &MaskPlan, mut value: f32, from: usize, to: usize) -> f32 {
        for effect in plan.effects.iter().take(to).skip(from) {
            let opacity = effect.opacity.clamp(0.0, 1.0);
            match &*effect.kind {
                EffectKind::Paint | EffectKind::Path(_) => {
                    let (mut painted, mut cov) = match effect.data.as_deref() {
                        Some(data) => (from_u16(data.get(self.o * 2).copied().unwrap_or(0)), from_u16(data.get(self.o * 2 + 1).copied().unwrap_or(0))),
                        None => (0.0, 0.0),
                    };
                    if effect.preview
                        && let Some(preview) = self.inp.preview
                    {
                        let stroke = (self.stroke * preview.opacity).clamp(0.0, 1.0);
                        if preview.erase {
                            painted *= 1.0 - stroke;
                            cov *= 1.0 - stroke;
                        } else {
                            painted = preview.mask_gray.at(self.x, self.y, (self.inp.gbuf.width, self.inp.gbuf.height)) * stroke + painted * (1.0 - stroke);
                            cov = stroke + cov * (1.0 - stroke);
                        }
                    }
                    if cov > 0.0 {
                        let layer_value = painted / cov.max(1e-5);
                        value += (blend1(effect.blend, value, layer_value) - value) * cov * opacity;
                    }
                }
                EffectKind::Fill { source, projection, sampling } => {
                    let ([fill, ..], cover) = self.symmetric(projection, *sampling, |t| {
                        let fill = t.source(source, projection, *sampling, ChannelKind::Scalar);
                        (fill, t.cover(std::iter::once(source), projection, *sampling, effect.depth.as_deref()))
                    });
                    value += (blend1(effect.blend, value, fill) - value) * opacity * cover;
                }
                EffectKind::Generator(generator) => {
                    let generated = self.generator(generator, effect.curvature_sizes.as_deref());
                    value += (blend1(effect.blend, value, generated) - value) * opacity;
                }
                EffectKind::Levels(levels) => value += (levels.apply(value) - value) * opacity,
                EffectKind::Filter(filter @ Filter::GradientMap { debug: true, .. }) => {
                    let ramp = filter.ramp_at(self.coords().uv.x.rem_euclid(1.0)).map_or(value, lum);
                    value += (ramp - value) * opacity;
                }
                EffectKind::Filter(Filter::GrayscaleConversion(conversion)) => {
                    let read = match &effect.input {
                        Some(source) => self.source(source, &Projection::default(), Sampling::default(), ChannelKind::Color),
                        None => [value; 3],
                    };
                    value += (conversion.gray(read, 1.0) - value) * opacity;
                }
                EffectKind::Filter(filter) => {
                    // A mask is gray (HSL only changes its lightness); neighbourhood filters come precomputed through `MaskPlan::start`
                    let filtered = filter.point(ChannelKind::Scalar, [value; 3]).map_or(value, |gray| gray[0]);
                    value += (filtered - value) * opacity;
                }
                EffectKind::ColorSelection(selection) => {
                    let (selected, cover) = selection.value(self.id_color(selection.id_map.as_deref()));
                    value += (blend1(effect.blend, value, selected) - value) * opacity * cover;
                }
                EffectKind::Compare { op, value: constant, tolerance, left, right, channel, hardness } => {
                    let left = self.compare_side(*left, *channel, plan, value, *constant);
                    let right = self.compare_side(*right, *channel, plan, value, *constant);
                    value += (op.mask(left, right, *tolerance, *hardness) - value) * opacity;
                }

                // Every channel of a mask anchor point reads the mask's gray
                EffectKind::Anchor => self.anchors.push((effect.id, [[value.clamp(0.0, 1.0); 3]; NCH])),
                // Content only
                EffectKind::Material(_) => {}
            }
        }
        value.clamp(0.0, 1.0)
    }

    /// What one side of a compare effect in mask `plan` reads at this texel (0..1): `mask` is the mask so far
    fn compare_side(&mut self, side: CompareSource, channel: Channel, plan: &MaskPlan, mask: f32, constant: f32) -> f32 {
        let below = self.below.and_then(|acc| acc.get(channel.index()).copied()).unwrap_or_else(|| channel.default_value());
        let read = match side {
            CompareSource::ThisMask => return mask,
            CompareSource::Constant => return constant,
            CompareSource::LayersBelow => below,
            CompareSource::ThisLayer => {
                let own = plan.layer.as_ref().and_then(|(content, chans)| {
                    let chan = chans.iter().find(|c| c.ch == channel.index())?;
                    self.content(content, chan)
                });

                // Where the layer is empty (or thin) it shows the stack below it
                own.map_or(below, |(v, cov)| mix3(below, v, cov))
            }
        };
        match channel.kind() {
            ChannelKind::Color | ChannelKind::NormalMap => lum(read),
            ChannelKind::Scalar => read[0],
            ChannelKind::Signed => read[0] * 0.5 + 0.5,
        }
    }

    /// Composite `nodes` over `acc`, recording anchors. Returns true when it reached the `stop`
    /// layer (`acc` then holds the stack below it).
    fn eval(&mut self, nodes: &[Node], acc: &mut Acc) -> bool {
        for n in nodes {
            if self.stop == Some(n.id) {
                self.below = Some(*acc);
                return true;
            }
            if n.mask.as_ref().is_some_and(|mask| mask.layer.is_some()) {
                self.below = Some(*acc);
            }
            let m = self.tape_mask(n);
            match &n.content {
                // A masked-out group still runs when it records anchors (they hold its inner
                // stack) or may hold the stop layer.
                Content::Group(children) if m > 0.0 || n.has_anchor || self.stop.is_some() => {
                    let mut inner = *acc;
                    if self.eval(children, &mut inner) {
                        return true;
                    }
                    if m > 0.0 {
                        mix_group(acc, &inner, &n.chans, m);
                    }
                }
                Content::Fill(..) | Content::Paint { .. } if m > 0.0 => {
                    let before = if n.anchors.iter().any(|a| a.1.is_some()) { Some(*acc) } else { None };
                    self.lit = self.lit_material(n, acc, m);
                    for c in &n.chans {
                        if let Some((v, cov)) = self.tape_content(&n.content, c) {
                            apply(acc, c, v, cov, m);
                        }
                    }
                    self.lit = None;
                    // Anchors inside the content stack: the stack with the content cut there.
                    for (a, cut) in &n.anchors {
                        let rec = match (cut, before) {
                            (Some(k), Some(mut b)) => {
                                for c in &n.chans {
                                    if let Some((v, cov)) = self.content_to(&n.content, c, *k) {
                                        apply(&mut b, c, v, cov, m);
                                    }
                                }
                                b
                            }
                            _ => *acc,
                        };
                        self.anchors.push((*a, rec));
                    }
                    continue;
                }
                _ => {}
            }
            for (a, _) in &n.anchors {
                self.anchors.push((*a, *acc));
            }
        }
        false
    }

    /// [`Texel::eval`] of the whole stack over `acc`, split along the path to the hot layer
    /// (`levels`, top level first). Cold (`warm` false) it evaluates everything, leaving in
    /// `accs` the stack before the path layer of each level and recording on `tape` the masks
    /// of the folders on the path and what the layers after the path contribute. Warm (`accs`
    /// loaded from the cache, `tape` replaying) only the hot layer is evaluated and the blends
    /// after it are replayed. The order of evaluation differs from `eval` only in masks, which
    /// read no stack values (given no anchor readers, see `BelowCache::prepare`), so the result
    /// is identical.
    fn eval_split(&mut self, mut acc: Acc, levels: &[LevelPlan], accs: &mut [Acc], masks: &mut [f32], warm: bool, tape: &mut Tape<'b>) -> Acc {
        let last = levels.len().saturating_sub(1);
        // Down the path: the deepest level the stack reaches (a folder masked out or not drawn
        // here stops it).
        let mut reach = last;
        for (d, lv) in levels.iter().enumerate() {
            if !warm {
                self.eval(&lv.below, &mut acc);
                if let Some(slot) = accs.get_mut(d) {
                    *slot = acc;
                }
            }
            if d == last {
                break;
            }
            let m = match &lv.at {
                Some(g) => self.taped(tape, |t| t.tape_mask(g)),
                None => 0.0,
            };
            if let Some(slot) = masks.get_mut(d) {
                *slot = m;
            }
            if m > 0.0 {
                continue;
            }
            reach = d;
            break;
        }
        let mut acc = accs.get(reach).copied().unwrap_or(acc);
        if reach == last
            && let Some(hot) = levels.get(last).and_then(|l| l.at.as_ref())
        {
            self.eval(std::slice::from_ref(hot), &mut acc);
        }
        // Back up: the layers after the path, then each folder's mix over the stack outside it.
        for d in (0..=reach).rev() {
            if let Some(lv) = levels.get(d) {
                self.taped(tape, |t| t.eval(&lv.above, &mut acc));
            }
            if let Some(o) = d.checked_sub(1)
                && let (Some(outer), Some(g), Some(m)) = (accs.get(o), levels.get(o).and_then(|l| l.at.as_ref()), masks.get(o))
            {
                let inner = acc;
                acc = *outer;
                mix_group(&mut acc, &inner, &g.chans, *m);
            }
        }
        acc
    }

    /// The mask of node `n` at this texel, combined with the layer's height when it masks with
    /// height.
    fn node_mask(&mut self, n: &Node) -> f32 {
        if n.parts.as_ref().is_some_and(|p| !self.in_parts(p)) {
            return 0.0;
        }
        let m = match &n.mask {
            Some(mp) => self.mask(mp),
            None => 1.0,
        };
        if !n.height_mask.enabled || m <= 0.0 || m >= 1.0 {
            return m;
        }
        let height = Channel::Height.index();
        let h = n.chans.iter().find(|c| c.ch == height).and_then(|c| self.content(&n.content, c)).map_or(0.0, |(v, _)| v[0]);
        n.height_mask.apply(m, h)
    }

    /// Whether this texel's triangle lies in a submesh `allowed` marks (uncovered texels and
    /// meshes without submeshes count as inside).
    fn in_parts(&self, allowed: &[bool]) -> bool {
        let g = self.inp.gbuf;
        let tri = g.tri.get((self.y * g.width + self.x) as usize).copied().unwrap_or(NO_TRI);
        if tri == NO_TRI {
            return true;
        }
        let first = tri.saturating_mul(3);
        let subs = &self.inp.mesh.submeshes;
        let holds = |k: usize| subs.get(k).is_some_and(|s| s.start <= first && first < s.start.saturating_add(s.count));
        // Submeshes are in index order; anything else is searched.
        let k = subs.partition_point(|s| s.start <= first).checked_sub(1).filter(|k| holds(*k)).or_else(|| (0..subs.len()).find(|k| holds(*k)));
        k.and_then(|k| allowed.get(k).copied()).unwrap_or(true)
    }

    /// Record the anchors of the stack below layer `id` (`full` is the planned stack), to
    /// evaluate that layer on its own.
    fn anchors_below(&mut self, full: &[Node], id: LayerId) {
        self.anchors.clear();
        self.stop = Some(id);
        let mut acc = Channel::ALL.map(Channel::default_value);
        self.eval(full, &mut acc);
        self.stop = None;
    }

    /// Straight value and coverage of one channel of a paint or fill layer at this texel (content effects and live stroke preview included); None where the layer is empty
    fn content(&mut self, content: &Content, c: &ChanPlan) -> Option<([f32; 3], f32)> {
        self.content_to(content, c, usize::MAX)
    }

    /// [`Texel::content`] with only the first `end` visible content effects.
    fn content_to(&mut self, content: &Content, c: &ChanPlan, end: usize) -> Option<([f32; 3], f32)> {
        let start = c.start.filter(|(k, _)| *k < end);
        let (mut v, mut cov, skip) = match start.and_then(|(k, f)| f.get(self.x, self.y).map(|d| (k, d))) {
            Some((k, d)) => {
                let px = |i: usize| d.get(i).copied().unwrap_or(0.0);
                ([px(0), px(1), px(2)], px(3), k + 1)
            }
            None => {
                let (v, cov) = match content {
                    Content::Fill(f, depth) => {
                        let ch = Channel::ALL.get(c.ch).copied()?;
                        self.symmetric(&f.projection, f.sampling, |t| {
                            // A channel without a source fills with its default (no clone per texel)
                            let v = match f.sources.get(&ch) {
                                Some(s) => t.source(s, &f.projection, f.sampling, ch.kind()),
                                None => ch.default_value(),
                            };
                            (v, t.cover(f.sources.values(), &f.projection, f.sampling, depth.as_deref()))
                        })
                    }
                    Content::Paint { preview } => self.painted(c.data.as_deref(), c, *preview),
                    Content::Group(_) => return None,
                };
                (v, cov, 0)
            }
        };
        for e in c.effects.iter().take(end).skip(skip) {
            (v, cov) = self.content_effect(e, c, v, cov);
        }
        (cov > 0.0).then_some((v, cov.min(1.0)))
    }

    /// Straight value and coverage of painted data of one channel (premultiplied values plus
    /// coverage per texel), with the live stroke when `preview`.
    fn painted(&mut self, data: Option<&[u16]>, c: &ChanPlan, preview: bool) -> ([f32; 3], f32) {
        let pv = if preview { self.inp.preview } else { None };
        let mut sa = pv.map(|p| (self.stroke * p.opacity).clamp(0.0, 1.0)).unwrap_or(0.0);
        // Projection: the stencil's alpha shapes the stroke and its color is the base color.
        let stencil = match pv.and_then(|p| p.stencil.as_ref()) {
            Some(st) if sa > 0.0 => {
                let s = self.world().and_then(|w| st.sample_world(w));
                sa *= s.map(|(_, a)| a).unwrap_or(0.0);
                s.map(|(c, _)| c)
            }
            _ => None,
        };
        let mut val = [0.0f32; 3];
        let mut cov = 0.0f32;
        if let Some(d) = data {
            let base = self.o * (c.comps + 1);
            for (k, slot) in val.iter_mut().take(c.comps).enumerate() {
                *slot = decode(c.kind, d.get(base + k).copied().unwrap_or(0));
            }
            cov = from_u16(d.get(base + c.comps).copied().unwrap_or(0));
        }
        if let Some(p) = pv
            && sa > 0.0
        {
            if p.erase {
                val = val.map(|v| v * (1.0 - sa));
                cov *= 1.0 - sa;
            } else if let Some(ch) = Channel::ALL.get(c.ch)
                && let Some(pval) = p.values.get(ch)
            {
                let pval = match stencil {
                    Some(col) if *ch == Channel::BaseColor => col,
                    _ => match p.carried.get(ch) {
                        Some(img) => {
                            let raw = img.get(self.x, self.y).map(|v| decode(c.kind, v));
                            [raw[0], raw[1], raw[2]]
                        }
                        None => *pval,
                    },
                };
                for k in 0..c.comps {
                    val[k] = pval[k] * sa + val[k] * (1.0 - sa);
                }
                cov = sa + cov * (1.0 - sa);
            }
        }
        if cov <= 0.0 {
            return ([0.0; 3], 0.0);
        }
        let inv = 1.0 / cov.max(1e-5);
        let v = if c.comps == 3 { val.map(|x| x * inv) } else { [val[0] * inv; 3] };
        (v, cov.min(1.0))
    }

    /// Apply one content effect to a channel's straight value and coverage.
    fn content_effect(&mut self, e: &EffectPlan, c: &ChanPlan, v: [f32; 3], cov: f32) -> ([f32; 3], f32) {
        let o = e.opacity.clamp(0.0, 1.0);
        match &*e.kind {
            EffectKind::Paint | EffectKind::Path(_) => {
                let (p, pc) = self.painted(e.data.as_deref(), c, e.preview);
                over(c, e.blend, v, cov, p, pc * o)
            }
            EffectKind::Material(f) => match Channel::ALL.get(c.ch).and_then(|ch| f.sources.get(ch).map(|s| (*ch, s))) {
                Some((ch, s)) => {
                    let (p, w) = self.symmetric(&f.projection, f.sampling, |t| {
                        (t.source(s, &f.projection, f.sampling, ch.kind()), t.cover(f.sources.values(), &f.projection, f.sampling, e.depth.as_deref()))
                    });
                    over(c, e.blend, v, cov, p, w * o)
                }
                None => (v, cov),
            },
            EffectKind::Levels(l) => match Channel::ALL.get(c.ch) {
                Some(&channel) if l.remaps(channel) => (mix3(v, per_unit(c.kind, v, |x| l.apply(x)), o), cov),
                _ => (v, cov),
            },
            // Baked lighting reads the texel's normal and occlusion, and lights the base color only.
            EffectKind::Filter(f @ Filter::BakedLighting { .. }) => match Channel::ALL.get(c.ch) {
                Some(Channel::BaseColor) => {
                    let ao = self.baked(BakedMap::AmbientOcclusion).unwrap_or(1.0);
                    let lit = match f.stylized_lighting() {
                        Some((stylized, light)) => {
                            let nrm = self.lit_normal(stylized);
                            let curvature = self.baked(BakedMap::Curvature).unwrap_or(FLAT_CURVATURE);
                            let material = self.lit.unwrap_or_else(|| Channel::ALL.map(Channel::default_value));
                            stylized.shade(light, v, nrm, [ao, curvature], &material)
                        }
                        None => {
                            let k = f.baked_light(self.coords().nrm.to_array(), ao).unwrap_or(1.0);
                            v.map(|x| (x * k).clamp(0.0, 1.0))
                        }
                    };
                    (mix3(v, lit, o), cov)
                }
                _ => (v, cov),
            },
            // Environment lighting reads the texel's normal, occlusion and metal, and lights the base color only
            EffectKind::Filter(Filter::EnvironmentLighting { horizontal, vertical, exposure, height_intensity, ambient_occlusion, specular_occlusion }) => {
                match (Channel::ALL.get(c.ch), &e.environment) {
                    (Some(Channel::BaseColor), Some(environment)) => {
                        let ao = self.baked(BakedMap::AmbientOcclusion).unwrap_or(1.0);
                        let nrm = Vec3::from(self.detail_normal(height_intensity / ENVIRONMENT_HEIGHT_UNIT));
                        let light = environment.irradiance(nrm, horizontal.to_radians(), vertical.to_radians())
                            * exposure.clamp(-EXPOSURE_STOPS, EXPOSURE_STOPS).exp2();
                        let material = self.lit.unwrap_or_else(|| Channel::ALL.map(Channel::default_value));
                        let lit = EnvironmentSh::shade(v, light, ao, [*ambient_occlusion, *specular_occlusion], &material);
                        (mix3(v, lit, o), cov)
                    }
                    _ => (v, cov),
                }
            }

            // PBR Validate checks the base color against the layer's metal and leaves the other channels as they are
            EffectKind::Filter(Filter::PbrValidate(validate)) => match Channel::ALL.get(c.ch) {
                Some(Channel::BaseColor) => {
                    let metal = self.lit.and_then(|material| material.get(Channel::Metallic.index()).map(|metal| metal[0])).unwrap_or(0.0);
                    (mix3(v, validate.check(v, metal), o), cov)
                }
                _ => (v, cov),
            },
            // A gradient's debug view shows its ramp left to right across each UV tile
            EffectKind::Filter(f @ Filter::GradientMap { debug: true, .. }) => {
                let ramp = f.ramp_at(self.coords().uv.x.rem_euclid(1.0)).map(|color| if c.kind == ChannelKind::Color { color } else { [lum(color); 3] });
                (ramp.map_or(v, |ramp| mix3(v, ramp, o)), cov)
            }

            // Posterize with Apply to Alpha steps the content's coverage too
            EffectKind::Filter(f @ Filter::Posterize { levels, alpha: true }) => {
                let stepped = cov + (posterize(cov, *levels) - cov) * o;
                (f.point(c.kind, v).map_or(v, |out| mix3(v, out, o)), stepped)
            }

            // Grayscale Conversion weighs the coverage as Alpha, or reads its Custom Input
            EffectKind::Filter(Filter::GrayscaleConversion(conversion)) => {
                let (read, alpha) = match &e.input {
                    Some(source) => (self.source(source, &Projection::default(), Sampling::default(), ChannelKind::Color), 1.0),
                    None => (v, cov),
                };
                (mix3(v, [conversion.gray(read, alpha); 3], o), cov)
            }
            // Neighbourhood filters come precomputed through `ChanPlan::start`.
            EffectKind::Filter(f) => (f.point(c.kind, v).map_or(v, |out| mix3(v, out, o)), cov),
            // Anchors are recorded by `eval`; mask-only effects leave content as it is.
            EffectKind::Anchor | EffectKind::Fill { .. } | EffectKind::Generator(_) | EffectKind::ColorSelection(_) | EffectKind::Compare { .. } => (v, cov),
        }
    }
}

/// Mix a folder's inner stack into the stack outside it by its opacity and mask `m`.
fn mix_group(acc: &mut Acc, inner: &Acc, chans: &[ChanPlan], m: f32) {
    for c in chans {
        if let (Some(a), Some(b)) = (acc.get(c.ch).copied(), inner.get(c.ch).copied())
            && let Some(slot) = acc.get_mut(c.ch)
        {
            *slot = mix3(a, b, c.opacity * m);
        }
    }
}

#[inline]
fn apply(acc: &mut Acc, c: &ChanPlan, v: [f32; 3], cov: f32, mask: f32) {
    let Some(a) = acc.get(c.ch).copied() else { return };
    let w = if c.blend == BlendMode::Replace { c.opacity * mask } else { cov * c.opacity * mask };
    let blended = blend_chan(c, c.blend, a, v);
    if let Some(slot) = acc.get_mut(c.ch) {
        *slot = mix3(a, blended, w);
    }
}

/// Composite `tiles` of texture set `inp.ts` into `out` (other tiles are left as they are).
pub fn composite_tiles(inp: &Inputs, out: &mut Composite, tiles: &[(u32, u32)]) {
    composite_tiles_cached(inp, out, tiles, None);
}

/// Tiles per side of the blocks a composite works through (bounds neighbourhood-filter memory).
const BLOCK: u32 = 16;

/// [`composite_tiles`], serving everything but the hot layer from `below` where it can (and
/// filling it). Results are identical to [`composite_tiles`].
pub fn composite_tiles_cached(inp: &Inputs, out: &mut Composite, tiles: &[(u32, u32)], mut below: Option<&mut BelowCache>) {
    let Ok(set) = inp.project.texture_set(inp.ts) else { return };
    if (inp.gbuf.width, inp.gbuf.height) != (out.resolution, out.height) {
        return;
    }
    let split = below.as_deref_mut().and_then(|b| b.prepare(inp));
    // A pass over more tiles than the cache holds records only what fits: recording the rest
    // would just evict it again. Tiles hold the stack per level plus a tape of varying size.
    let levels = split.as_ref().map_or(1, Vec::len);
    let estimate = TILE_AREA * levels * set.channel_list().iter().map(|c| c.components()).sum::<usize>() * std::mem::size_of::<f32>();
    let tile_bytes = below.as_deref().and_then(BelowCache::tile_bytes).unwrap_or(estimate).max(1);
    let fits = below.as_deref().is_some_and(|b| tiles.len().max(b.pass) <= b.limit() / tile_bytes);
    let mut blocks: BTreeMap<(u32, u32), Vec<(u32, u32)>> = BTreeMap::new();
    for t in tiles {
        blocks.entry((t.1 / BLOCK, t.0 / BLOCK)).or_default().push(*t);
    }
    for block in blocks.values() {
        let quota = match below.as_deref() {
            Some(_) if fits => usize::MAX,
            Some(b) => b.limit().saturating_sub(b.bytes()) / tile_bytes,
            None => 0,
        };
        let cached = below.as_deref().zip(split.as_deref());
        let records = composite_block(inp, &set.stack, out, block, cached, quota);
        if let Some(b) = below.as_deref_mut() {
            for t in block {
                b.touch(*t);
            }
            for (t, bands) in records {
                b.insert(t, bands);
            }
        }
    }
}

/// Rows of a tile composited as one parallel task.
const BAND: u32 = 16;

/// One band of a tile: per-channel buffers, what to cache of it (when recorded) and whether
/// any texel is covered.
type BandResult = (Vec<Vec<u16>>, Option<Band>, bool);

/// Per-channel tiles of one composited tile and whether any texel is covered.
type Joined = (Vec<Option<std::sync::Arc<Vec<u16>>>>, bool);

/// Composite one block of tiles, split along the path to the hot layer when `below` holds it;
/// returns what to cache (for at most `quota` tiles).
fn composite_block(
    inp: &Inputs,
    stack: &[LayerId],
    out: &mut Composite,
    tiles: &[(u32, u32)],
    below: Option<(&BelowCache, &[Level])>,
    quota: usize,
) -> Vec<((u32, u32), Vec<Band>)> {
    let channels: Vec<Channel> = inp.project.texture_set(inp.ts).map(|s| s.channel_list()).unwrap_or_default();
    let (w, h) = (out.resolution, out.height);
    let split = below.map(|(_, l)| l);
    let cached = |t: (u32, u32)| below.and_then(|(b, _)| b.tile(t));
    let fresh: Vec<(u32, u32)> = tiles.iter().copied().filter(|t| cached(*t).is_none()).collect();
    let recorded: std::collections::BTreeSet<(u32, u32)> = if split.is_some() { fresh.iter().copied().take(quota).collect() } else { Default::default() };
    // Off the path, layers are only evaluated on tiles the cache lacks.
    let fields = match split.and_then(<[Level]>::last) {
        Some(hot) if fresh.len() < tiles.len() => {
            let mut fields = fields_of(inp, &channels, stack, &fresh);
            fields.extend(fields_of(inp, &channels, &[hot.at], tiles));
            fields
        }
        _ => fields_of(inp, &channels, stack, tiles),
    };
    let lanes: Vec<Lane> = channels.iter().map(|c| Lane { ch: c.index(), n: c.components(), signed: c.kind() == ChannelKind::Signed }).collect();
    let stride: usize = lanes.iter().map(|l| l.n).sum();
    let nlev = split.map_or(0, <[Level]>::len);
    let defaults: Acc = Channel::ALL.map(Channel::default_value);
    // Tiles split into bands of rows, so a handful of tiles still keeps every thread busy.
    let bands = TILE / BAND;
    let tasks: Vec<((u32, u32), u32)> = tiles.iter().flat_map(|t| (0..bands).map(move |b| (*t, b))).collect();
    let mut results: Vec<BandResult> = tasks
        .par_iter()
        .map(|&((tx, ty), band)| {
            let warm = cached((tx, ty)).and_then(|b| b.get(band as usize));
            let record = warm.is_none() && recorded.contains(&(tx, ty));
            let levels = split.map(|l| plan_levels(inp, l, &channels, tx, ty, &fields, warm.is_some()));
            let nodes = if levels.is_none() { plan(inp, stack, &channels, tx, ty, 0, Some(&fields)) } else { Vec::new() };
            let rows = (BAND * TILE) as usize;
            let mut accs = vec![defaults; nlev];
            let mut masks = vec![0.0f32; nlev];
            let mut tape = match warm {
                Some(b) => Tape::Replay(b, 0),
                None if record => Tape::Record(Vec::new()),
                None => Tape::Off,
            };
            // The levels whose stack the band stores (its header): a level with nothing drawn
            // before the path layer repeats the level above it.
            let stored: Vec<bool> = match (&levels, warm) {
                (Some(_), Some(_)) => tape.take(nlev).iter().map(|v| *v > 0.0).collect(),
                (Some(levels), None) => levels.iter().map(|l| !l.below.is_empty()).collect(),
                (None, _) => Vec::new(),
            };
            if let Tape::Record(t) = &mut tape {
                t.extend(stored.iter().map(|s| if *s { 1.0 } else { 0.0 }));
            }
            let slots = stored.iter().filter(|s| **s).count() * stride;
            let preview_tile = inp.preview.and_then(|p| p.alpha.tile(tx, ty));
            let mut bufs: Vec<Vec<u16>> = channels.iter().map(|c| vec![0u16; rows * c.components()]).collect();
            let mut anchors = Vec::new();
            let mut any = false;
            for ly in band * BAND..(band + 1) * BAND {
                let y = ty * TILE + ly;
                if y >= h {
                    break;
                }
                for lx in 0..TILE {
                    let x = tx * TILE + lx;
                    if x >= w {
                        break;
                    }
                    let gi = (y * w + x) as usize;
                    // Offset in the tile, and in this band.
                    let o = (ly * TILE + lx) as usize;
                    let ob = ((ly - band * BAND) * TILE + lx) as usize;
                    let covered = inp.gbuf.tri.get(gi).is_some_and(|t| *t != NO_TRI);
                    let mut acc = defaults;
                    if covered {
                        any = true;
                        let mut tex = Texel::new(inp, x, y, o, stroke_at(preview_tile, o));
                        // Reuse one anchor buffer per band.
                        tex.anchors = std::mem::take(&mut anchors);
                        match &levels {
                            Some(levels) => {
                                // The texel's stream starts with the stack at each level.
                                let start = match &mut tape {
                                    Tape::Record(t) => {
                                        t.resize(t.len() + slots, 0.0);
                                        t.len() - slots
                                    }
                                    _ => 0,
                                };
                                if warm.is_some() {
                                    load_levels(tape.take(slots), &lanes, stride, &stored, &defaults, &mut accs);
                                }
                                acc = tex.eval_split(defaults, levels, &mut accs, &mut masks, warm.is_some(), &mut tape);
                                if let Tape::Record(t) = &mut tape
                                    && let Some(below) = t.get_mut(start..start + slots)
                                {
                                    store_levels(below, &lanes, stride, &stored, &accs);
                                }
                            }
                            None => {
                                tex.eval(&nodes, &mut acc);
                            }
                        }
                        anchors = tex.anchors;
                        anchors.clear();
                    }
                    for (l, buf) in lanes.iter().zip(&mut bufs) {
                        let v = acc.get(l.ch).copied().unwrap_or([0.0; 3]);
                        for (k, x) in v.iter().take(l.n).enumerate() {
                            let enc = if l.signed { to_u16_signed(*x) } else { to_u16(*x) };
                            if let Some(slot) = buf.get_mut(ob * l.n + k) {
                                *slot = enc;
                            }
                        }
                    }
                }
            }
            let cache = if let Tape::Record(t) = tape { Some(t) } else { None };
            (bufs, cache, any)
        })
        .collect();
    // Join the bands of each tile (in parallel; uncovered tiles are freed).
    let joined: Vec<Joined> = results
        .par_chunks(bands as usize)
        .map(|parts| {
            let any = parts.iter().any(|p| p.2);
            let data = (0..channels.len())
                .map(|k| {
                    any.then(|| {
                        let mut v = Vec::with_capacity(parts.iter().map(|p| p.0.get(k).map_or(0, Vec::len)).sum());
                        for p in parts {
                            v.extend_from_slice(p.0.get(k).map_or(&[][..], Vec::as_slice));
                        }
                        std::sync::Arc::new(v)
                    })
                })
                .collect();
            (data, any)
        })
        .collect();
    let mut records = Vec::new();
    for ((&(tx, ty), (data, any)), parts) in tiles.iter().zip(joined).zip(results.chunks_mut(bands as usize)) {
        for (c, d) in channels.iter().zip(data) {
            if let Some(img) = out.channels.get_mut(c) {
                img.set_tile(tx, ty, d);
            }
        }
        // Tiles without covered texels cost nothing to composite: not worth caching.
        if any && let Some(bands) = parts.iter_mut().map(|p| p.1.take()).collect::<Option<Vec<Band>>>() {
            records.push(((tx, ty), bands));
        }
    }
    records
}

/// Restore the stack at each nesting level of a texel: the `stored` levels from `src`, the
/// others repeat the level above them (the defaults at the top).
fn load_levels(src: &[f32], lanes: &[Lane], stride: usize, stored: &[bool], defaults: &Acc, accs: &mut [Acc]) {
    let (mut k, mut above) = (0, *defaults);
    for (a, s) in accs.iter_mut().zip(stored) {
        if *s {
            load_below(src, lanes, stride, k, a);
            k += 1;
        } else {
            *a = above;
        }
        above = *a;
    }
}

/// Store the `stored` levels of a texel's stack (see [`load_levels`]).
fn store_levels(dst: &mut [f32], lanes: &[Lane], stride: usize, stored: &[bool], accs: &[Acc]) {
    for (k, a) in accs.iter().zip(stored).filter(|(_, s)| **s).map(|(a, _)| a).enumerate() {
        store_below(dst, lanes, stride, k, a);
    }
}

/// Where a texture set channel lives in a stack value, its components and whether it is signed.
#[derive(Clone, Copy)]
struct Lane {
    ch: usize,
    n: usize,
    signed: bool,
}

/// Restore the cached stack value of slot `o` (a texel at one nesting level). Scalars are
/// cached once: only their first component is ever read.
fn load_below(cache: &[f32], lanes: &[Lane], stride: usize, o: usize, acc: &mut Acc) {
    let mut i = o * stride;
    for l in lanes {
        if let (Some(src), Some(slot)) = (cache.get(i..i + l.n), acc.get_mut(l.ch)) {
            *slot = match src {
                [a, b, c] => [*a, *b, *c],
                _ => [src.first().copied().unwrap_or(0.0); 3],
            };
        }
        i += l.n;
    }
}

fn store_below(cache: &mut [f32], lanes: &[Lane], stride: usize, o: usize, acc: &Acc) {
    let mut i = o * stride;
    for l in lanes {
        if let (Some(dst), Some(v)) = (cache.get_mut(i..i + l.n), acc.get(l.ch)) {
            for (d, x) in dst.iter_mut().zip(v) {
                *d = *x;
            }
        }
        i += l.n;
    }
}

/// UV padding for display: fill uncovered texels in `tiles` and their neighbours from their
/// nearest covered texel. Returns the tiles it wrote.
pub fn pad_tiles(out: &mut Composite, dmap: &DilationMap, tiles: &[(u32, u32)]) -> std::collections::BTreeSet<(u32, u32)> {
    let mut written = std::collections::BTreeSet::new();
    let (w, h) = (out.resolution, out.height);
    if (dmap.width, dmap.height) != (w, h) {
        return written;
    }
    let (nx, ny) = (w.div_ceil(TILE) as i64, h.div_ceil(TILE) as i64);
    let mut set = std::collections::BTreeSet::new();
    for &(tx, ty) in tiles {
        for dy in -1..=1i64 {
            for dx in -1..=1i64 {
                let (x, y) = (tx as i64 + dx, ty as i64 + dy);
                if x >= 0 && y >= 0 && x < nx && y < ny {
                    set.insert((x as u32, y as u32));
                }
            }
        }
    }
    let set: Vec<(u32, u32)> = set.into_iter().collect();
    for img in out.channels.values_mut() {
        let comps = img.comps();
        let src: &TileImage = img;
        // Padding writes only uncovered texels and reads only covered ones, so tiles pad
        // independently (in parallel) from the unpadded image.
        let padded: Vec<((u32, u32), Vec<u16>)> = set
            .par_iter()
            .filter_map(|&(tx, ty)| {
                let mut data: Option<Vec<u16>> = None;
                for ly in 0..TILE {
                    for lx in 0..TILE {
                        let (x, y) = (tx * TILE + lx, ty * TILE + ly);
                        if x >= w || y >= h {
                            continue;
                        }
                        let Some((sx, sy)) = dmap.source(x, y) else { continue };
                        if (sx, sy) == (x, y) {
                            continue;
                        }
                        let v = src.get(sx, sy);
                        let d = data.get_or_insert_with(|| src.tile(tx, ty).map_or_else(|| vec![0; TILE_AREA * comps], <[u16]>::to_vec));
                        let o = (ly * TILE + lx) as usize * comps;
                        if let Some(px) = d.get_mut(o..o + comps) {
                            px.copy_from_slice(&v[..comps]);
                        }
                    }
                }
                Some(((tx, ty), data?))
            })
            .collect();
        for ((tx, ty), d) in padded {
            img.set_tile(tx, ty, Some(std::sync::Arc::new(d)));
            written.insert((tx, ty));
        }
    }
    written
}

/// The layer mask as a square grayscale image, using nearest texels and black outside the mesh
/// Returns None without an enabled mask
pub fn mask_image(inputs: &Inputs, id: LayerId, size: u32) -> Option<Vec<u8>> {
    let mask = layer_mask(inputs.project, id).filter(|mask| mask.enabled)?;
    let channels = inputs.project.texture_set(inputs.ts).map(|set| set.channel_list()).unwrap_or_default();
    let (width, height) = (inputs.gbuf.width, inputs.gbuf.height);
    let coordinates = all_tiles(width, height);

    // A hidden layer's mask is left out of the stack's preparation
    prepare_mask_curvature_sizes(inputs, std::iter::once(mask));

    let fields = neighbourhood_fields(inputs, &channels, &coordinates);
    let tiles: HashMap<(u32, u32), Vec<u8>> = coordinates
        .par_iter()
        .map(|&tile| {
            let mut values = vec![0u8; TILE_AREA];
            for (offset, value) in mask_tile(inputs, id, mask, &channels, &fields, tile, 0..TILE_AREA) {
                if let Some(destination) = values.get_mut(offset) {
                    *destination = (value * 255.0 + 0.5) as u8;
                }
            }
            (tile, values)
        })
        .collect();

    let size = size.max(1);
    let mut output = vec![0u8; (size * size) as usize];
    for y in 0..size {
        for x in 0..size {
            let sample_x = (u64::from(x) * u64::from(width) / u64::from(size)) as u32;
            let sample_y = (u64::from(y) * u64::from(height) / u64::from(size)) as u32;
            if let Some(tile) = tiles.get(&(sample_x / TILE, sample_y / TILE))
                && let Some(value) = tile.get(((sample_y % TILE) * TILE + sample_x % TILE) as usize)
                && let Some(destination) = output.get_mut((y * size + x) as usize)
            {
                *destination = *value;
            }
        }
    }
    Some(output)
}

/// Mask values at texture-set coordinates, in request order; None without an enabled mask
/// Uncovered and out-of-range texels read zero; neighbourhood filters also evaluate surrounding texels
pub fn mask_at(inputs: &Inputs, id: LayerId, texels: &[(u32, u32)]) -> Option<Vec<f32>> {
    let mask = layer_mask(inputs.project, id).filter(|mask| mask.enabled)?;
    let mut output = vec![0.0; texels.len()];

    // Group valid requests before planning effects, which may resample painted tiles
    let mut requested: HashMap<(u32, u32), Vec<(usize, usize)>> = HashMap::new();
    for (index, &(x, y)) in texels.iter().enumerate() {
        if x >= inputs.gbuf.width || y >= inputs.gbuf.height {
            continue;
        }

        let offset = ((y % TILE) * TILE + x % TILE) as usize;
        requested.entry((x / TILE, y / TILE)).or_default().push((offset, index));
    }

    if requested.is_empty() {
        return Some(output);
    }

    let channels = inputs.project.texture_set(inputs.ts).map(|set| set.channel_list()).unwrap_or_default();

    // A hidden layer's mask is left out of the stack's preparation
    prepare_mask_curvature_sizes(inputs, std::iter::once(mask));

    // Filters and anchors can depend on texels outside the requested tiles
    let fields = neighbourhood_fields(inputs, &channels, &all_tiles(inputs.gbuf.width, inputs.gbuf.height));
    let values: Vec<(usize, f32)> = requested
        .par_iter_mut()
        .flat_map_iter(|(&tile, requests)| {
            requests.sort_unstable_by_key(|&(offset, _)| offset);
            let groups = requests.chunk_by(|left, right| left.0 == right.0);
            let offsets = groups.clone().filter_map(|group| group.first().map(|&(offset, _)| offset));
            mask_tile(inputs, id, mask, &channels, &fields, tile, offsets)
                .zip(groups)
                .flat_map(|((_, value), group)| group.iter().map(move |&(_, index)| (index, value)))
        })
        .collect();

    for (index, value) in values {
        if let Some(destination) = output.get_mut(index) {
            *destination = value;
        }
    }
    Some(output)
}

/// Mask values in offset order, including zeros for uncovered texels so request groups stay aligned
fn mask_tile<'a>(
    inputs: &'a Inputs<'a>,
    id: LayerId,
    mask: &'a Mask,
    channels: &[Channel],
    fields: &'a Fields,
    (tile_x, tile_y): (u32, u32),
    offsets: impl Iterator<Item = usize> + 'a,
) -> impl Iterator<Item = (usize, f32)> + 'a {
    let plan = mask_plan(inputs, id, mask, tile_x, tile_y, Some(fields), inputs.project.contents_around(id));
    let anchor_stack = anchor_plan(inputs, id, channels, tile_x, tile_y, Some(fields));
    offsets.map(move |offset| {
        let Some(plan) = plan.as_ref() else { return (offset, 0.0) };

        let x = tile_x * TILE + (offset % TILE as usize) as u32;
        let y = tile_y * TILE + (offset / TILE as usize) as u32;
        if x >= inputs.gbuf.width || y >= inputs.gbuf.height || inputs.gbuf.tri.get((y * inputs.gbuf.width + x) as usize).is_none_or(|tri| *tri == NO_TRI) {
            return (offset, 0.0);
        }

        let mut texel = Texel::new(inputs, x, y, offset, 0.0);
        if !anchor_stack.is_empty() {
            texel.anchors_below(&anchor_stack, id);
        }
        (offset, texel.mask(plan))
    })
}

/// Tiles [`effect_histogram`] samples at most (a million texels).
const HISTOGRAM_TILES: usize = 256;

/// The tiles [`effect_histogram`] reads: an even sample of at most [`HISTOGRAM_TILES`], which
/// keeps the cost flat up to 8K. The stride is odd: a power of two would divide the row length
/// and sample the same few columns.
fn histogram_tiles(width: u32, height: u32) -> Vec<(u32, u32)> {
    let all = all_tiles(width, height);
    let stride = all.len().div_ceil(HISTOGRAM_TILES).max(1) | 1;
    all.into_iter().step_by(stride).collect()
}

/// Histogram (`bins` buckets over 0..1) of the values entering effect `effect`: its layer's mask below it, or (a content effect) the layer's `channel` below it where the layer has content, signed values mapped to 0..1, a color's components pooled, on their own and as luminance. Over the texels the mesh covers (in an even sample of tiles on large sets); None for an unknown effect
pub fn effect_histogram(inp: &Inputs, effect: LayerId, channel: Channel, bins: usize) -> Option<EffectHistogram> {
    let (owner, _) = inp.project.effect(effect)?;
    let bins = bins.max(1);
    let (cut, in_mask) = cut_below(inp.project, owner, effect)?;
    let cut_inp = Inputs { project: &cut, preview: None, block_scales: inp.block_scales.clone(), ..*inp };
    let channels = cut.texture_set(inp.ts).map(|t| t.channel_list()).unwrap_or_default();
    let tiles = histogram_tiles(inp.gbuf.width, inp.gbuf.height);
    // A field covers the bounding box of the tiles it is built for, which for a scattered sample
    // is the whole set: with a neighbourhood filter below, each tile gets its own.
    let probe = neighbourhood_fields(&cut_inp, &channels, tiles.get(..1).unwrap_or_default());
    let shared = probe.is_empty().then_some(probe);
    let color = !in_mask && channel.kind() == ChannelKind::Color;
    let hist = tiles
        .par_iter()
        .map(|&(tx, ty)| {
            let own;
            let fields = match &shared {
                Some(f) => f,
                None => {
                    own = neighbourhood_fields(&cut_inp, &channels, &[(tx, ty)]);
                    &own
                }
            };
            let mut h = EffectHistogram::new(bins, color);
            // The mask is planned on its own: a layer with nothing drawn plans no node.
            let mask = if in_mask {
                layer_mask(&cut, owner).and_then(|m| mask_plan(&cut_inp, owner, m, tx, ty, Some(fields), cut.contents_around(owner)))
            } else {
                None
            };
            let nodes = if in_mask { Vec::new() } else { plan(&cut_inp, &[owner], &channels, tx, ty, 0, Some(fields)) };
            let content = nodes.first().and_then(|n| n.chans.iter().find(|c| Channel::ALL.get(c.ch) == Some(&channel)).map(|c| (n, c)));
            if mask.is_none() && content.is_none() {
                return h;
            }
            let full = anchor_plan(&cut_inp, owner, &channels, tx, ty, Some(fields));
            for (x, y, o) in tile_texels(inp.gbuf, tx, ty) {
                let mut tex = Texel::new(&cut_inp, x, y, o, 0.0);
                if !full.is_empty() {
                    tex.anchors_below(&full, owner);
                }
                if let Some(m) = &mask {
                    h.add_value(tex.mask(m));
                } else if let Some((n, c)) = content
                    && let Some((v, _)) = tex.content(&n.content, c)
                {
                    // As levels reads them (`per_unit`); normal maps pass through levels unchanged.
                    match c.kind {
                        ChannelKind::Color => h.add_color(v),
                        ChannelKind::Scalar => h.add_value(v[0]),
                        ChannelKind::Signed => h.add_value(v[0] * 0.5 + 0.5),
                        ChannelKind::NormalMap => {}
                    }
                }
            }
            h
        })
        .reduce(|| EffectHistogram::new(bins, color), |a, b| a.merged(&b));
    Some(hist)
}

/// The project with layer `owner` forced visible, its mask enabled, cut just below its effect `effect`, and whether the effect is in the mask
fn cut_below(project: &Project, owner: LayerId, effect: LayerId) -> Option<(Project, bool)> {
    let mut cut = project.clone();
    let l = cut.layers.get_mut(&owner)?;
    l.visible = true;
    let in_mask = match l.mask.as_mut().and_then(|m| m.effects.iter().position(|e| e.id == effect).map(|k| (m, k))) {
        Some((m, k)) => {
            m.enabled = true;
            m.effects.truncate(k);
            true
        }
        None => {
            let k = l.content_effects.iter().position(|e| e.id == effect)?;
            l.content_effects.truncate(k);
            false
        }
    };
    Some((cut, in_mask))
}

/// Histograms (`bins` buckets over 0..1) of what compare mask `effect` reads on its left and right, over [`effect_histogram`]'s texels (None for another effect)
pub fn compare_histograms(inp: &Inputs, effect: LayerId, bins: usize) -> Option<[Vec<u32>; 2]> {
    let (owner, found) = inp.project.effect(effect)?;
    let EffectKind::Compare { left, right, channel, value: constant, .. } = found.kind else { return None };
    let bins = bins.max(1);
    let (cut, in_mask) = cut_below(inp.project, owner, effect)?;
    let set = cut.texture_set(inp.ts).ok()?;
    if !in_mask {
        return None;
    }

    let cut_inp = Inputs { project: &cut, preview: None, block_scales: inp.block_scales.clone(), ..*inp };
    let mut channels = set.channel_list();
    if !channels.contains(&channel) {
        channels.push(channel);
    }
    let reads_stack = found.kind.reads_stack();
    let tiles = histogram_tiles(inp.gbuf.width, inp.gbuf.height);
    let probe = neighbourhood_fields(&cut_inp, &channels, tiles.get(..1).unwrap_or_default());
    let shared = probe.is_empty().then_some(probe);
    let empty = || [vec![0u32; bins], vec![0u32; bins]];
    let counts = tiles
        .par_iter()
        .map(|&(tx, ty)| {
            let own;
            let fields = match &shared {
                Some(f) => f,
                None => {
                    own = neighbourhood_fields(&cut_inp, &channels, &[(tx, ty)]);
                    &own
                }
            };
            let mut counts = empty();

            // The mask under the compare, and (sides reading layers) the layer's content and the stack below it
            let Some(mut mask) = layer_mask(&cut, owner).and_then(|m| mask_plan(&cut_inp, owner, m, tx, ty, Some(fields), cut.contents_around(owner))) else {
                return counts;
            };

            let full = if reads_stack {
                mask.layer = cut
                    .content_layer(owner)
                    .and_then(|l| plan_content(&cut_inp, owner, l, &[channel], tx, ty, 0, Some(fields), false, cut.contents_around(owner)));
                plan(&cut_inp, &set.stack, &channels, tx, ty, 0, Some(fields))
            } else {
                Vec::new()
            };

            for (x, y, o) in tile_texels(inp.gbuf, tx, ty) {
                let mut tex = Texel::new(&cut_inp, x, y, o, 0.0);
                if !full.is_empty() {
                    tex.anchors_below(&full, owner);
                }
                let so_far = tex.mask(&mask);
                for (side, slot) in [left, right].into_iter().zip(counts.iter_mut()) {
                    let read = tex.compare_side(side, channel, &mask, so_far, constant);
                    let index = ((read.clamp(0.0, 1.0) * bins as f32) as usize).min(bins - 1);
                    if let Some(count) = slot.get_mut(index) {
                        *count = count.saturating_add(1);
                    }
                }
            }
            counts
        })
        .reduce(empty, |mut a, b| {
            for (into, from) in a.iter_mut().zip(b) {
                for (count, more) in into.iter_mut().zip(from) {
                    *count = count.saturating_add(more);
                }
            }
            a
        });
    Some(counts)
}

/// One flattened tile of one channel: (channel, tile, premultiplied value + coverage data).
pub type FlatTile = (Channel, (u32, u32), Vec<u16>);

/// Flatten one paint or fill layer: its content (with content effects) times its mask, as
/// premultiplied (value, coverage) tiles per enabled channel, ready to become a plain paint
/// layer. Groups are not supported (None).
pub fn flatten_layer(inputs: &Inputs, id: LayerId, tiles: &[(u32, u32)]) -> Option<Vec<FlatTile>> {
    let set = inputs.project.texture_set(inputs.ts).ok()?;
    if inputs.project.content_layer(id)?.is_group() {
        return None;
    }

    let channels: Vec<Channel> = set.channel_list();

    // Plan the layer forced visible so hidden layers flatten too
    let mut solo = inputs.project.clone();
    if let Some(l) = solo.layers.get_mut(&id) {
        l.visible = true;
    }
    let visible_inputs = Inputs { project: &solo, preview: None, block_scales: inputs.block_scales.clone(), ..*inputs };
    let fields = neighbourhood_fields(&visible_inputs, &channels, tiles);

    let out: Vec<Vec<FlatTile>> = tiles
        .par_iter()
        .map(|&(tx, ty)| {
            let nodes = plan(&visible_inputs, &[id], &channels, tx, ty, 0, Some(&fields));
            let Some(n) = nodes.first() else { return Vec::new() };
            let full = anchor_plan(&visible_inputs, id, &channels, tx, ty, Some(&fields));
            let mut buffers: Vec<(Channel, Vec<u16>, bool)> =
                n.chans.iter().filter_map(|c| Channel::ALL.get(c.ch).map(|ch| (*ch, vec![0u16; TILE_AREA * (c.comps + 1)], false))).collect();
            for (ch, buffer, _) in &mut buffers {
                if ch.kind() == ChannelKind::Signed {
                    for pixel in buffer.as_chunks_mut::<2>().0 {
                        pixel[0] = to_u16_signed(0.0);
                    }
                }
            }

            for (x, y, o) in tile_texels(inputs.gbuf, tx, ty) {
                let mut tex = Texel::new(&visible_inputs, x, y, o, 0.0);
                if !full.is_empty() {
                    tex.anchors_below(&full, id);
                }
                let m = tex.node_mask(n);
                for (c, (ch, buffer, any)) in n.chans.iter().zip(buffers.iter_mut()) {
                    let Some((v, cov)) = tex.content(&n.content, c) else { continue };
                    let a = (cov * m).clamp(0.0, 1.0);
                    if a <= 0.0 {
                        continue;
                    }

                    *any = true;
                    let comps = c.comps;
                    let Some(pixel) = buffer.get_mut(o * (comps + 1)..(o + 1) * (comps + 1)) else { continue };
                    for (slot, pv) in pixel.iter_mut().zip(v.iter().take(comps)) {
                        *slot = if ch.kind() == ChannelKind::Signed { to_u16_signed(pv * a) } else { to_u16(pv * a) };
                    }
                    if let Some(slot) = pixel.get_mut(comps) {
                        *slot = to_u16(a);
                    }
                }
            }

            buffers.into_iter().filter(|(_, _, any)| *any).map(|(ch, buffer, _)| (ch, (tx, ty), buffer)).collect()
        })
        .collect();

    Some(out.into_iter().flatten().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use texturecraft_core::{ChannelBlend, Effect, Layer, Procedural, ProceduralKind, TextureSet};
    use texturecraft_geom::primitives;

    struct Fixture {
        project: Project,
        mesh: Mesh,
        gbuf: UvGBuffer,
        pixels: PixelStore,
        baked: BakedMaps,
    }

    fn fixture() -> Fixture {
        let mesh = primitives::plane(2.0, 2);
        let gbuf = UvGBuffer::new(&mesh, 0, 64, 64);
        let mut project = Project::new("t", "builtin:plane");
        project.texture_sets.push(TextureSet::new("Plane", 0, 64));
        Fixture { project, mesh, gbuf, pixels: PixelStore::default(), baked: BakedMaps::default() }
    }

    fn run(f: &Fixture, preview: Option<&StrokePreview>) -> Composite {
        let images = Default::default();
        let inp = Inputs {
            project: &f.project,
            ts: 0,
            mesh: &f.mesh,
            gbuf: &f.gbuf,
            pixels: &f.pixels,
            baked: &f.baked,
            block_scales: f.project.texture_sets[0].block_scales(),
            preview,
            images: &images,
            bounds: f.mesh.bounds(),
        };
        let mut out = Composite::new(&f.project.texture_sets[0].channel_list(), 64);
        composite_tiles(&inp, &mut out, &all_tiles(64, 64));
        out
    }

    fn fill_layer(p: &mut Project, color: [f32; 3], rough: f32) -> LayerId {
        let id = p.alloc_id();
        let mut fill = Fill::default();
        fill.sources.insert(Channel::BaseColor, Source::Uniform(color));
        fill.sources.insert(Channel::Roughness, Source::Uniform([rough; 3]));
        let l = Layer::new(id, "Fill", LayerKind::Fill(fill), &[Channel::BaseColor, Channel::Roughness]);
        p.insert_layer(0, l, None, false).unwrap()
    }

    fn close(a: [f32; 3], b: [f32; 3]) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() < 2e-4)
    }

    #[test]
    fn histogram_samples_spread_over_every_column() {
        assert_eq!(histogram_tiles(1024, 1024), all_tiles(1024, 1024), "small sets are read whole");
        for size in [2048, 4096, 8192] {
            let tiles = histogram_tiles(size, size);
            let columns: std::collections::BTreeSet<u32> = tiles.iter().map(|t| t.0).collect();
            assert!(tiles.len() <= HISTOGRAM_TILES, "{size}: {} tiles", tiles.len());
            assert!(columns.len() as u32 >= size / TILE / 2, "{size}: only {} columns", columns.len());
        }
    }

    #[test]
    fn empty_stack_gives_channel_defaults() {
        let f = fixture();
        let out = run(&f, None);
        assert!(close(out.value(Channel::BaseColor, 10, 10), [0.5; 3]));
        assert!(close(out.value(Channel::Roughness, 10, 10), [0.5; 3]));
        assert!(close(out.value(Channel::Normal, 10, 10), [0.5, 0.5, 1.0]));
    }

    #[test]
    fn fill_layers_blend_with_opacity_and_modes() {
        let mut f = fixture();
        fill_layer(&mut f.project, [1.0, 0.0, 0.0], 0.2);
        let top = fill_layer(&mut f.project, [0.0, 0.0, 1.0], 0.8);
        f.project.layer_mut(top).unwrap().channels.insert(Channel::BaseColor, ChannelBlend { mode: BlendMode::Normal, opacity: 0.25 });
        let out = run(&f, None);
        assert!(close(out.value(Channel::BaseColor, 5, 5), [0.75, 0.0, 0.25]));
        assert!(close(out.value(Channel::Roughness, 5, 5), [0.8; 3]));
        f.project.layer_mut(top).unwrap().channels.insert(Channel::BaseColor, ChannelBlend { mode: BlendMode::Multiply, opacity: 1.0 });
        let out = run(&f, None);
        assert!(close(out.value(Channel::BaseColor, 5, 5), [0.0, 0.0, 0.0]));
    }

    #[test]
    fn paint_layers_unpremultiply_and_preview_and_masks() {
        let mut f = fixture();
        let id = f.project.alloc_id();
        let l = Layer::new(id, "Paint", LayerKind::Paint, &[Channel::BaseColor]);
        f.project.insert_layer(0, l, None, false).unwrap();
        // Half-covered red texel: premul (0.5, 0, 0), coverage 0.5.
        let img = f.pixels.layer_mut(id, Channel::BaseColor, (64, 64));
        img.set(3, 3, &[to_u16(0.5), 0, 0, to_u16(0.5)]);
        let out = run(&f, None);
        assert!(close(out.value(Channel::BaseColor, 3, 3), [0.75, 0.25, 0.25]));
        assert!(close(out.value(Channel::BaseColor, 4, 3), [0.5; 3]));

        // A live stroke painting blue at full alpha on texel (4, 3).
        let mut alpha = TileImage::new(64, 64, 1);
        alpha.set(4, 3, &[to_u16(1.0)]);
        let pv = StrokePreview {
            target: PaintTarget::Layer(id),
            alpha,
            opacity: 1.0,
            erase: false,
            values: [(Channel::BaseColor, [0.0, 0.0, 1.0])].into_iter().collect(),
            mask_gray: MaskGray::Value(1.0),
            stencil: None,
            carried: BTreeMap::new(),
        };
        let out = run(&f, Some(&pv));
        assert!(close(out.value(Channel::BaseColor, 4, 3), [0.0, 0.0, 1.0]));

        // A black mask hides the layer.
        f.project.layer_mut(id).unwrap().mask = Some(Mask::new(MaskBackground::Black));
        let out = run(&f, None);
        assert!(close(out.value(Channel::BaseColor, 3, 3), [0.5; 3]));
    }

    #[test]
    fn mask_effects_and_procedural_fills_stay_in_range() {
        let mut f = fixture();
        let id = fill_layer(&mut f.project, [1.0, 1.0, 1.0], 1.0);
        let eid = f.project.alloc_id();
        let mut mask = Mask::new(MaskBackground::Black);
        let pattern = Procedural::new(ProceduralKind::Checker);
        mask.effects.push(Effect::new(
            eid,
            EffectKind::Fill {
                source: Source::Procedural { pattern, low: [0.0; 3], high: [1.0; 3] },
                projection: Projection::default(),
                sampling: Default::default(),
            },
        ));
        f.project.layer_mut(id).unwrap().mask = Some(mask);
        let out = run(&f, None);
        let vals: Vec<f32> = (0..64).map(|x| out.value(Channel::BaseColor, x, 20)[0]).collect();
        assert!(vals.iter().any(|v| *v > 0.99) && vals.iter().any(|v| *v < 0.51), "{vals:?}");
    }

    fn assert_sparse_mask_matches_image(fixture: &Fixture, layer: LayerId, texels: &[(u32, u32)]) -> Option<Vec<f32>> {
        let images = ImageStore::default();
        let inputs = Inputs {
            project: &fixture.project,
            ts: 0,
            mesh: &fixture.mesh,
            gbuf: &fixture.gbuf,
            pixels: &fixture.pixels,
            baked: &fixture.baked,
            block_scales: fixture.project.texture_sets[0].block_scales(),
            preview: None,
            images: &images,
            bounds: fixture.mesh.bounds(),
        };
        let sparse = mask_at(&inputs, layer, texels);
        let image = mask_image(&inputs, layer, fixture.gbuf.width);
        assert_eq!(sparse.is_some(), image.is_some());
        let values = sparse?;
        let image = image?;
        assert_eq!(values.len(), texels.len());

        for (&(x, y), &value) in texels.iter().zip(&values) {
            let expected =
                if x < fixture.gbuf.width && y < fixture.gbuf.height { f32::from(image[(y * fixture.gbuf.width + x) as usize]) / 255.0 } else { 0.0 };
            assert!((value - expected).abs() <= 0.5 / 255.0 + 1e-6, "({x}, {y}): {value} vs {expected}");
        }
        Some(values)
    }

    #[test]
    fn mask_at_reads_the_same_values_as_the_mask_image() {
        let mut fixture = fixture();
        fixture.gbuf = UvGBuffer::new(&fixture.mesh, 0, 128, 128);
        fixture.project.texture_sets[0].resolution = 128;
        fixture.gbuf.tri[30 * 128 + 90] = NO_TRI;
        let layer = fill_layer(&mut fixture.project, [1.0; 3], 1.0);
        let maskless = fill_layer(&mut fixture.project, [1.0; 3], 1.0);
        let effect = fixture.project.alloc_id();
        let mut mask = Mask::new(MaskBackground::Black);
        let pattern = Procedural::new(ProceduralKind::Checker);
        mask.effects.push(Effect::new(
            effect,
            EffectKind::Fill {
                source: Source::Procedural { pattern, low: [0.0; 3], high: [1.0; 3] },
                projection: Projection::default(),
                sampling: Default::default(),
            },
        ));
        fixture.project.layer_mut(layer).unwrap().mask = Some(mask);

        // Cross tile boundaries and include duplicates, a UV hole and invalid coordinates
        let texels = [(0, 0), (0, 40), (63, 63), (64, 64), (127, 127), (0, 40), (90, 30), (128, 0), (0, 128), (u32::MAX, u32::MAX)];
        let values = assert_sparse_mask_matches_image(&fixture, layer, &texels).unwrap();
        assert!(values.iter().any(|value| *value > 0.99) && values.iter().take(6).any(|value| *value < 0.01));
        assert_eq!(&values[6..], &[0.0; 4]);
        assert_eq!(assert_sparse_mask_matches_image(&fixture, layer, &[]), Some(Vec::new()));
        assert!(assert_sparse_mask_matches_image(&fixture, maskless, &texels).is_none());
        let missing = fixture.project.alloc_id();
        assert!(assert_sparse_mask_matches_image(&fixture, missing, &texels).is_none());
        fixture.project.layer_mut(layer).unwrap().mask.as_mut().unwrap().enabled = false;
        assert!(assert_sparse_mask_matches_image(&fixture, layer, &texels).is_none());
    }

    #[test]
    fn mask_at_ignores_out_of_range_texels_before_resampling_paint() {
        let mut fixture = fixture();
        let layer = fill_layer(&mut fixture.project, [1.0; 3], 1.0);
        let paint = fixture.project.alloc_id();
        let mut mask = Mask::new(MaskBackground::Black);
        mask.effects.push(Effect::new(paint, EffectKind::Paint));
        fixture.project.layer_mut(layer).unwrap().mask = Some(mask);
        fixture.pixels.mask_mut(paint, (32, 32)).set(4, 5, &[u16::MAX; 2]);

        let texels = [(8, 10), (u32::MAX, 0), (0, u32::MAX), (u32::MAX, u32::MAX), (64, 0), (0, 64)];
        let values = assert_sparse_mask_matches_image(&fixture, layer, &texels).unwrap();
        assert!(values[0] > 0.5, "valid texels still read resampled paint");
        assert_eq!(&values[1..], &[0.0; 5]);
        assert_eq!(assert_sparse_mask_matches_image(&fixture, layer, &texels[1..]), Some(vec![0.0; 5]));
    }

    #[test]
    fn mask_at_preserves_anchors_and_neighbourhood_filters_across_tiles() {
        let mut fixture = fixture();
        fixture.gbuf = UvGBuffer::new(&fixture.mesh, 0, 128, 128);
        fixture.project.texture_sets[0].resolution = 128;
        let lower = fill_layer(&mut fixture.project, [0.0; 3], 1.0);
        let LayerKind::Fill(fill) = &mut fixture.project.layer_mut(lower).unwrap().kind else { panic!("expected a fill layer") };

        fill.sources.insert(Channel::BaseColor, Source::Procedural { pattern: Procedural::new(ProceduralKind::Checker), low: [0.0; 3], high: [1.0; 3] });
        let anchor = content_fx(&mut fixture, lower, EffectKind::Anchor);
        let upper = fill_layer(&mut fixture.project, [1.0; 3], 1.0);
        let mut mask = Mask::new(MaskBackground::Black);
        mask.effects.push(Effect::new(
            fixture.project.alloc_id(),
            EffectKind::Fill {
                source: Source::Anchor { anchor, channel: Channel::BaseColor },
                projection: Projection::default(),
                sampling: Default::default(),
            },
        ));
        fixture.project.layer_mut(upper).unwrap().mask = Some(mask.clone());
        let texels: Vec<_> = (0..128).map(|x| (x, 63)).collect();
        let original = assert_sparse_mask_matches_image(&fixture, upper, &texels).unwrap();
        assert!(original.iter().any(|value| *value > 0.99) && original.iter().any(|value| *value < 0.01), "the anchor supplies the checker");

        for filter in [Filter::Blur { radius: 64.0, wrap: false }, Filter::DirectionalWarp { intensity: 256.0, angle: 90.0, scale: 4.0, seed: 3, divider: 1 }] {
            let mut filtered = mask.clone();
            filtered.effects.push(Effect::new(fixture.project.alloc_id(), EffectKind::Filter(filter)));
            fixture.project.layer_mut(upper).unwrap().mask = Some(filtered);
            let values = assert_sparse_mask_matches_image(&fixture, upper, &texels).unwrap();
            assert!(values.iter().zip(&original).any(|(value, original)| (value - original).abs() > 0.1), "the filter changes the anchor values");
        }
    }

    #[test]
    fn a_mask_runs_every_neighbourhood_filter_in_order() {
        let mut fixture = fixture();
        fixture.gbuf = UvGBuffer::new(&fixture.mesh, 0, 128, 128);
        fixture.project.texture_sets[0].resolution = 128;
        let layer = fill_layer(&mut fixture.project, [1.0; 3], 1.0);
        let checker = Source::Procedural { pattern: Procedural::new(ProceduralKind::Checker), low: [0.0; 3], high: [1.0; 3] };
        let warp = Filter::DirectionalWarp { intensity: 256.0, angle: 90.0, scale: 4.0, seed: 3, divider: 1 };
        let texels: Vec<_> = (0..128).map(|x| (x, 63)).collect();
        let mut masked = |filters: &[Filter]| {
            let mut mask = Mask::new(MaskBackground::Black);
            mask.effects.push(Effect::new(
                fixture.project.alloc_id(),
                EffectKind::Fill { source: checker.clone(), projection: Projection::default(), sampling: Default::default() },
            ));
            for filter in filters {
                mask.effects.push(Effect::new(fixture.project.alloc_id(), EffectKind::Filter(filter.clone())));
            }
            fixture.project.layer_mut(layer).unwrap().mask = Some(mask);
            assert_sparse_mask_matches_image(&fixture, layer, &texels).unwrap()
        };

        let warped = masked(std::slice::from_ref(&warp));
        let blurred_then_warped = masked(&[Filter::Blur { radius: 64.0, wrap: false }, warp]);
        assert!(blurred_then_warped.iter().zip(&warped).any(|(chained, single)| (chained - single).abs() > 0.1), "the blur under the warp still runs");
        assert!(blurred_then_warped.iter().any(|value| *value > 0.05 && *value < 0.95), "the blur softens the checker before the warp moves it");
    }

    #[test]
    fn a_content_stack_runs_every_neighbourhood_filter_in_order() {
        let mut fixture = fixture();
        let layer = fill_layer(&mut fixture.project, [0.0; 3], 1.0);
        let LayerKind::Fill(fill) = &mut fixture.project.layer_mut(layer).unwrap().kind else { panic!("expected a fill layer") };

        fill.sources.insert(Channel::BaseColor, Source::Procedural { pattern: Procedural::new(ProceduralKind::Checker), low: [0.0; 3], high: [1.0; 3] });
        let warp = Filter::DirectionalWarp { intensity: 256.0, angle: 90.0, scale: 4.0, seed: 3, divider: 1 };
        let warp_effect = content_fx(&mut fixture, layer, EffectKind::Filter(warp));
        let row = |fixture: &Fixture| -> Vec<f32> {
            let composite = run(fixture, None);
            (0..64).map(|x| composite.value(Channel::BaseColor, x, 20)[0]).collect()
        };
        let warped = row(&fixture);

        content_fx(&mut fixture, layer, EffectKind::Filter(Filter::Blur { radius: 64.0, wrap: false }));
        let effects = &mut fixture.project.layer_mut(layer).unwrap().content_effects;
        let warp_index = effects.iter().position(|effect| effect.id == warp_effect).unwrap();
        let warp_on_top = effects.remove(warp_index);
        effects.push(warp_on_top);
        let blurred_then_warped = row(&fixture);
        assert!(blurred_then_warped.iter().zip(&warped).any(|(chained, single)| (chained - single).abs() > 0.1), "the blur under the warp still runs");
    }

    #[test]
    fn larger_curvature_detail_sizes_reach_further_from_an_edge() {
        let mut fixture = fixture();
        fixture.gbuf = UvGBuffer::new(&fixture.mesh, 0, 256, 256);
        fixture.project.texture_sets[0].resolution = 256;
        let ridge: Vec<f32> = (0..256 * 256).map(|texel| if texel % 256 == 128 { 1.0 } else { 0.5 }).collect();
        fixture.baked = BakedMaps { resolution: 256, maps: [(BakedMap::Curvature, Arc::new(ridge))].into() };
        let layer = fill_layer(&mut fixture.project, [1.0; 3], 1.0);
        let row_with = |fixture: &mut Fixture, weights: [f32; 7]| -> Vec<f32> {
            let detail = CurvatureDetail { mode: CurvatureMode::Edges, weights };
            let generator = Generator { level: 0.5, softness: 0.5, noise_amount: 0.0, detail: Some(detail), ..Generator::new(GeneratorKind::Curvature) };
            let mut mask = Mask::new(MaskBackground::Black);
            mask.effects.push(Effect::new(fixture.project.alloc_id(), EffectKind::Generator(generator)));
            fixture.project.layer_mut(layer).unwrap().mask = Some(mask);
            let images = Default::default();
            let inputs = Inputs {
                project: &fixture.project,
                ts: 0,
                mesh: &fixture.mesh,
                gbuf: &fixture.gbuf,
                pixels: &fixture.pixels,
                baked: &fixture.baked,
                block_scales: BlockScales::default(),
                preview: None,
                images: &images,
                bounds: fixture.mesh.bounds(),
            };
            mask_at(&inputs, layer, &(120..137).map(|x| (x, 128)).collect::<Vec<_>>()).unwrap()
        };

        // The row starts at x 120: the ridge is its ninth texel, five texels left of it the fourth
        let (ridge, five_texels_away) = (8, 3);
        let finest = row_with(&mut fixture, [1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
        let largest = row_with(&mut fixture, [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0]);
        assert!(finest[ridge] > 0.99 && finest[five_texels_away] < 0.01, "the map itself marks only the ridge: {finest:?}");
        assert!(largest[ridge] > 0.99 && largest[five_texels_away] > 0.5, "the largest size reaches texels away from it: {largest:?}");
    }

    #[test]
    fn curvature_detail_noise_keeps_flat_areas_neutral() {
        let mut fixture = fixture();
        fixture.baked = BakedMaps { resolution: 64, maps: [(BakedMap::Curvature, Arc::new(vec![0.5; 64 * 64]))].into() };
        let layer = fill_layer(&mut fixture.project, [1.0; 3], 1.0);
        let texels: Vec<_> = (0..64).map(|x| (x, 32)).collect();

        for kind in [GeneratorKind::Curvature, GeneratorKind::EdgeWear] {
            for mode in CurvatureMode::ALL {
                let detail = CurvatureDetail { mode, weights: [1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0] };
                let generator = Generator { level: 0.5, softness: 0.5, noise_amount: 2.0, detail: Some(detail), ..Generator::new(kind) };
                let mut mask = Mask::new(MaskBackground::Black);
                mask.effects.push(Effect::new(fixture.project.alloc_id(), EffectKind::Generator(generator)));
                fixture.project.layer_mut(layer).unwrap().mask = Some(mask);

                let values = assert_sparse_mask_matches_image(&fixture, layer, &texels).unwrap();
                let expected = if mode == CurvatureMode::Raw { 0.5 } else { 0.0 };
                assert!(values.iter().all(|value| (value - expected).abs() < 0.001), "{kind:?} {mode:?} paints noise on flat curvature: {values:?}");
            }
        }
    }

    #[test]
    fn raw_curvature_reads_mid_gray_without_a_curvature_map() {
        let mut fixture = fixture();
        let layer = fill_layer(&mut fixture.project, [1.0; 3], 1.0);
        let detail = CurvatureDetail { mode: CurvatureMode::Raw, ..CurvatureDetail::default() };
        let generator = Generator { level: 0.5, softness: 0.5, noise_amount: 0.0, detail: Some(detail), ..Generator::new(GeneratorKind::Curvature) };
        let mut mask = Mask::new(MaskBackground::Black);
        mask.effects.push(Effect::new(fixture.project.alloc_id(), EffectKind::Generator(generator)));
        fixture.project.layer_mut(layer).unwrap().mask = Some(mask);

        let values = assert_sparse_mask_matches_image(&fixture, layer, &[(32, 32)]).unwrap();
        assert!(values.iter().all(|value| (value - 0.5).abs() < 0.01), "flat curvature is mid gray in raw mode, not black: {values:?}");
    }

    #[test]
    fn blur_filter_softens_mask_edges() {
        let mut f = fixture();
        let id = fill_layer(&mut f.project, [1.0, 1.0, 1.0], 1.0);
        let mut mask = Mask::new(MaskBackground::Black);
        let pattern = Procedural::new(ProceduralKind::Checker);
        let (e1, e2) = (f.project.alloc_id(), f.project.alloc_id());
        mask.effects.push(Effect::new(
            e1,
            EffectKind::Fill {
                source: Source::Procedural { pattern, low: [0.0; 3], high: [1.0; 3] },
                projection: Projection::default(),
                sampling: Default::default(),
            },
        ));
        f.project.layer_mut(id).unwrap().mask = Some(mask.clone());
        let hard: Vec<f32> = (0..64).map(|x| run(&f, None).value(Channel::BaseColor, x, 20)[0]).collect();
        mask.effects.push(Effect::new(e2, EffectKind::Filter(Filter::Blur { radius: 64.0, wrap: false })));
        f.project.layer_mut(id).unwrap().mask = Some(mask);
        let soft_img = run(&f, None);
        let soft: Vec<f32> = (0..64).map(|x| soft_img.value(Channel::BaseColor, x, 20)[0]).collect();
        let mids = |v: &[f32]| v.iter().filter(|x| **x > 0.55 && **x < 0.95).count();
        assert!(
            mids(&soft) > mids(&hard) + 4,
            "hard {hard:?}
soft {soft:?}"
        );
    }

    #[test]
    fn groups_pass_through_with_opacity() {
        let mut f = fixture();
        let gid = f.project.alloc_id();
        let g = Layer::new(
            gid,
            "G",
            LayerKind::Group { children: vec![], collapsed: false, contents: Default::default(), randomized: Vec::new() },
            &[Channel::BaseColor],
        );
        f.project.insert_layer(0, g, None, false).unwrap();
        f.project.layer_mut(gid).unwrap().channels.insert(Channel::BaseColor, ChannelBlend { mode: BlendMode::Passthrough, opacity: 0.5 });
        let cid = f.project.alloc_id();
        let mut fill = Fill::default();
        fill.sources.insert(Channel::BaseColor, Source::Uniform([1.0, 1.0, 1.0]));
        let child = Layer::new(cid, "C", LayerKind::Fill(fill), &[Channel::BaseColor]);
        f.project.insert_layer(0, child, Some(gid), true).unwrap();
        let out = run(&f, None);
        assert!(close(out.value(Channel::BaseColor, 1, 1), [0.75; 3]));
    }

    #[test]
    fn padding_copies_nearest_covered_value() {
        let mut out = Composite::new(&[Channel::Roughness], 64);
        out.channels.get_mut(&Channel::Roughness).unwrap().set(10, 10, &[to_u16(0.9)]);
        let mut covered = vec![false; 64 * 64];
        covered[10 * 64 + 10] = true;
        let dmap = DilationMap::new(&covered, 64, 64, Some(4));
        pad_tiles(&mut out, &dmap, &[(0, 0)]);
        assert!((out.value(Channel::Roughness, 12, 12)[0] - 0.9).abs() < 1e-3);
        assert!(out.value(Channel::Roughness, 30, 30)[0] < 0.1 || !out.channels[&Channel::Roughness].is_allocated(0, 0));
    }

    fn content_fx(f: &mut Fixture, id: LayerId, kind: EffectKind) -> LayerId {
        let eid = f.project.alloc_id();
        f.project.layer_mut(id).unwrap().content_effects.push(Effect::new(eid, kind));
        eid
    }

    #[test]
    fn content_filters_hsl_and_opacity_change_layer_values() {
        let mut f = fixture();
        let id = fill_layer(&mut f.project, [1.0, 0.0, 0.0], 0.2);
        content_fx(&mut f, id, EffectKind::Filter(Filter::Hsl { hue: 1.0 / 3.0, saturation: 0.0, lightness: 0.0, perceptive: false }));
        let out = run(&f, None);
        assert!(close(out.value(Channel::BaseColor, 5, 5), [0.0, 1.0, 0.0]), "{:?}", out.value(Channel::BaseColor, 5, 5));
        assert!(close(out.value(Channel::Roughness, 5, 5), [0.2; 3]), "a hue shift leaves scalars alone");
        content_fx(&mut f, id, EffectKind::Filter(Filter::Invert));
        let out = run(&f, None);
        assert!(close(out.value(Channel::BaseColor, 5, 5), [1.0, 0.0, 1.0]));
        assert!(close(out.value(Channel::Roughness, 5, 5), [0.8; 3]));
        // Hidden effects are skipped and opacity fades an effect.
        let fx = &mut f.project.layer_mut(id).unwrap().content_effects;
        fx[0].visible = false;
        fx[1].opacity = 0.5;
        let out = run(&f, None);
        assert!(close(out.value(Channel::BaseColor, 5, 5), [0.5; 3]));
        assert!(close(out.value(Channel::Roughness, 5, 5), [0.5; 3]));
    }

    /// Posterize steps a paint layer's coverage only with Apply to Alpha
    #[test]
    fn posterize_steps_the_coverage_when_it_applies_to_alpha() {
        let mut f = fixture();
        let id = f.project.alloc_id();
        let l = Layer::new(id, "Paint", LayerKind::Paint, &[Channel::BaseColor]);
        f.project.insert_layer(0, l, None, false).unwrap();

        // Half-covered red over the default mid gray
        f.pixels.layer_mut(id, Channel::BaseColor, (64, 64)).set(3, 3, &[to_u16(0.5), 0, 0, to_u16(0.5)]);
        let posterize = content_fx(&mut f, id, EffectKind::Filter(Filter::Posterize { levels: 2.0, alpha: false }));
        assert!(close(run(&f, None).value(Channel::BaseColor, 3, 3), [0.75, 0.25, 0.25]), "the coverage stays as it is");
        f.project.effect_mut(posterize).unwrap().kind = EffectKind::Filter(Filter::Posterize { levels: 2.0, alpha: true });
        assert!(close(run(&f, None).value(Channel::BaseColor, 3, 3), [1.0, 0.0, 0.0]), "half coverage rounds up to whole");
    }

    /// A paint layer with base color texels set by `paint` (x, y -> Some(linear color, coverage))
    fn painted(f: &mut Fixture, paint: impl Fn(u32, u32) -> Option<([f32; 3], f32)>) -> LayerId {
        let id = f.project.alloc_id();
        let l = Layer::new(id, "Paint", LayerKind::Paint, &[Channel::BaseColor]);
        f.project.insert_layer(0, l, None, false).unwrap();
        let tile = f.pixels.layer_mut(id, Channel::BaseColor, (64, 64));
        for (x, y) in (0..64).flat_map(|y| (0..64).map(move |x| (x, y))) {
            if let Some((color, cover)) = paint(x, y) {
                tile.set(x, y, &[to_u16(color[0] * cover), to_u16(color[1] * cover), to_u16(color[2] * cover), to_u16(cover)]);
            }
        }
        id
    }

    /// Pixelate steps the coverage into its cells only with Apply To Alpha; off, each texel keeps its own
    #[test]
    fn pixelate_keeps_each_texels_coverage_without_apply_to_alpha() {
        let mut f = fixture();

        // One red texel in a cell of 4 × 4 texels (128 texels at 2K at 64 across), whose center reads it a quarter
        let id = painted(&mut f, |x, y| ((x, y) == (1, 1)).then_some(([1.0, 0.0, 0.0], 1.0)));
        let fx = content_fx(&mut f, id, EffectKind::Filter(Filter::Mosaic { size: 128.0, alpha: true }));
        let out = run(&f, None);
        assert!(close(out.value(Channel::BaseColor, 1, 1), [0.625, 0.375, 0.375]), "the cell's coverage: {:?}", out.value(Channel::BaseColor, 1, 1));
        assert!(close(out.value(Channel::BaseColor, 2, 2), [0.625, 0.375, 0.375]), "spread over the cell");
        f.project.effect_mut(fx).unwrap().kind = EffectKind::Filter(Filter::Mosaic { size: 128.0, alpha: false });
        let out = run(&f, None);
        assert!(close(out.value(Channel::BaseColor, 1, 1), [1.0, 0.0, 0.0]), "the texel keeps its whole coverage");
        assert!(close(out.value(Channel::BaseColor, 2, 2), [0.5; 3]), "an uncovered texel stays uncovered");
    }

    /// FXAA blends a stair-stepped edge and leaves flat areas; the coverage only with Affect Alpha Channel
    #[test]
    fn fxaa_smooths_stair_steps() {
        let stairs = |x: u32, y: u32| x >= 20 + y / 4;
        let mids = |out: &Composite| (0..64 * 64).filter(|i| (0.05..0.95).contains(&out.value(Channel::BaseColor, i % 64, i / 64)[0])).count();

        // Black and white, both covered: the edge gets in-between values
        let mut f = fixture();
        let id = painted(&mut f, |x, y| Some((if stairs(x, y) { [1.0; 3] } else { [0.0; 3] }, 1.0)));
        assert_eq!(mids(&run(&f, None)), 0);
        content_fx(&mut f, id, EffectKind::Filter(Filter::Fxaa { alpha: false }));
        let out = run(&f, None);
        assert!(mids(&out) > 16, "{} texels blended", mids(&out));
        assert!(close(out.value(Channel::BaseColor, 2, 30), [0.0; 3]) && close(out.value(Channel::BaseColor, 60, 30), [1.0; 3]), "flat areas stay");

        // White shapes on nothing: the coverage changes only with Affect Alpha Channel
        let mut f = fixture();
        let id = painted(&mut f, |x, y| stairs(x, y).then_some(([1.0; 3], 1.0)));
        let fx = content_fx(&mut f, id, EffectKind::Filter(Filter::Fxaa { alpha: false }));
        let plain = |out: &Composite| (0..64 * 64).all(|i| [0.5, 1.0].iter().any(|v| (out.value(Channel::BaseColor, i % 64, i / 64)[0] - v).abs() < 2e-4));
        assert!(plain(&run(&f, None)), "each texel keeps its coverage");
        f.project.effect_mut(fx).unwrap().kind = EffectKind::Filter(Filter::Fxaa { alpha: true });
        assert!(!plain(&run(&f, None)), "the coverage is smoothed");
    }

    #[test]
    fn hsl_on_a_mask_changes_its_lightness() {
        let mut f = fixture();
        let id = fill_layer(&mut f.project, [1.0; 3], 1.0);
        let mut mask = Mask::new(MaskBackground::White);
        mask.effects.push(Effect::new(f.project.alloc_id(), EffectKind::Filter(Filter::Hsl { hue: 0.3, saturation: 1.0, lightness: -0.5, perceptive: false })));
        f.project.layer_mut(id).unwrap().mask = Some(mask);
        assert!(close(run(&f, None).value(Channel::BaseColor, 5, 5), [0.75; 3]));
    }

    #[test]
    fn content_blur_softens_fill_edges() {
        let mut f = fixture();
        let id = f.project.alloc_id();
        let mut fill = Fill::default();
        fill.sources.insert(Channel::Roughness, Source::Procedural { pattern: Procedural::new(ProceduralKind::Checker), low: [0.0; 3], high: [1.0; 3] });
        f.project.insert_layer(0, Layer::new(id, "F", LayerKind::Fill(fill), &[Channel::Roughness]), None, false).unwrap();
        let row = |f: &Fixture| {
            let out = run(f, None);
            (0..64).map(|x| out.value(Channel::Roughness, x, 20)[0]).collect::<Vec<f32>>()
        };
        let hard = row(&f);
        content_fx(&mut f, id, EffectKind::Filter(Filter::Blur { radius: 64.0, wrap: false }));
        let soft = row(&f);
        let mids = |v: &[f32]| v.iter().filter(|x| **x > 0.05 && **x < 0.95).count();
        assert!(mids(&soft) > mids(&hard) + 4, "hard {hard:?}\nsoft {soft:?}");
        assert!(soft.iter().all(|v| (0.0..=1.0).contains(v)));
    }

    #[test]
    fn a_wrapping_blur_reads_the_far_side_of_the_texture() {
        let row = |wrap: Option<bool>| {
            let mut f = fixture();
            let id = f.project.alloc_id();
            let mut fill = Fill::default();
            fill.sources.insert(Channel::Roughness, Source::Procedural { pattern: Procedural::new(ProceduralKind::Checker), low: [0.0; 3], high: [1.0; 3] });
            f.project.insert_layer(0, Layer::new(id, "F", LayerKind::Fill(fill), &[Channel::Roughness]), None, false).unwrap();
            if let Some(wrap) = wrap {
                content_fx(&mut f, id, EffectKind::Filter(Filter::Blur { radius: 96.0, wrap }));
            }
            let out = run(&f, None);
            (0..64).map(|x| out.value(Channel::Roughness, x, 20)[0]).collect::<Vec<f32>>()
        };

        // The checker tiles: its period P repeats across the whole row
        let hard = row(None);
        let period = (1..64).find(|&p| (0..64 - p).all(|x| (hard[x] - hard[x + p]).abs() < 1e-3)).unwrap();
        assert!(period <= 32, "{hard:?}");

        // Wrapped, the blur at the texture's edge sees the far side, so the result keeps tiling; clipped, the edge differs
        let wrapped = row(Some(true));
        assert!((0..64 - period).all(|x| (wrapped[x] - wrapped[x + period]).abs() < 0.02), "{wrapped:?}");
        let clipped = row(Some(false));
        assert!((clipped[0] - clipped[period]).abs() > 0.05, "{clipped:?}");
        assert!((12..52).all(|x| (clipped[x] - wrapped[x]).abs() < 0.02), "away from the edges, wrap changes nothing");
    }

    #[test]
    fn a_gradients_blur_intensity_blurs_what_it_looks_up() {
        let row = |filter: Option<Filter>| {
            let mut f = fixture();
            let id = f.project.alloc_id();
            let mut fill = Fill::default();
            fill.sources.insert(Channel::Roughness, Source::Procedural { pattern: Procedural::new(ProceduralKind::Checker), low: [0.0; 3], high: [1.0; 3] });
            f.project.insert_layer(0, Layer::new(id, "F", LayerKind::Fill(fill), &[Channel::Roughness]), None, false).unwrap();
            if let Some(filter) = filter {
                content_fx(&mut f, id, EffectKind::Filter(filter));
            }
            let out = run(&f, None);
            (16..48).map(|x| out.value(Channel::Roughness, x, 20)[0]).collect::<Vec<f32>>()
        };
        let mut gradient = Filter::even_gradient(vec![[0.0; 3], [1.0; 3]]);
        let hard = row(Some(gradient.clone()));
        assert!(hard.iter().zip(row(None)).all(|(a, b)| (a - b).abs() < 1e-3), "a black to white ramp keeps a scalar as it is");

        // Blurred first, then looked up: the same as a blur of that size
        if let Filter::GradientMap { input, .. } = &mut gradient {
            input.blur = 8.0;
        }
        let blurred = row(Some(gradient));
        let reference = row(Some(Filter::Blur { radius: 8.0, wrap: false }));
        assert!(blurred.iter().zip(&reference).all(|(a, b)| (a - b).abs() < 0.02), "{blurred:?} vs {reference:?}");
        assert!(blurred.iter().zip(&hard).any(|(a, b)| (a - b).abs() > 0.1), "{blurred:?}");
    }

    #[test]
    fn highpass_works_in_its_color_space_and_applies_to_alpha() {
        // Uniform content has no detail: mid gray in the working space, sRGB mid gray (linear 0.214) in sRGB
        let mut f = fixture();
        let below = fill_layer(&mut f.project, [0.0; 3], 0.0);
        let id = fill_layer(&mut f.project, [1.0, 0.0, 0.0], 0.2);
        assert_ne!(below, id);
        let hp = content_fx(&mut f, id, EffectKind::Filter(Filter::HighPass { radius: 64.0, space: HighpassSpace::SrgbFromLinear, alpha: false }));
        let out = run(&f, None);
        assert!(close(out.value(Channel::BaseColor, 30, 30), [texturecraft_texel::srgb_to_linear(0.5); 3]), "{:?}", out.value(Channel::BaseColor, 30, 30));
        assert!(close(out.value(Channel::Roughness, 30, 30), [0.5; 3]), "scalar channels stay in the working space");

        // Apply To Alpha: full coverage minus its blur, around 0.5, lays the layer at half over the black one below
        f.project.effect_mut(hp).unwrap().kind = EffectKind::Filter(Filter::HighPass { radius: 64.0, space: HighpassSpace::Working, alpha: true });
        let out = run(&f, None);
        assert!(close(out.value(Channel::Roughness, 30, 30), [0.25; 3]), "{:?}", out.value(Channel::Roughness, 30, 30));
    }

    #[test]
    fn high_pass_edge_detect_directional_warp_grayscale_and_normal_invert() {
        // Uniform content: high pass is mid gray (0 for height), edge detect black.
        let mut f = fixture();
        let id = fill_layer(&mut f.project, [1.0, 0.0, 0.0], 0.2);
        let hp = content_fx(&mut f, id, EffectKind::Filter(Filter::HighPass { radius: 64.0, space: HighpassSpace::Working, alpha: false }));
        let out = run(&f, None);
        assert!(close(out.value(Channel::BaseColor, 30, 30), [0.5; 3]), "{:?}", out.value(Channel::BaseColor, 30, 30));
        assert!(close(out.value(Channel::Roughness, 30, 30), [0.5; 3]));
        f.project.layer_mut(id).unwrap().content_effects.retain(|e| e.id != hp);
        content_fx(&mut f, id, EffectKind::Filter(Filter::EdgeDetect { width: 64.0 }));
        assert!(close(run(&f, None).value(Channel::Roughness, 30, 30), [0.0; 3]));
        // A checker: its borders light up, the middle of its cells stays black.
        let mut f = fixture();
        let id = f.project.alloc_id();
        let mut fill = Fill::default();
        fill.sources.insert(Channel::Roughness, Source::Procedural { pattern: Procedural::new(ProceduralKind::Checker), low: [0.0; 3], high: [1.0; 3] });
        f.project.insert_layer(0, Layer::new(id, "F", LayerKind::Fill(fill), &[Channel::Roughness]), None, false).unwrap();
        let plain = run(&f, None);
        let edges = content_fx(&mut f, id, EffectKind::Filter(Filter::EdgeDetect { width: 32.0 }));
        let out = run(&f, None);
        let vals: Vec<f32> = (0..64).flat_map(|y| (0..64).map(move |x| (x, y))).map(|(x, y)| out.value(Channel::Roughness, x, y)[0]).collect();
        assert!(vals.iter().all(|v| (0.0..=1.0).contains(v)));
        assert!(vals.iter().filter(|v| **v > 0.5).count() > 64 && vals.iter().filter(|v| **v < 0.01).count() > 64, "{vals:?}");
        f.project.layer_mut(id).unwrap().content_effects.retain(|e| e.id != edges);
        content_fx(&mut f, id, EffectKind::Filter(Filter::DirectionalWarp { intensity: 256.0, angle: 90.0, scale: 4.0, seed: 3, divider: 1 }));
        let warped = run(&f, None);
        let differs = (0..64).any(|y| (0..64).any(|x| (warped.value(Channel::Roughness, x, y)[0] - plain.value(Channel::Roughness, x, y)[0]).abs() > 0.2));
        assert!(differs, "the warp moves the checker");
        // Point filters.
        assert!(close(Filter::Grayscale.point(ChannelKind::Color, [1.0, 0.0, 0.0]).unwrap(), [lum([1.0, 0.0, 0.0]); 3]));
        assert_eq!(Filter::Grayscale.point(ChannelKind::Scalar, [0.3; 3]), Some([0.3; 3]));
        assert_eq!(Filter::NormalInvert.point(ChannelKind::NormalMap, [0.5, 0.25, 1.0]), Some([0.5, 0.75, 1.0]));
        assert_eq!(Filter::NormalInvert.point(ChannelKind::Color, [0.5, 0.25, 1.0]), Some([0.5, 0.25, 1.0]));
        // Replace color: near colors shift (keeping their offset), far ones and scalars stay.
        let rc = Filter::ReplaceColor { from: [0.8, 0.1, 0.1], to: [0.1, 0.1, 0.8], tolerance: 0.1, softness: 0.1 };
        assert!(close(rc.point(ChannelKind::Color, [0.85, 0.1, 0.1]).unwrap(), [0.15, 0.1, 0.8]));
        assert!(close(rc.point(ChannelKind::Color, [0.1, 0.8, 0.1]).unwrap(), [0.1, 0.8, 0.1]));
        let half = rc.point(ChannelKind::Color, [0.8, 0.1 + 0.15 * 3f32.sqrt(), 0.1]).unwrap();
        assert!((half[2] - 0.45).abs() < 0.02, "halfway through the softness: {half:?}");
        assert_eq!(rc.point(ChannelKind::Scalar, [0.8; 3]), Some([0.8; 3]));
    }

    #[test]
    fn a_halo_glow_lights_around_a_masks_shapes_and_a_drop_shadow_lands_under_them_away_from_the_light() {
        let row_of = |filter: Option<Filter>| {
            let mut f = fixture();
            let id = fill_layer(&mut f.project, [1.0; 3], 1.0);
            let mut mask = Mask::new(MaskBackground::Black);
            let fill = EffectKind::Fill {
                source: Source::Procedural { pattern: Procedural::new(ProceduralKind::Checker), low: [0.0; 3], high: [1.0; 3] },
                projection: Projection::default(),
                sampling: Default::default(),
            };
            mask.effects.push(Effect::new(f.project.alloc_id(), fill));
            if let Some(filter) = filter {
                mask.effects.push(Effect::new(f.project.alloc_id(), EffectKind::Filter(filter)));
            }
            f.project.layer_mut(id).unwrap().mask = Some(mask);
            let out = run(&f, None);
            (0..64 * 64).map(|i| out.value(Channel::BaseColor, i % 64, i / 64)[0]).collect::<Vec<f32>>()
        };
        let plain = row_of(None);
        let (low, high) = plain.iter().fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(*v), hi.max(*v)));
        let dark = |v: f32| v < low + 0.01;

        // A glow only adds, and lights the dark texels beside the shapes
        let glow = Filter::HaloGlow { on_mask: true, precise: false, width: 1.0, spread: 0.0, opacity: 1.0, color: [1.0; 3] };
        let glowing = row_of(Some(glow));
        assert!(glowing.iter().zip(&plain).all(|(g, p)| *g >= p - 1e-4), "a glow only adds");
        let lit = (0..64 * 64).filter(|&i| dark(plain[i]) && glowing[i] > low + 0.1 * (high - low)).count();
        assert!(lit > 64, "{lit} dark texels lit");

        // A shadow from a light above lands below the shapes: dark texels under a shape take it, those above one do not
        let shadow = Filter::DropShadow { angle: 90.0, distance: 0.1, size: 0.0, spread: 0.0, opacity: 0.5, color: [0.0; 3] };
        let shadowed = row_of(Some(shadow));
        assert!(shadowed.iter().zip(&plain).all(|(s, p)| *s >= p - 1e-4), "a shadow only goes under the shapes");
        let reach = 6;
        let at = |x: usize, y: usize| plain[y * 64 + x];
        let (mut below, mut above) = (Vec::new(), Vec::new());
        for y in 8..56 {
            for x in 0..64 {
                if dark(at(x, y)) && !dark(at(x, y - reach)) && (y - 2..=y).all(|row| dark(at(x, row))) {
                    below.push(shadowed[y * 64 + x] - at(x, y));
                }
                if dark(at(x, y)) && !dark(at(x, y + reach)) && dark(at(x, y - reach)) && (y..=y + 2).all(|row| dark(at(x, row))) {
                    above.push(shadowed[y * 64 + x] - at(x, y));
                }
            }
        }
        assert!(!below.is_empty() && !above.is_empty());
        let mean = |v: &[f32]| v.iter().sum::<f32>() / v.len() as f32;
        assert!(mean(&below) > 0.2 * (high - low), "under: {below:?}");
        assert!(mean(&above) < 0.05 * (high - low), "above: {above:?}");
    }

    #[test]
    fn bevel_glow_and_mosaic() {
        let checker = |f: &mut Fixture| {
            let id = f.project.alloc_id();
            let mut fill = Fill::default();
            fill.sources.insert(Channel::Roughness, Source::Procedural { pattern: Procedural::new(ProceduralKind::Checker), low: [0.0; 3], high: [1.0; 3] });
            f.project.insert_layer(0, Layer::new(id, "F", LayerKind::Fill(fill), &[Channel::Roughness]), None, false).unwrap();
            id
        };
        let values = |f: &Fixture| {
            let out = run(f, None);
            (0..64 * 64).map(|i| out.value(Channel::Roughness, i % 64, i / 64)[0]).collect::<Vec<f32>>()
        };
        let mut f = fixture();
        let id = checker(&mut f);
        let plain = values(&f);
        // Bevel: never above the shape, and its edges become ramps.
        let fx = content_fx(&mut f, id, EffectKind::Filter(Filter::Bevel { width: 128.0, wrap: false }));
        let bevel = values(&f);
        let mids = |v: &[f32]| v.iter().filter(|x| **x > 0.05 && **x < 0.95).count();
        assert!(bevel.iter().zip(&plain).all(|(b, p)| *b <= p + 1e-4 && *b >= 0.0), "a bevel only lowers values");
        assert!(mids(&bevel) > mids(&plain) + 64, "{} vs {}", mids(&bevel), mids(&plain));
        // Mosaic: fewer changes from one texel to the next.
        f.project.layer_mut(id).unwrap().content_effects.retain(|e| e.id != fx);
        let fx = content_fx(&mut f, id, EffectKind::Filter(Filter::Mosaic { size: 768.0, alpha: true }));
        let mosaic = values(&f);
        let steps = |v: &[f32]| v.windows(2).filter(|w| (w[0] - w[1]).abs() > 0.01).count();
        assert!(steps(&mosaic) < steps(&plain), "{} vs {}", steps(&mosaic), steps(&plain));
        assert!(mosaic.iter().all(|v| (0.0..=1.0).contains(v)));
        // Glow on uniform content adds its own blur: 0.2 + 0.2.
        f.project.layer_mut(id).unwrap().content_effects.retain(|e| e.id != fx);
        let mut f = fixture();
        let id = fill_layer(&mut f.project, [0.1; 3], 0.2);
        content_fx(&mut f, id, EffectKind::Filter(Filter::Glow { radius: 32.0, amount: 1.0 }));
        assert!(close(run(&f, None).value(Channel::Roughness, 30, 30), [0.4; 3]));
    }

    #[test]
    fn baked_lighting_lights_the_base_color_by_the_normal() {
        let lit = |elevation: f32| {
            let mut f = fixture();
            let id = fill_layer(&mut f.project, [0.5; 3], 0.3);
            let fx = Filter::BakedLighting { azimuth: 0.0, elevation, light: 0.6, ambient: 0.4, occlusion: 1.0, stylized: None };
            content_fx(&mut f, id, EffectKind::Filter(fx));
            let out = run(&f, None);
            (out.value(Channel::BaseColor, 30, 30), out.value(Channel::Roughness, 30, 30))
        };
        let n = primitives::plane(2.0, 2).normals[0];
        // Facing the light: ambient + light = 1 (unchanged); facing away: ambient only.
        let (toward, rough) = lit(if n.y > 0.0 { 90.0 } else { -90.0 });
        assert!(close(toward, [0.5; 3]) && close(rough, [0.3; 3]), "{toward:?} {rough:?}");
        let (away, _) = lit(if n.y > 0.0 { -90.0 } else { 90.0 });
        assert!(close(away, [0.2; 3]), "{away:?}");
        let f = Filter::BakedLighting { azimuth: 0.0, elevation: 0.0, light: 0.0, ambient: 1.0, occlusion: 0.5, stylized: None };
        assert_eq!(f.baked_light([0.0, 1.0, 0.0], 0.2), Some(0.6), "half the occlusion");
        assert_eq!(Filter::Invert.baked_light([0.0; 3], 1.0), None);
    }

    /// Baked Lighting Stylized's highlights read the roughness of the layer it lights: a smooth layer shines brighter than a rough one under the same sun
    #[test]
    fn stylized_baked_lighting_highlights_read_the_layers_roughness() {
        let lit = |rough: f32| {
            let mut f = fixture();
            let id = fill_layer(&mut f.project, [0.2; 3], rough);
            let n = primitives::plane(2.0, 2).normals[0];
            let stylized = Some(Box::new(StylizedLighting { lights: [texturecraft_core::DirectionalLight::off(0.0); 2], ..StylizedLighting::library() }));
            let elevation = if n.y > 0.0 { 90.0 } else { -90.0 };
            let fx = Filter::BakedLighting { azimuth: 0.0, elevation, light: 1.0, ambient: 0.0, occlusion: 0.0, stylized };
            content_fx(&mut f, id, EffectKind::Filter(fx));
            run(&f, None).value(Channel::BaseColor, 30, 30)
        };
        let (smooth, rough) = (lit(0.15), lit(0.9));
        assert!(smooth[0] > rough[0] + 0.05, "smooth {smooth:?} rough {rough:?}");
    }

    #[test]
    fn geometry_masks_show_layers_on_chosen_parts_only() {
        let mut f = fixture();
        // Two parts of the plane's one material: the first half of its triangles and the rest.
        let n = f.mesh.indices.len() as u32 / 6 * 3;
        f.mesh.submeshes = vec![
            texturecraft_geom::SubMesh { name: "Lid".into(), material: 0, start: 0, count: n },
            texturecraft_geom::SubMesh { name: "Body".into(), material: 0, start: n, count: f.mesh.indices.len() as u32 - n },
        ];
        let id = fill_layer(&mut f.project, [1.0, 0.0, 0.0], 0.2);
        let red = |f: &Fixture| {
            let out = run(f, None);
            (0..64 * 64).filter(|i| close(out.value(Channel::BaseColor, i % 64, i / 64), [1.0, 0.0, 0.0])).count()
        };
        let all = red(&f);
        f.project.layer_mut(id).unwrap().geometry_mask = Some(vec!["Lid".into()]);
        let lid = red(&f);
        f.project.layer_mut(id).unwrap().geometry_mask = Some(vec!["Body".into()]);
        let body = red(&f);
        assert!(lid > 0 && body > 0 && lid + body == all, "{lid} + {body} of {all}");
        f.project.layer_mut(id).unwrap().geometry_mask = Some(Vec::new());
        assert_eq!(red(&f), 0, "no parts: hidden everywhere");
    }

    #[test]
    fn content_paint_and_fill_effects_add_to_layers() {
        let mut f = fixture();
        let id = f.project.alloc_id();
        f.project.insert_layer(0, Layer::new(id, "P", LayerKind::Paint, &[Channel::BaseColor, Channel::Roughness]), None, false).unwrap();
        let mut material = Fill::default();
        material.sources.insert(Channel::Roughness, Source::Uniform([0.2; 3]));
        content_fx(&mut f, id, EffectKind::Material(material));
        let paint = content_fx(&mut f, id, EffectKind::Paint);
        // Roughness 0.9 painted at half coverage (premultiplied).
        f.pixels.layer_mut(paint, Channel::Roughness, (64, 64)).set(3, 3, &[to_u16(0.45), to_u16(0.5)]);
        let out = run(&f, None);
        assert!(close(out.value(Channel::Roughness, 10, 10), [0.2; 3]), "the fill effect covers the empty paint layer");
        assert!(close(out.value(Channel::Roughness, 3, 3), [0.55; 3]), "{:?}", out.value(Channel::Roughness, 3, 3));
        assert!(close(out.value(Channel::BaseColor, 3, 3), [0.5; 3]), "channels without a source stay untouched");
        // Flattening bakes the content effects in.
        let images = Default::default();
        let inp = Inputs {
            project: &f.project,
            ts: 0,
            mesh: &f.mesh,
            gbuf: &f.gbuf,
            pixels: &f.pixels,
            baked: &f.baked,
            block_scales: f.project.texture_sets[0].block_scales(),
            preview: None,
            images: &images,
            bounds: f.mesh.bounds(),
        };
        let flat = flatten_layer(&inp, id, &all_tiles(64, 64)).unwrap();
        assert!(flat.iter().all(|(c, _, _)| *c == Channel::Roughness));
        let (_, _, data) = flat.iter().find(|(_, t, _)| *t == (0, 0)).unwrap();
        let o = 3 * TILE as usize + 3;
        assert!((from_u16(data[o * 2]) - 0.55).abs() < 1e-3 && (from_u16(data[o * 2 + 1]) - 1.0).abs() < 1e-3);
    }

    #[test]
    fn anchors_feed_masks_above_in_stack_order() {
        let mut f = fixture();
        let low = f.project.alloc_id();
        let mut fill = Fill::default();
        fill.sources.insert(Channel::Height, Source::Uniform([0.4; 3]));
        f.project.insert_layer(0, Layer::new(low, "Low", LayerKind::Fill(fill), &[Channel::Height]), None, false).unwrap();
        let anchor = content_fx(&mut f, low, EffectKind::Anchor);
        let top = fill_layer(&mut f.project, [1.0; 3], 0.5);
        let mut mask = Mask::new(MaskBackground::Black);
        let source = Source::Anchor { anchor, channel: Channel::Height };
        mask.effects.push(Effect::new(f.project.alloc_id(), EffectKind::Fill { source, projection: Projection::default(), sampling: Default::default() }));
        f.project.layer_mut(top).unwrap().mask = Some(mask.clone());
        assert!(close(run(&f, None).value(Channel::BaseColor, 5, 5), [0.7; 3]));
        // Under a blur the anchor is read by the field pre-pass too: a flat height stays flat.
        mask.effects.push(Effect::new(f.project.alloc_id(), EffectKind::Filter(Filter::Blur { radius: 32.0, wrap: false })));
        f.project.layer_mut(top).unwrap().mask = Some(mask);
        assert!(close(run(&f, None).value(Channel::BaseColor, 5, 5), [0.7; 3]));
        // An anchor above its reader is not evaluated yet: it reads the channel default (0).
        f.project.move_layer(low, 1).unwrap();
        assert!(close(run(&f, None).value(Channel::BaseColor, 5, 5), [0.5; 3]));
    }

    /// A white fill layer on top whose mask reads anchor point `anchor` (any channel: a mask anchor point is gray)
    fn anchor_reader(f: &mut Fixture, anchor: LayerId) -> LayerId {
        let top = fill_layer(&mut f.project, [1.0; 3], 0.5);
        let mut mask = Mask::new(MaskBackground::Black);
        let source = Source::Anchor { anchor, channel: Channel::Roughness };
        mask.effects.push(Effect::new(f.project.alloc_id(), EffectKind::Fill { source, projection: Projection::default(), sampling: Default::default() }));
        f.project.layer_mut(top).unwrap().mask = Some(mask);
        top
    }

    /// A mask of a 0.4 gray fill, an anchor point, then a fill of `after`: (the mask, the anchor point)
    fn anchored_mask(f: &mut Fixture, after: f32) -> (Mask, LayerId) {
        let (gray, anchor, last) = (f.project.alloc_id(), f.project.alloc_id(), f.project.alloc_id());
        let uniform = |v: f32| EffectKind::Fill { source: Source::Uniform([v; 3]), projection: Projection::default(), sampling: Default::default() };
        let mut mask = Mask::new(MaskBackground::Black);
        mask.effects = vec![Effect::new(gray, uniform(0.4)), Effect::new(anchor, EffectKind::Anchor), Effect::new(last, uniform(after))];
        (mask, anchor)
    }

    #[test]
    fn mask_anchor_points_record_the_mask_value_at_their_place() {
        let mut f = fixture();
        let low = fill_layer(&mut f.project, [0.0; 3], 0.5);
        let (mut mask, anchor) = anchored_mask(&mut f, 1.0);
        f.project.layer_mut(low).unwrap().mask = Some(mask.clone());
        anchor_reader(&mut f, anchor);

        // The low layer shows whole (black), the top one at the 0.4 the anchor point recorded
        assert!(close(run(&f, None).value(Channel::BaseColor, 5, 5), [0.4; 3]));

        // Under a blur precomputed over the region the anchor point still records its own place
        mask.effects.push(Effect::new(f.project.alloc_id(), EffectKind::Filter(Filter::Blur { radius: 32.0, wrap: false })));
        f.project.layer_mut(low).unwrap().mask = Some(mask);
        let v = run(&f, None).value(Channel::BaseColor, 5, 5);
        assert!(close(v, [0.4; 3]), "{v:?}");

        // A paint layer with nothing painted draws nothing (the base stays 0.5) but its mask's anchor points still record
        f.project.layer_mut(low).unwrap().kind = LayerKind::Paint;
        let v = run(&f, None).value(Channel::BaseColor, 5, 5);
        assert!(close(v, [0.7; 3]), "{v:?}");
    }

    #[test]
    fn fills_inside_folders_take_the_folders_scale_and_rotation_on_top_of_their_own() {
        // Nested folders (x2 turned 30, then x1.5 turned -10) around a procedural fill with a procedural mask fill
        let build = |outer: ContentsOffset, inner: ContentsOffset, projection: Projection| {
            let mut f = fixture();
            let folder = |f: &mut Fixture, contents: ContentsOffset| {
                let id = f.project.alloc_id();
                Layer::new(id, "G", LayerKind::Group { children: vec![], collapsed: false, contents, randomized: Vec::new() }, &[Channel::BaseColor])
            };
            let outer = folder(&mut f, outer);
            let outer = f.project.insert_layer(0, outer, None, false).unwrap();
            let inner = folder(&mut f, inner);
            let inner = f.project.insert_layer(0, inner, Some(outer), true).unwrap();
            let pattern = |seed| Source::Procedural { pattern: Procedural { seed, ..Procedural::new(ProceduralKind::Perlin) }, low: [0.0; 3], high: [1.0; 3] };
            let mut fill = Fill { projection: projection.clone(), ..Fill::default() };
            fill.sources.insert(Channel::BaseColor, pattern(3));
            let id = f.project.alloc_id();
            let mut layer = Layer::new(id, "C", LayerKind::Fill(fill), &[Channel::BaseColor]);
            let mut mask = Mask::new(MaskBackground::Black);
            mask.effects.push(Effect::new(f.project.alloc_id(), EffectKind::Fill { source: pattern(9), projection, sampling: Default::default() }));
            layer.mask = Some(mask);
            f.project.insert_layer(0, layer, Some(inner), true).unwrap();
            run(&f, None)
        };
        let own = Projection::Uv { scale: [2.0, 1.0], rotation: 15.0, offset: [0.1, 0.0] };
        let (outer, inner) = (ContentsOffset::new(2.0, 30.0), ContentsOffset::new(1.5, -10.0));
        let none = ContentsOffset::NONE;
        let offset = build(outer, inner, own.clone());
        let written = build(none, none, Projection::Uv { scale: [6.0, 3.0], rotation: 35.0, offset: [0.1, 0.0] });
        assert_eq!(offset, written, "the folders' offset renders as if it were written into the fills");
        assert_ne!(offset, build(none, none, own), "without the offset every fill keeps its own");
    }

    #[test]
    fn folder_mask_anchor_points_record_even_where_the_folder_is_masked_out() {
        let mut f = fixture();
        let gid = f.project.alloc_id();
        let folder = Layer::new(
            gid,
            "G",
            LayerKind::Group { children: vec![], collapsed: false, contents: Default::default(), randomized: Vec::new() },
            &[Channel::BaseColor, Channel::Roughness],
        );
        f.project.insert_layer(0, folder, None, false).unwrap();
        let child = f.project.alloc_id();
        let mut fill = Fill::default();
        fill.sources.insert(Channel::BaseColor, Source::Uniform([0.0; 3]));
        f.project.insert_layer(0, Layer::new(child, "C", LayerKind::Fill(fill), &[Channel::BaseColor]), Some(gid), true).unwrap();
        let (mask, anchor) = anchored_mask(&mut f, 0.0);
        f.project.layer_mut(gid).unwrap().mask = Some(mask);
        anchor_reader(&mut f, anchor);

        // The folder's mask ends at 0 (its black child hides), the reader above shows at 0.4 over the 0.5 base
        let v = run(&f, None).value(Channel::BaseColor, 5, 5);
        assert!(close(v, [0.7; 3]), "{v:?}");
    }

    /// A fixture under a decal fill of image "decal" framed on the plane's middle, its projection changed by `change`
    fn decal_fixture(change: impl FnOnce(&mut Fill)) -> Fixture {
        let mut f = fixture();
        let s = f.gbuf.surface(&f.mesh, 32 * 64 + 32).unwrap();
        let d = texturecraft_core::Decal {
            center: s.pos.to_array(),
            normal: s.nrm.to_array(),
            up: s.nrm.any_orthonormal_vector().to_array(),
            size: [1.0, 1.0],
            rotation: 0.0,
            offset: [0.0; 2],
            depth: 0.5,
        };
        let id = f.project.alloc_id();
        let mut fill = Fill { projection: Projection::decal(d), ..Fill::default() };
        fill.sources.insert(Channel::BaseColor, Source::image("decal"));
        change(&mut fill);
        f.project.insert_layer(0, Layer::new(id, "Decal", LayerKind::Fill(fill), &[Channel::BaseColor]), None, false).unwrap();
        f
    }

    /// `f` composited with `image` as its "decal" image
    fn composite_with_decal(f: &Fixture, image: crate::store::ImageRes) -> Composite {
        let images: ImageStore = [("decal".to_string(), std::sync::Arc::new(image))].into_iter().collect();
        let inp = Inputs {
            project: &f.project,
            ts: 0,
            mesh: &f.mesh,
            gbuf: &f.gbuf,
            pixels: &f.pixels,
            baked: &f.baked,
            block_scales: f.project.texture_sets[0].block_scales(),
            preview: None,
            images: &images,
            bounds: f.mesh.bounds(),
        };
        let mut out = Composite::new(&f.project.texture_sets[0].channel_list(), 64);
        composite_tiles(&inp, &mut out, &all_tiles(64, 64));
        out
    }

    /// Texels of a 64 x 64 composite whose base colour is not the fixture's grey
    fn decal_texels(out: &Composite) -> Vec<[f32; 3]> {
        (0..64 * 64).map(|i| out.value(Channel::BaseColor, i % 64, i / 64)).filter(|v| !close(*v, [0.5; 3])).collect()
    }

    #[test]
    fn decal_fills_cover_only_their_frame_and_image_alpha() {
        let f = decal_fixture(|_| {});
        let composite = |alpha: f32| composite_with_decal(&f, crate::store::ImageRes::new(2, 2, vec![[1.0, 0.0, 0.0, alpha]; 4], false));
        let out = composite(1.0);
        assert!(close(out.value(Channel::BaseColor, 32, 32), [1.0, 0.0, 0.0]), "{:?}", out.value(Channel::BaseColor, 32, 32));
        assert!(close(out.value(Channel::BaseColor, 2, 2), [0.5; 3]), "outside the frame");
        let half = composite(0.5);
        assert!(close(half.value(Channel::BaseColor, 32, 32), [0.75, 0.25, 0.25]), "the image alpha shapes the decal");
    }

    #[test]
    fn a_cropped_frames_image_alpha_turns_with_its_image() {
        let f = decal_fixture(|fill| {
            if let Projection::Planar { uv, .. } = &mut fill.projection {
                uv.rotation = 180.0;
            }
        });

        // Opaque red on the left half, see-through green on the right
        let half = (0..8 * 8).map(|i| if i % 8 < 4 { [1.0, 0.0, 0.0, 1.0] } else { [0.0, 1.0, 0.0, 0.0] }).collect();
        let shown = decal_texels(&composite_with_decal(&f, crate::store::ImageRes::new(8, 8, half, false)));

        assert!(!shown.is_empty(), "the red half shows");
        assert!(shown.iter().all(|v| v[1] < 0.75), "the see-through green half stays hidden once turned (a seam texel blends some in): {shown:?}");
    }

    #[test]
    fn a_cropped_frame_with_wrap_none_shows_one_image_however_it_tiles() {
        let opaque = || crate::store::ImageRes::new(2, 2, vec![[1.0, 0.0, 0.0, 1.0]; 4], false);
        let once = decal_texels(&composite_with_decal(&decal_fixture(|_| {}), opaque())).len();
        let tiled = decal_fixture(|fill| {
            fill.sampling.wrap = texturecraft_core::UvWrap::None;
            if let Projection::Planar { uv, .. } = &mut fill.projection {
                uv.tiling = [2.0, 2.0];
            }
        });

        let quarter = decal_texels(&composite_with_decal(&tiled, opaque())).len();

        assert!(once > 0, "the frame shows");
        assert!(quarter.abs_diff(once / 4) <= once / 16, "tiled twice with no wrap, one image fills a quarter of the frame: {quarter} of {once}");
    }

    #[test]
    fn anchors_read_colors_as_gray_in_fill_layers() {
        let mut f = fixture();
        let low = fill_layer(&mut f.project, [0.2, 0.4, 0.6], 0.5);
        let anchor = content_fx(&mut f, low, EffectKind::Anchor);
        let id = f.project.alloc_id();
        let mut fill = Fill::default();
        fill.sources.insert(Channel::Roughness, Source::Anchor { anchor, channel: Channel::BaseColor });
        f.project.insert_layer(0, Layer::new(id, "Top", LayerKind::Fill(fill), &[Channel::Roughness]), None, false).unwrap();
        let expect = texturecraft_core::lum([0.2, 0.4, 0.6]);
        assert!(close(run(&f, None).value(Channel::Roughness, 9, 9), [expect; 3]));
    }
}
