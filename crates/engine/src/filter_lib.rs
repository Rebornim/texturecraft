//! The filter library (the Filters shelf): adjustment, blur and distortion filters for
//! layer content and masks, and material finishes (content only). `effect.add.filter` adds one
//! by id; `filter.list` lists them with their default parameters.

use serde_json::{Value, json};
use texturecraft_core::{
    AnisotropicKuwahara, Bevel, BevelBlur, BevelCorner, BevelDirection, BevelSmooth, Channel, ColorMatch, ConversionMode, CurvePreset, DirectionalDistance,
    EffectKind, Fill, Filter, GradientInput, GrayscaleConversion, GrayscaleMode, HeightToNormal, HighpassSpace, Levels, MaskOutline, Mirror, MirrorCorner,
    MirrorMode, OutlinePosition, PbrValidate, Procedural, ProceduralKind, Projection, SlopeBlending, Source, SourceColorMode, SourceSlopeBlur, SourceWarp,
    StylizedLighting, TargetColorMode, ThresholdMode, TileTransform, ValidationMode, WarpBlending, WarpMode, WarpSourceMode,
};

use crate::commands::{CommandSpec, always, bad, boolp, color_of, color_param, f32p, has_layer, strp, u64p};
use crate::filter_about::FilterAbout;
use crate::{Result, Session, cmd};

/// A filter of the library.
#[derive(Clone, Copy, Debug)]
pub struct FilterDef {
    pub id: &'static str,
    pub label: &'static str,
    /// Shelf group.
    pub group: &'static str,
    /// Usable on masks too (material finishes only fill layer content).
    pub mask: bool,
}

impl FilterDef {
    /// What it does and the words it is found by (its Attributes)
    pub fn about(&self) -> Option<FilterAbout> {
        crate::filter_about::of(self.id)
    }

    /// Whether it builds a filter effect (levels and material finishes are effect types of their own)
    pub fn is_filter_effect(&self) -> bool {
        matches!(build(self.id, &Value::Null), Ok(EffectKind::Filter(_)))
    }
}

/// The library entries that build filter effects, in shelf order, worked out once
pub fn filter_effects() -> &'static [&'static FilterDef] {
    static FILTER_EFFECTS: std::sync::LazyLock<Vec<&'static FilterDef>> =
        std::sync::LazyLock::new(|| FILTERS.iter().filter(|def| def.is_filter_effect()).collect());
    &FILTER_EFFECTS
}

const fn def(id: &'static str, label: &'static str, group: &'static str, mask: bool) -> FilterDef {
    FilterDef { id, label, group, mask }
}

pub const FILTERS: &[FilterDef] = &[
    def("hsl", "HSL Perceptive", "Adjustments", true),
    def("levels", "Levels", "Adjustments", true),
    def("contrastLuminosity", "Contrast Luminosity", "Adjustments", true),
    def("colorBalance", "Color Balance", "Adjustments", true),
    def("colorCorrect", "Color Correct", "Adjustments", true),
    def("gradientMap", "Gradient", "Adjustments", true),
    def("gradientCurve", "Gradient Curve", "Adjustments", true),
    def("gradientDynamic", "Gradient Dynamic", "Adjustments", true),
    def("posterize", "Posterize", "Adjustments", true),
    def("clamp", "Clamp", "Adjustments", true),
    def("smoothstep", "Smoothstep", "Adjustments", true),
    def("heightAdjust", "Height Adjust", "Adjustments", true),
    def("histogramScan", "Histogram Scan", "Adjustments", true),
    def("histogramRange", "Histogram Range", "Adjustments", true),
    def("histogramShift", "Histogram Shift", "Adjustments", true),
    def("invert", "Invert", "Adjustments", true),
    def("threshold", "Threshold", "Adjustments", true),
    def("grayscale", "Grayscale Conversion", "Adjustments", true),
    def("edgeDetect", "Edge Detect", "Adjustments", true),
    def("normalInvert", "Normal Invert", "Adjustments", false),
    def("bakedLighting", "Baked Lighting Stylized", "Adjustments", false),
    def("bakedLightingEnvironment", "Baked Lighting Environment", "Adjustments", false),
    def("replaceColor", "Replace Color", "Adjustments", false),
    def("pbrValidate", "PBR Validate (Metallic Roughness)", "Adjustments", false),
    def("heightToNormal", "Height To Normal", "Adjustments", false),
    def("colorMatch", "Color Match", "Adjustments", false),
    def("blur", "Blur", "Blur and sharpen", true),
    def("blurDirectional", "Blur Directional", "Blur and sharpen", true),
    def("slopeBlur", "Blur Slope", "Blur and sharpen", true),
    def("sharpen", "Sharpen", "Blur and sharpen", true),
    def("highPass", "Highpass", "Blur and sharpen", true),
    def("fxaa", "FXAA (Anti-aliasing)", "Blur and sharpen", true),
    def("anisotropicKuwahara", "Anisotropic Kuwahara", "Blur and sharpen", true),
    def("bevel", "Bevel", "Blur and sharpen", true),
    def("bevelSmooth", "Bevel Smooth", "Blur and sharpen", true),
    def("directionalDistance", "Directional Distance", "Blur and sharpen", true),
    def("glow", "Glow", "Blur and sharpen", true),
    def("dropShadow", "Drop Shadow", "Blur and sharpen", true),
    def("maskOutline", "Mask Outline", "Blur and sharpen", true),
    def("warp", "Warp", "Distort", true),
    def("mosaic", "Pixelate", "Distort", true),
    def("mirror", "Mirror", "Distort", true),
    def("transform", "Transform", "Distort", true),
    def("matFinishGrind", "MatFinish Grind", "Material finish", false),
    def("matFinishRaw", "MatFinish Raw", "Material finish", false),
    def("matFinishSanded", "MatFinish Sanded", "Material finish", false),
    def("matFinishHammered", "MatFinish Hammered", "Material finish", false),
];

fn norm(s: &str) -> String {
    s.to_ascii_lowercase().replace([' ', '_', '-', '/'], "")
}

/// The library filter `id` (case and separators ignored; labels work too). Directional Warp's former id and labels find Warp, whose mode it is now.
pub fn find(id: &str) -> Option<&'static FilterDef> {
    let n = if is_directional_warp(id) { norm("warp") } else { norm(id) };
    FILTERS.iter().find(|f| norm(f.id) == n || norm(f.label) == n)
}

/// Whether `id` names Directional Warp, a filter of its own before the warp modes (`warpDirectional`, "Warp Directional", "Directional Warp")
fn is_directional_warp(id: &str) -> bool {
    matches!(norm(id).as_str(), "warpdirectional" | "directionalwarp")
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "effect.filter.restoreDefaults",
            "Restore defaults",
            [],
            None,
            "{effect?: filter effect id, default the selected effect} — its filter's parameters back to their defaults (name, channels and blending stay) in one undo step; no step when they are already; returns {changed}",
            has_layer,
            restore_defaults
        ),
        cmd!(
            "filter.list",
            "List filters",
            [],
            None,
            "{} — the filter library: id, label, group, whether masks take it, default parameters, what it does (description) and its tags",
            always,
            |_, _| Ok(Value::Array(FILTERS.iter().map(listed).collect()))
        ),
    ]
}

/// `filter.list`'s entry for `def`
fn listed(def: &FilterDef) -> Value {
    let about = def.about();
    json!({
        "id": def.id,
        "label": def.label,
        "group": def.group,
        "mask": def.mask,
        "defaults": build(def.id, &Value::Null).ok(),
        "description": about.map(|about| about.description),
        "tags": about.map(|about| about.tags),
    })
}

/// The filter `effect.filter.restoreDefaults` works on: its id and the library defaults of the filter it is (None: an empty filter)
fn filter_defaults(s: &Session, p: &Value) -> Result<(texturecraft_core::LayerId, Option<Filter>)> {
    const CMD: &str = "effect.filter.restoreDefaults";
    let effect = crate::mask_cmds::effect_id(s, p, CMD)?;
    let filter = match s.doc.project.effect(effect) {
        Some((_, texturecraft_core::Effect { kind: EffectKind::Filter(filter), .. })) => filter,
        Some(_) => return Err(bad(CMD, format!("effect {} is not a filter", effect.0))),
        None => return Err(bad(CMD, format!("no effect {}", effect.0))),
    };
    let defaults = match find(filter.label()) {
        Some(_) => match build(filter.label(), &Value::Null)? {
            EffectKind::Filter(defaults) => Some(defaults),
            _ => None,
        },
        None => None,
    };
    Ok((effect, defaults))
}

/// Puts a filter effect's parameters back to their library defaults in one undo step
fn restore_defaults(s: &mut Session, p: &Value) -> Result<Value> {
    let (effect, defaults) = filter_defaults(s, p)?;
    let current = s.doc.project.effect(effect).map(|(_, found)| &found.kind);
    let Some(defaults) = defaults.filter(|defaults| current != Some(&EffectKind::Filter(defaults.clone()))) else {
        return Ok(json!({"changed": false}));
    };
    s.edit("Restore defaults", |s| {
        if let Some(edited) = s.doc.project.effect_mut(effect) {
            edited.kind = EffectKind::Filter(defaults);
        }
        Ok(())
    })?;
    Ok(json!({"changed": true}))
}

/// A Color Correct tone that changes nothing: no contrast, no luminosity, saturation as it is
const NEUTRAL_TONE: [f32; 3] = [0.0, 0.0, 0.5];

/// Mid gray as a color picker shows it (sRGB 0.5), in linear RGB
const MID_GRAY: f32 = 0.214;

/// A new glow's warm light, in linear RGB
const GLOW_COLOR: [f32; 3] = [0.871, 0.578, 0.188];

/// The Gradient filter as the library adds it: black, mid gray and white at 0, 0.5 and 1 (the other seven colors wait white at 1)
fn library_gradient() -> Filter {
    let quantity = 3;
    let mut colors = vec![[0.0; 3], [MID_GRAY; 3]];
    colors.resize(usize::from(Filter::MAX_GRADIENT_COLORS), [1.0; 3]);
    let mut positions = vec![0.0, 0.5];
    positions.resize(colors.len(), 1.0);
    Filter::GradientMap { colors, positions, quantity: Some(quantity), input: GradientInput::LIBRARY, linear: true, debug: false }
}

/// The effect for library filter `id` with its defaults, then the parameters in `p`.
pub fn build(id: &str, p: &Value) -> Result<EffectKind> {
    let cmd = "effect.add.filter";
    let d = find(id).ok_or_else(|| bad(cmd, format!("unknown filter `{id}` (see filter.list)")))?;
    let mut f = match d.id {
        "warp" if is_directional_warp(id) => Filter::SourceWarp(SourceWarp { mode: WarpMode::Directional, ..SourceWarp::LIBRARY }),
        "invert" => Filter::Invert,
        "threshold" => Filter::Threshold { level: 0.01, mode: ThresholdMode::Greater },
        "blur" => Filter::Blur { radius: 0.3 * Filter::BLUR_INTENSITY, wrap: false },
        "sharpen" => Filter::Sharpen { amount: 1.0 },
        "hsl" => Filter::Hsl { hue: 0.0, saturation: 0.0, lightness: 0.0, perceptive: true },
        "contrastLuminosity" => Filter::ContrastLuminosity { contrast: 0.0, luminosity: 0.0 },
        "colorCorrect" => Filter::ColorCorrect { shadows: NEUTRAL_TONE, midtones: NEUTRAL_TONE, highlights: NEUTRAL_TONE },
        "colorBalance" => Filter::ColorBalance { shadows: [0.0; 3], midtones: [0.0; 3], highlights: [0.0; 3] },
        "gradientMap" => library_gradient(),
        "gradientCurve" => Filter::GradientCurve { preset: CurvePreset::Round, invert: false, balance: 0.5, contrast: 0.0 },
        "gradientDynamic" => Filter::GradientDynamic { vertical: false, position: 0.0, color: [0.6; 3] },
        "posterize" => Filter::Posterize { levels: 8.0, alpha: false },
        "clamp" => Filter::Clamp { min: 0.0, max: 1.0 },
        "smoothstep" => Filter::Smoothstep { min: 0.0, max: 1.0 },
        "heightAdjust" => Filter::HeightAdjust { invert: false, offset: 0.0, multiply: 1.0 },
        "histogramScan" => Filter::HistogramScan { position: 0.5, contrast: 0.0, invert_position: false },
        "histogramRange" => Filter::HistogramRange { range: 0.5, position: 0.5 },
        "histogramShift" => Filter::HistogramShift { offset: 0.0 },
        "warp" => Filter::SourceWarp(SourceWarp::LIBRARY),
        "blurDirectional" => Filter::DirectionalBlur { radius: 16.0, angle: 0.0 },
        "slopeBlur" => Filter::SourceSlopeBlur(SourceSlopeBlur::LIBRARY),
        "highPass" => Filter::HighPass { radius: 3.0 * Filter::BLUR_INTENSITY, space: HighpassSpace::Working, alpha: false },
        "edgeDetect" => Filter::EdgeDetect { width: 2.0 },
        "grayscale" => Filter::GrayscaleConversion(GrayscaleConversion::LIBRARY),
        "normalInvert" => Filter::NormalInvert,
        "bevel" => Filter::DistanceBevel(Bevel::LIBRARY),
        "bevelSmooth" => Filter::BevelSmooth(BevelSmooth::LIBRARY),
        "directionalDistance" => Filter::DirectionalDistance(DirectionalDistance::LIBRARY),
        "mosaic" => Filter::Mosaic { size: Filter::pixel_size(32.0), alpha: true },
        "fxaa" => Filter::Fxaa { alpha: false },
        "anisotropicKuwahara" => Filter::AnisotropicKuwahara(AnisotropicKuwahara::LIBRARY),
        "pbrValidate" => Filter::PbrValidate(PbrValidate::LIBRARY),
        "heightToNormal" => Filter::HeightToNormal(HeightToNormal::LIBRARY),
        "colorMatch" => Filter::ColorMatch(ColorMatch::LIBRARY),
        "mirror" => Filter::Mirror(Mirror::LIBRARY),
        "transform" => Filter::Transform(TileTransform::LIBRARY),
        "maskOutline" => Filter::MaskOutline(MaskOutline::LIBRARY),
        "glow" => Filter::HaloGlow { on_mask: false, precise: false, width: 0.15, spread: 0.0, opacity: 1.0, color: GLOW_COLOR },
        "dropShadow" => Filter::DropShadow { angle: 90.0, distance: 0.1, size: 0.2, spread: 0.0, opacity: 0.5, color: [0.0; 3] },
        "replaceColor" => Filter::ReplaceColor { from: [0.5, 0.05, 0.05], to: [0.05, 0.15, 0.5], tolerance: 0.2, softness: 0.15 },
        "bakedLighting" => Filter::BakedLighting {
            azimuth: 0.0,
            elevation: 45.0,
            light: 1.0,
            ambient: 1.0,
            occlusion: 0.0,
            stylized: Some(Box::new(StylizedLighting::library())),
        },
        "bakedLightingEnvironment" => Filter::EnvironmentLighting {
            horizontal: 0.0,
            vertical: 0.0,
            exposure: 0.0,
            height_intensity: 4.0,
            ambient_occlusion: 1.0,
            specular_occlusion: 0.0,
        },
        "levels" => {
            let mut l = Levels::default();
            crate::mask_cmds::levels_from(cmd, p, &mut l)?;
            return Ok(EffectKind::Levels(l));
        }
        finish => return mat_finish(finish).map(EffectKind::Material).ok_or_else(|| bad(cmd, format!("unknown filter `{id}`"))),
    };
    apply(p, &mut f, cmd)?;
    Ok(EffectKind::Filter(f))
}

/// A material finish: procedural roughness and height (tri-planar, so it is seamless).
fn mat_finish(id: &str) -> Option<Fill> {
    let (kind, scale, rough, height) = match id {
        "matFinishGrind" => (ProceduralKind::Brushed, 6.0, [0.22, 0.42], 0.012),
        "matFinishRaw" => (ProceduralKind::Clouds, 8.0, [0.45, 0.75], 0.02),
        "matFinishSanded" => (ProceduralKind::Noise, 3.0, [0.38, 0.58], 0.008),
        "matFinishHammered" => (ProceduralKind::Hammered, 6.0, [0.25, 0.45], 0.08),
        _ => return None,
    };
    let pattern = Procedural { scale, ..Procedural::new(kind) };
    let mut fill = Fill {
        projection: Projection::Triplanar { scale: 1.0, sharpness: 8.0, offset: [0.0; 3], uv: Default::default(), projector: Default::default() },
        ..Fill::default()
    };
    fill.sources.insert(Channel::Roughness, Source::Procedural { pattern: pattern.clone(), low: [rough[0]; 3], high: [rough[1]; 3] });
    fill.sources.insert(Channel::Height, Source::Procedural { pattern, low: [-height; 3], high: [height; 3] });
    Some(fill)
}

/// Scalar parameters of a filter for editors: (param key, label, value, min, max); colors (color balance, gradient map) and the HSL hue have their own editors
pub fn scalar_params(f: &Filter) -> Vec<(&'static str, &'static str, f32, f32, f32)> {
    match f {
        Filter::Blur { radius, .. } => vec![("intensity", "Blur Intensity", *radius / Filter::BLUR_INTENSITY, 0.0, 16.0)],
        Filter::Sharpen { amount } => vec![("amount", "Sharpen Intensity", *amount, 0.0, 4.0)],
        Filter::Threshold { level, .. } => vec![("level", "Threshold", *level, 0.0, 1.0)],
        Filter::Hsl { saturation, lightness, .. } => {
            vec![("saturation", "Saturation", *saturation, -1.0, 1.0), ("lightness", "Lightness", *lightness, -1.0, 1.0)]
        }
        Filter::ContrastLuminosity { contrast, luminosity } => {
            vec![("contrast", "Contrast", *contrast, -1.0, 1.0), ("luminosity", "Luminosity", *luminosity, -1.0, 1.0)]
        }
        Filter::Posterize { levels, .. } => vec![("levels", "Colors Amount", *levels, 2.0, 64.0)],
        Filter::Clamp { min, max } => vec![("min", "Min", *min, 0.0, 1.0), ("max", "Max", *max, 0.0, 1.0)],
        Filter::Smoothstep { min, max } => vec![("min", "Minimum", *min, 0.0, 1.0), ("max", "Maximum", *max, 0.0, 1.0)],
        Filter::HeightAdjust { offset, multiply, .. } => vec![("offset", "Offset", *offset, -1.0, 1.0), ("multiply", "Multiply", *multiply, 0.0, 10.0)],
        Filter::HistogramScan { position, contrast, .. } => vec![("position", "Position", *position, 0.0, 1.0), ("contrast", "Contrast", *contrast, 0.0, 1.0)],
        Filter::HistogramRange { range, position } => vec![("range", "Range", *range, 0.0, 1.0), ("position", "Position", *position, 0.0, 1.0)],
        Filter::HistogramShift { offset } => vec![("position", "Position", Filter::shift_position(*offset), 0.0, 1.0)],
        Filter::Warp { intensity, scale, seed, .. } => {
            vec![("intensity", "Intensity", *intensity, 0.0, 128.0), ("scale", "Source Tiling", *scale, 0.5, 64.0), ("seed", "Seed", *seed as f32, 0.0, 999.0)]
        }
        Filter::DirectionalBlur { radius, angle } => {
            vec![("intensity", "Intensity", *radius / Filter::BLUR_INTENSITY, 0.0, 16.0), ("angle", "Angle", *angle, 0.0, 360.0)]
        }
        Filter::SlopeBlur { radius, scale, seed, .. } => {
            vec![("radius", "Intensity", *radius, 0.0, 128.0), ("scale", "Source Tiling", *scale, 0.5, 64.0), ("seed", "Seed", *seed as f32, 0.0, 999.0)]
        }
        Filter::HighPass { radius, .. } => vec![("intensity", "Radius", *radius / Filter::BLUR_INTENSITY, 0.0, 16.0)],
        Filter::GrayscaleConversion(conversion) => conversion_params(conversion),
        Filter::AnisotropicKuwahara(kuwahara) => vec![
            ("radius", "Radius", kuwahara.radius, 0.0, AnisotropicKuwahara::MAX_RADIUS),
            ("smoothness", "Smoothness", kuwahara.smoothness, 0.0, 1.0),
            ("sharpness", "Sharpness", kuwahara.sharpness, 0.25, 1.0),
            ("tensorSmoothness", "Tensor Smoothness", kuwahara.tensor_smoothness, 0.0, 1.0),
            ("anisotropy", "Anisotropy", kuwahara.anisotropy, 0.0, 1.0),
            ("angle", "Anisotropy Angle", kuwahara.angle, 0.0, 360.0),
        ],
        Filter::Grayscale => conversion_params(&GrayscaleConversion::of_luminance()),
        Filter::HeightToNormal(filter) if filter.world_units => vec![
            ("surfaceSize", "Surface Size (cm)", filter.surface_size, 1.0, HeightToNormal::MAX_SURFACE_SIZE),
            ("heightDepth", "Height Depth (cm)", filter.height_depth, 0.0, HeightToNormal::MAX_HEIGHT_DEPTH),
        ],
        Filter::HeightToNormal(filter) => vec![("normalIntensity", "Normal Intensity", filter.intensity, 0.0, HeightToNormal::MAX_INTENSITY)],
        Filter::ColorMatch(matcher) => {
            vec![("hue", "Hue", matcher.hue, 0.0, 360.0), ("chroma", "Chroma", matcher.chroma, 0.0, 1.0), ("luma", "Luma", matcher.luma, 0.0, 1.0)]
        }
        Filter::EdgeDetect { width } => vec![("width", "Width", *width, 0.0, 64.0)],
        Filter::DirectionalWarp { intensity, angle, scale, seed, .. } => {
            vec![
                ("intensity", "Intensity", *intensity, 0.0, 128.0),
                ("angle", "Angle", *angle, 0.0, 360.0),
                ("scale", "Source Tiling", *scale, 0.5, 64.0),
                ("seed", "Seed", *seed as f32, 0.0, 999.0),
            ]
        }
        Filter::Bevel { width, .. } => vec![("width", "Distance", *width, 0.0, 128.0)],
        Filter::Mosaic { size, .. } => vec![("pixelsAmount", "Pixels Amount", Filter::pixels_amount(*size), 1.0, 128.0)],
        Filter::DistanceBevel(bevel) => vec![("distance", "Distance", bevel.distance, -1.0, 1.0), ("smoothing", "Smoothing", bevel.smoothing, 0.0, 5.0)],
        Filter::SourceWarp(warp) => {
            let mut params = vec![("seed", "Seed", warp.seed as f32, 0.0, 999.0), ("intensity", "Intensity", warp.intensity, 0.0, 1.0)];
            if warp.mode != WarpMode::Warp {
                params.push(("angle", "Angle", warp.angle, 0.0, 360.0));
            }
            params.extend([
                ("sourceBlur", "Source Blur", warp.source.blur, 0.0, 1.0),
                ("sourceBalance", "Source Balance", warp.source.balance, 0.0, 1.0),
                ("sourceContrast", "Source Contrast", warp.source.contrast, 0.0, 1.0),
                ("sourceTiling", "Source Tiling", warp.source.tiling, 0.01, 16.0),
            ]);
            params
        }
        Filter::SourceSlopeBlur(slope) => vec![
            ("seed", "Seed", slope.seed as f32, 0.0, 999.0),
            ("intensity", "Intensity", slope.intensity, 0.0, 1.0),
            ("quality", "Quality", slope.quality, 0.0, 1.0),
            ("sourceBlur", "Blur", slope.source.blur, 0.0, 1.0),
            ("sourceBalance", "Position", slope.source.balance, 0.0, 1.0),
            ("sourceContrast", "Contrast", slope.source.contrast, 0.0, 1.0),
            ("sourceTiling", "Source Tiling", slope.source.tiling, 0.01, 16.0),
        ],
        Filter::BevelSmooth(smooth) => vec![
            ("distance", "Distance", smooth.distance, 0.0, 0.3),
            ("smoothing", "Smoothing", smooth.smoothing, 0.0, 10.0),
            ("curveOffset", "Curve Offset", smooth.curve_offset, -1.0, 1.0),
            ("curveShape", "Curve Shape", smooth.curve_shape, -1.0, 1.0),
            ("maskThreshold", "Mask Threshold", smooth.threshold, 0.0, 1.0),
            ("distanceMapMultiplier", "Distance Map Multiplier", smooth.multiplier, 0.0, 1.0),
        ],
        Filter::DirectionalDistance(trail) => vec![
            ("distance", "Distance", trail.distance, 0.0, 0.3),
            ("angle", "Angle", trail.angle, 0.0, 360.0),
            ("contrast", "Contrast", trail.contrast, 0.0, 1.0),
            ("distanceMapMultiplier", "Distance Map Multiplier", trail.multiplier, 0.0, 1.0),
        ],
        // Only the offset of the axis the mode mirrors across shows
        Filter::Mirror(mirror) => match mirror.mode {
            MirrorMode::AxisX => vec![("axisXOffset", "Axis X Offset", mirror.offset[0], 0.0, 1.0)],
            MirrorMode::AxisY => vec![("axisYOffset", "Axis Y Offset", mirror.offset[1], 0.0, 1.0)],
            MirrorMode::Corner => Vec::new(),
        },
        Filter::MaskOutline(outline) => vec![
            ("threshold", "Threshold", outline.threshold, 0.0, 1.0),
            ("width", "Width", outline.width, 0.0, 1.0),
            ("blur", "Blur", outline.blur, 0.0, 1.0),
            ("curveShape", "Curve Shape", outline.curve, -1.0, 1.0),
        ],
        Filter::Glow { radius, amount } => {
            vec![("radius", "Width", *radius, 0.0, 128.0), ("amount", "Amount", *amount, 0.0, 5.0)]
        }
        Filter::HaloGlow { width, spread, opacity, .. } => {
            vec![("width", "Width", *width, -1.0, 1.0), ("spread", "Spread", *spread, 0.0, 1.0), ("opacity", "Opacity", *opacity, 0.0, 1.0)]
        }
        Filter::DropShadow { angle, distance, size, spread, opacity, .. } => vec![
            ("angle", "Angle", *angle, 0.0, 360.0),
            ("distance", "Distance", *distance, -0.5, 0.5),
            ("size", "Size", *size, 0.0, 1.0),
            ("spread", "Spread", *spread, 0.0, 1.0),
            ("opacity", "Opacity", *opacity, 0.0, 1.0),
        ],
        Filter::BakedLighting { azimuth, elevation, light, ambient, occlusion, .. } => vec![
            ("light", "Sun Intensity", *light, 0.0, 10.0),
            ("azimuth", "Horizontal Angle", *azimuth, 0.0, 360.0),
            ("elevation", "Vertical Angle", *elevation, 0.0, 360.0),
            ("ambient", "Sky Intensity", *ambient, 0.0, 2.0),
            ("occlusion", "Diffuse AO", *occlusion, 0.0, 1.0),
        ],
        Filter::EnvironmentLighting { horizontal, vertical, exposure, height_intensity, ambient_occlusion, specular_occlusion } => vec![
            ("horizontalRotation", "Horizontal Rotation", *horizontal, 0.0, 360.0),
            ("verticalRotation", "Vertical Rotation", *vertical, 0.0, 360.0),
            ("exposure", "Exposure", *exposure, -5.0, 5.0),
            ("heightIntensity", "Height Intensity", *height_intensity, 0.0, 8.0),
            ("ambientOcclusion", "Ambient Occlusion Intensity", *ambient_occlusion, 0.0, 1.0),
            ("specularOcclusion", "Specular Occlusion Intensity", *specular_occlusion, 0.0, 1.0),
        ],
        Filter::ReplaceColor { tolerance, softness, .. } => {
            vec![("tolerance", "Tolerance", *tolerance, 0.0, 1.0), ("softness", "Softness", *softness, 0.0, 1.0)]
        }
        Filter::GradientMap { input, .. } => vec![
            ("blur", "Blur Intensity", input.blur, 0.0, 16.0),
            ("balance", "Balance", input.balance, 0.0, 1.0),
            ("contrast", "Contrast", input.contrast, 0.0, 1.0),
        ],
        Filter::GradientCurve { balance, contrast, .. } => vec![("balance", "Balance", *balance, 0.0, 1.0), ("contrast", "Contrast", *contrast, 0.0, 1.0)],
        Filter::GradientDynamic { position, .. } => vec![("position", "Gradient Input Position", *position, 0.0, 1.0)],
        Filter::Invert
        | Filter::InvertOff
        | Filter::ColorBalance { .. }
        | Filter::ColorCorrect { .. }
        | Filter::NormalInvert
        | Filter::Fxaa { .. }
        | Filter::PbrValidate(_)
        | Filter::Transform(_)
        | Filter::Empty => Vec::new(),
    }
}

/// Grayscale Conversion's sliders: Balance and Contrast, then the Channels Weights' Red, Green, Blue and Alpha in that mode
fn conversion_params(conversion: &GrayscaleConversion) -> Vec<(&'static str, &'static str, f32, f32, f32)> {
    let mut params = vec![("balance", "Balance", conversion.balance, 0.0, 1.0), ("contrast", "Contrast", conversion.contrast, 0.0, 1.0)];
    if conversion.mode == ConversionMode::ChannelsWeights {
        let [red, green, blue, alpha] = conversion.weights;
        params.extend([
            ("red", "Red", red, 0.0, 1.0),
            ("green", "Green", green, 0.0, 1.0),
            ("blue", "Blue", blue, 0.0, 1.0),
            ("alpha", "Alpha", alpha, 0.0, 1.0),
        ]);
    }
    params
}

/// The keys `effect.set` takes for Grayscale Conversion
const CONVERSION_KEYS: [&str; 9] = ["channelInput", "invert", "balance", "contrast", "mode", "red", "green", "blue", "alpha"];

/// Set the Grayscale Conversion settings given in `p`: channelInput (currentChannel or customInput), invert, balance, contrast, mode and the red, green, blue and alpha weights
fn apply_conversion(p: &Value, conversion: &mut GrayscaleConversion, cmd: &str) -> Result<()> {
    if p.get("channelInput").is_some() {
        conversion.custom_input = match strp(p, "channelInput").unwrap_or_default() {
            "currentChannel" => false,
            "customInput" => true,
            _ => return Err(bad(cmd, "`channelInput` must be currentChannel or customInput")),
        };
    }
    if p.get("invert").is_some() {
        conversion.invert = boolp(p, "invert").ok_or_else(|| bad(cmd, "`invert` must be true or false"))?;
    }
    conversion.mode = choice(p, "mode", &ConversionMode::ALL, ConversionMode::id, cmd)?.unwrap_or(conversion.mode);
    let [red, green, blue, alpha] = &mut conversion.weights;
    for (key, slot) in
        [("balance", &mut conversion.balance), ("contrast", &mut conversion.contrast), ("red", red), ("green", green), ("blue", blue), ("alpha", alpha)]
    {
        if let Some(v) = f32p(p, key) {
            *slot = v.clamp(0.0, 1.0);
        }
    }
    Ok(())
}

/// Make a gradient use its first `wanted` colors: an evenly spaced ramp keeps its places from now on, and each added color starts as the last one at 1
fn set_quantity(colors: &mut Vec<[f32; 3]>, positions: &mut Vec<f32>, quantity: &mut Option<u8>, wanted: u8) {
    let used = quantity.map_or(colors.len(), usize::from).min(colors.len());
    if positions.is_empty() {
        let last = used.saturating_sub(1).max(1) as f32;
        *positions = (0..used).map(|index| index as f32 / last).collect();
    }
    let wanted_len = usize::from(wanted);
    let fill = colors.get(used.saturating_sub(1)).copied().unwrap_or([1.0; 3]);
    if colors.len() < wanted_len {
        colors.resize(wanted_len, fill);
    }
    if positions.len() < wanted_len {
        positions.resize(wanted_len, 1.0);
    }
    *quantity = Some(wanted);
}

/// Set the Mirror settings given in `p`: displayDebug, mode, axisXOffset, axisYOffset, invertAxisX, invertAxisY and cornerType
fn apply_mirror(p: &Value, mirror: &mut Mirror, cmd: &str) -> Result<()> {
    if p.get("mode").is_some() {
        let id = strp(p, "mode").unwrap_or_default();
        mirror.mode = MirrorMode::ALL.into_iter().find(|m| m.id() == id).ok_or_else(|| bad(cmd, "`mode` must be axisX, axisY or corner"))?;
    }
    if p.get("cornerType").is_some() {
        let id = strp(p, "cornerType").unwrap_or_default();
        mirror.corner = MirrorCorner::ALL
            .into_iter()
            .find(|c| c.id() == id)
            .ok_or_else(|| bad(cmd, "`cornerType` must be topLeft, topRight, bottomLeft or bottomRight"))?;
    }
    let [offset_x, offset_y] = &mut mirror.offset;
    for (key, slot) in [("axisXOffset", offset_x), ("axisYOffset", offset_y)] {
        if let Some(v) = f32p(p, key) {
            *slot = v.clamp(0.0, 1.0);
        }
    }
    let [invert_x, invert_y] = &mut mirror.invert;
    for (key, slot) in [("displayDebug", &mut mirror.debug), ("invertAxisX", invert_x), ("invertAxisY", invert_y)] {
        if p.get(key).is_some() {
            *slot = boolp(p, key).ok_or_else(|| bad(cmd, format!("`{key}` must be true or false")))?;
        }
    }
    Ok(())
}

/// The choice among `all` whose id (`id`) `p` gives under `key`, if it gives one; an unknown id is refused
fn choice<T: Copy>(p: &Value, key: &str, all: &[T], id: fn(T) -> &'static str, cmd: &str) -> Result<Option<T>> {
    if p.get(key).is_none() {
        return Ok(None);
    }
    let given = strp(p, key).unwrap_or_default();
    let found = all.iter().copied().find(|choice| id(*choice) == given);
    found.map(Some).ok_or_else(|| {
        let ids: Vec<&str> = all.iter().map(|choice| id(*choice)).collect();
        bad(cmd, format!("`{key}` must be {}", ids.join(", ")))
    })
}

/// Set the parameters of `f` given in `p` (clamped to their ranges); `effect.set` uses it too
pub(crate) fn apply(p: &Value, f: &mut Filter, cmd: &str) -> Result<()> {
    let set = |slot: &mut f32, key: &str, lo: f32, hi: f32| {
        if let Some(v) = f32p(p, key) {
            *slot = v.clamp(lo, hi);
        }
    };
    let seed = |slot: &mut u32| {
        if let Some(v) = u64p(p, "seed") {
            *slot = u32::try_from(v).unwrap_or(u32::MAX);
        }
    };
    let divider = |slot: &mut u16| -> Result<()> {
        if p.get("divider").is_some() {
            let value = u64p(p, "divider").and_then(|value| u16::try_from(value).ok()).filter(|value| Filter::DIVIDERS.contains(value));
            *slot = value.ok_or_else(|| bad(cmd, "`divider` must be 1, 10, 100 or 1000"))?;
        }
        Ok(())
    };
    let wrap = |slot: &mut bool| -> Result<()> {
        if let Some(value) = p.get("wrap") {
            *slot = value.as_bool().ok_or_else(|| bad(cmd, "`wrap` must be true or false"))?;
        }
        Ok(())
    };
    // A blur's Intensity in the reference workflow's unit (radius in texels at 2K ÷ Filter::BLUR_INTENSITY)
    let blur_intensity = |radius: &mut f32| {
        if let Some(intensity) = f32p(p, "intensity") {
            *radius = (intensity * Filter::BLUR_INTENSITY).clamp(0.0, 256.0);
        }
    };
    let switch = |slot: &mut bool, key: &str| -> Result<()> {
        if p.get(key).is_some() {
            *slot = boolp(p, key).ok_or_else(|| bad(cmd, format!("`{key}` must be true or false")))?;
        }
        Ok(())
    };
    let shade = |slot: &mut [f32; 3]| -> Result<()> {
        if p.get("color").is_some() {
            let v = color_param(cmd, p, "color")?.ok_or_else(|| bad(cmd, "`color` must be a color ([r, g, b] linear or \"#rrggbb\")"))?;
            *slot = v.map(|x| x.clamp(0.0, 1.0));
        }
        Ok(())
    };
    let angle = |slot: &mut f32| {
        if let Some(v) = f32p(p, "angle") {
            *slot = v.rem_euclid(360.0);
        }
    };
    match f {
        Filter::Invert | Filter::InvertOff => {
            if p.get("invert").is_some() {
                let on = boolp(p, "invert").ok_or_else(|| bad(cmd, "`invert` must be true or false"))?;
                *f = if on { Filter::Invert } else { Filter::InvertOff };
            }
        }
        Filter::NormalInvert | Filter::Empty => {}
        // A Grayscale saved before the settings takes its luminance weights on its first edit, so it looks the same
        Filter::Grayscale => {
            if CONVERSION_KEYS.iter().any(|key| p.get(key).is_some()) {
                let mut conversion = GrayscaleConversion::of_luminance();
                apply_conversion(p, &mut conversion, cmd)?;
                *f = Filter::GrayscaleConversion(conversion);
            }
        }
        Filter::GrayscaleConversion(conversion) => apply_conversion(p, conversion, cmd)?,
        Filter::AnisotropicKuwahara(kuwahara) => {
            if p.get("extractDirection").is_some() {
                kuwahara.custom_input = match strp(p, "extractDirection").unwrap_or_default() {
                    "currentChannel" => false,
                    "customInput" => true,
                    _ => return Err(bad(cmd, "`extractDirection` must be currentChannel or customInput")),
                };
            }
            set(&mut kuwahara.radius, "radius", 0.0, AnisotropicKuwahara::MAX_RADIUS);
            set(&mut kuwahara.smoothness, "smoothness", 0.0, 1.0);
            set(&mut kuwahara.sharpness, "sharpness", 0.25, 1.0);
            set(&mut kuwahara.tensor_smoothness, "tensorSmoothness", 0.0, 1.0);
            set(&mut kuwahara.anisotropy, "anisotropy", 0.0, 1.0);
            angle(&mut kuwahara.angle);
        }
        Filter::PbrValidate(validate) => {
            validate.mode = choice(p, "validationMode", &ValidationMode::ALL, ValidationMode::id, cmd)?.unwrap_or(validate.mode);
            for (key, slot, allowed) in [
                ("albedoDarkRangeThreshold", &mut validate.albedo_dark, PbrValidate::ALBEDO_DARK),
                ("metalReflectanceRange", &mut validate.metal_low, PbrValidate::METAL_LOW),
            ] {
                if p.get(key).is_some() {
                    let value = u64p(p, key).and_then(|value| u8::try_from(value).ok()).filter(|value| allowed.contains(value));
                    *slot = value.ok_or_else(|| bad(cmd, format!("`{key}` must be {} or {}", allowed[0], allowed[1])))?;
                }
            }
        }
        Filter::HeightToNormal(filter) => {
            switch(&mut filter.overwrite, "overwriteExistingNormal")?;
            switch(&mut filter.world_units, "useWorldUnits")?;
            set(&mut filter.surface_size, "surfaceSize", 1.0, HeightToNormal::MAX_SURFACE_SIZE);
            set(&mut filter.height_depth, "heightDepth", 0.0, HeightToNormal::MAX_HEIGHT_DEPTH);
            set(&mut filter.intensity, "normalIntensity", 0.0, HeightToNormal::MAX_INTENSITY);
        }
        Filter::ColorMatch(matcher) => {
            matcher.source_mode = choice(p, "sourceColorMode", &SourceColorMode::ALL, SourceColorMode::id, cmd)?.unwrap_or(matcher.source_mode);
            matcher.target_mode = choice(p, "targetColorMode", &TargetColorMode::ALL, TargetColorMode::id, cmd)?.unwrap_or(matcher.target_mode);
            for (key, slot) in [("sourceColor", &mut matcher.source_color), ("targetColor", &mut matcher.target_color)] {
                if p.get(key).is_some() {
                    let v = color_param(cmd, p, key)?.ok_or_else(|| bad(cmd, format!("`{key}` must be a color ([r, g, b] linear or \"#rrggbb\")")))?;
                    *slot = v.map(|x| x.clamp(0.0, 1.0));
                }
            }
            switch(&mut matcher.custom_variation, "customColorVariation")?;
            set(&mut matcher.hue, "hue", 0.0, 360.0);
            set(&mut matcher.chroma, "chroma", 0.0, 1.0);
            set(&mut matcher.luma, "luma", 0.0, 1.0);
        }
        Filter::Threshold { level, mode } => {
            set(level, "level", 0.0, 1.0);
            if p.get("mode").is_some() {
                let id = strp(p, "mode").unwrap_or_default();
                *mode = ThresholdMode::ALL
                    .into_iter()
                    .find(|m| m.id() == id)
                    .ok_or_else(|| bad(cmd, "`mode` must be greater, greaterOrEqual, lower or lowerOrEqual"))?;
            }
        }
        Filter::Mirror(mirror) => apply_mirror(p, mirror, cmd)?,
        Filter::Transform(transform) => crate::tile_transform_params::apply(p, transform, cmd)?,
        Filter::SourceWarp(warp) => {
            seed(&mut warp.seed);
            warp.mode = choice(p, "warpMode", &WarpMode::ALL, WarpMode::id, cmd)?.unwrap_or(warp.mode);
            set(&mut warp.intensity, "intensity", 0.0, 1.0);
            divider(&mut warp.divider)?;
            angle(&mut warp.angle);
            if p.get("directions").is_some() {
                let count = u64p(p, "directions").and_then(|count| u8::try_from(count).ok()).filter(|count| SourceWarp::DIRECTIONS.contains(count));
                warp.directions = count.ok_or_else(|| bad(cmd, "`directions` must be 1, 2 or 4"))?;
            }
            warp.blending = choice(p, "blendingMode", &WarpBlending::ALL, WarpBlending::id, cmd)?.unwrap_or(warp.blending);
            warp.source.mode = choice(p, "sourceMode", &WarpSourceMode::ALL, WarpSourceMode::id, cmd)?.unwrap_or(warp.source.mode);
            set(&mut warp.source.blur, "sourceBlur", 0.0, 1.0);
            set(&mut warp.source.balance, "sourceBalance", 0.0, 1.0);
            set(&mut warp.source.contrast, "sourceContrast", 0.0, 1.0);
            set(&mut warp.source.tiling, "sourceTiling", 0.01, 16.0);
        }
        Filter::SourceSlopeBlur(slope) => {
            seed(&mut slope.seed);
            set(&mut slope.intensity, "intensity", 0.0, 1.0);
            divider(&mut slope.divider)?;
            slope.blending = choice(p, "blendingMode", &SlopeBlending::ALL, SlopeBlending::id, cmd)?.unwrap_or(slope.blending);
            set(&mut slope.quality, "quality", 0.0, 1.0);
            slope.source.mode = choice(p, "sourceMode", &WarpSourceMode::ALL, WarpSourceMode::id, cmd)?.unwrap_or(slope.source.mode);
            set(&mut slope.source.blur, "sourceBlur", 0.0, 1.0);
            set(&mut slope.source.balance, "sourceBalance", 0.0, 1.0);
            set(&mut slope.source.contrast, "sourceContrast", 0.0, 1.0);
            set(&mut slope.source.tiling, "sourceTiling", 0.01, 16.0);
        }
        Filter::DistanceBevel(bevel) => {
            set(&mut bevel.distance, "distance", -1.0, 1.0);
            set(&mut bevel.smoothing, "smoothing", 0.0, 5.0);
            bevel.corner = choice(p, "cornerType", &BevelCorner::ALL, BevelCorner::id, cmd)?.unwrap_or(bevel.corner);
            bevel.blur = choice(p, "blurMode", &BevelBlur::ALL, BevelBlur::id, cmd)?.unwrap_or(bevel.blur);
            wrap(&mut bevel.wrap)?;
        }
        Filter::BevelSmooth(smooth) => {
            smooth.direction = choice(p, "direction", &BevelDirection::ALL, BevelDirection::id, cmd)?.unwrap_or(smooth.direction);
            set(&mut smooth.distance, "distance", 0.0, 0.3);
            set(&mut smooth.smoothing, "smoothing", 0.0, 10.0);
            set(&mut smooth.curve_offset, "curveOffset", -1.0, 1.0);
            set(&mut smooth.curve_shape, "curveShape", -1.0, 1.0);
            set(&mut smooth.threshold, "maskThreshold", 0.0, 1.0);
            set(&mut smooth.multiplier, "distanceMapMultiplier", 0.0, 1.0);
            wrap(&mut smooth.wrap)?;
        }
        Filter::DirectionalDistance(trail) => {
            set(&mut trail.distance, "distance", 0.0, 0.3);
            angle(&mut trail.angle);
            set(&mut trail.contrast, "contrast", 0.0, 1.0);
            set(&mut trail.multiplier, "distanceMapMultiplier", 0.0, 1.0);
        }
        Filter::MaskOutline(outline) => {
            if p.get("maskPosition").is_some() {
                let id = strp(p, "maskPosition").unwrap_or_default();
                outline.position = OutlinePosition::ALL
                    .into_iter()
                    .find(|position| position.id() == id)
                    .ok_or_else(|| bad(cmd, "`maskPosition` must be outside, inside or both"))?;
            }
            if p.get("startFromBlack").is_some() {
                outline.from_black = boolp(p, "startFromBlack").ok_or_else(|| bad(cmd, "`startFromBlack` must be true or false"))?;
            }
            set(&mut outline.threshold, "threshold", 0.0, 1.0);
            set(&mut outline.width, "width", 0.0, 1.0);
            set(&mut outline.blur, "blur", 0.0, 1.0);
            set(&mut outline.curve, "curveShape", -1.0, 1.0);
        }
        Filter::Blur { radius, wrap: w } => {
            set(radius, "radius", 0.0, 256.0);
            blur_intensity(radius);
            wrap(w)?;
        }
        Filter::Sharpen { amount } => set(amount, "amount", 0.0, 10.0),
        Filter::Hsl { hue, saturation, lightness, perceptive } => {
            if let Some(mode) = p.get("mode") {
                *perceptive = match mode.as_str() {
                    Some("perceptive") => true,
                    Some("regular") => false,
                    _ => return Err(bad(cmd, "`mode` must be \"regular\" or \"perceptive\"")),
                };
            }
            set(hue, "hue", -1.0, 1.0);
            set(saturation, "saturation", -1.0, 1.0);
            set(lightness, "lightness", -1.0, 1.0);
        }
        Filter::ContrastLuminosity { contrast, luminosity } => {
            set(contrast, "contrast", -1.0, 1.0);
            set(luminosity, "luminosity", -1.0, 1.0);
        }
        Filter::ColorCorrect { shadows, midtones, highlights } => {
            for (key, slot) in [("shadows", shadows), ("midtones", midtones), ("highlights", highlights)] {
                if p.get(key).is_some() {
                    let v = p.get(key).and_then(color_of).ok_or_else(|| bad(cmd, format!("`{key}` must be [contrast, luminosity, saturation]")))?;
                    *slot = [v[0].clamp(-1.0, 1.0), v[1].clamp(-1.0, 1.0), v[2].clamp(0.0, 1.0)];
                }
            }
        }
        Filter::ColorBalance { shadows, midtones, highlights } => {
            for (key, slot) in [("shadows", shadows), ("midtones", midtones), ("highlights", highlights)] {
                if let Some(val) = p.get(key).filter(|v| !v.is_null()) {
                    let v = color_of(val).ok_or_else(|| bad(cmd, format!("`{key}` must be [r, g, b] offsets in -1..1")))?;
                    *slot = v.map(|x| x.clamp(-1.0, 1.0));
                }
            }
        }
        Filter::GradientMap { colors, positions, quantity, input, linear, debug } => {
            if let Some(list) = p.get("colors") {
                let parsed: Option<Vec<[f32; 3]>> = list.as_array().and_then(|a| a.iter().map(|c| color_of(c).map(|v| v.map(|x| x.clamp(0.0, 1.0)))).collect());
                match parsed {
                    Some(c) if (1..=16).contains(&c.len()) => *colors = c,
                    _ => return Err(bad(cmd, "`colors` must be 1 to 16 colors ([r, g, b] linear or \"#rrggbb\")")),
                }
            }
            if let Some(list) = p.get("positions") {
                let parsed: Option<Vec<f32>> = list.as_array().and_then(|a| a.iter().map(|v| v.as_f64().map(|v| (v as f32).clamp(0.0, 1.0))).collect());
                match parsed {
                    Some(parsed) if parsed.len() <= colors.len() => *positions = parsed,
                    _ => return Err(bad(cmd, "`positions` must be numbers in 0..1, at most one per color")),
                }
            }
            if p.get("quantity").is_some() {
                let wanted = u64p(p, "quantity").and_then(|q| u8::try_from(q).ok()).filter(|q| (2..=Filter::MAX_GRADIENT_COLORS).contains(q));
                let wanted = wanted.ok_or_else(|| bad(cmd, "`quantity` must be a whole number from 2 to 10"))?;
                set_quantity(colors, positions, quantity, wanted);
            }
            if let Some(mode) = p.get("grayscaleMode") {
                let id = mode.as_str().unwrap_or_default();
                input.mode = GrayscaleMode::ALL
                    .into_iter()
                    .find(|m| m.id() == id)
                    .ok_or_else(|| bad(cmd, "`grayscaleMode` must be desaturation, luma, average, max or min"))?;
            }
            set(&mut input.blur, "blur", 0.0, 16.0);
            set(&mut input.balance, "balance", 0.0, 1.0);
            set(&mut input.contrast, "contrast", 0.0, 1.0);
            for (key, slot) in [("invert", &mut input.invert), ("linear", linear), ("debug", debug)] {
                if p.get(key).is_some() {
                    *slot = boolp(p, key).ok_or_else(|| bad(cmd, format!("`{key}` must be true or false")))?;
                }
            }
        }
        Filter::GradientCurve { preset, invert, balance, contrast } => {
            if p.get("preset").is_some() {
                let id = strp(p, "preset").unwrap_or_default();
                *preset = CurvePreset::ALL.into_iter().find(|c| c.id() == id).ok_or_else(|| bad(cmd, format!("unknown `preset` `{id}`")))?;
            }
            if p.get("invert").is_some() {
                *invert = boolp(p, "invert").ok_or_else(|| bad(cmd, "`invert` must be true or false"))?;
            }
            set(balance, "balance", 0.0, 1.0);
            set(contrast, "contrast", 0.0, 1.0);
        }
        Filter::GradientDynamic { vertical, position, color } => {
            if p.get("orientation").is_some() {
                *vertical = match strp(p, "orientation") {
                    Some("horizontal") => false,
                    Some("vertical") => true,
                    _ => return Err(bad(cmd, "`orientation` must be \"horizontal\" or \"vertical\"")),
                };
            }
            set(position, "position", 0.0, 1.0);
            shade(color)?;
        }
        Filter::Posterize { levels, alpha } => {
            set(levels, "levels", 2.0, 64.0);
            switch(alpha, "applyToAlpha")?;
        }
        Filter::Clamp { min, max } | Filter::Smoothstep { min, max } => {
            set(min, "min", 0.0, 1.0);
            set(max, "max", 0.0, 1.0);
        }
        Filter::HeightAdjust { invert, offset, multiply } => {
            switch(invert, "invert")?;
            set(offset, "offset", -1.0, 1.0);
            set(multiply, "multiply", 0.0, 10.0);
        }
        Filter::HistogramScan { position, contrast, invert_position } => {
            set(position, "position", 0.0, 1.0);
            set(contrast, "contrast", 0.0, 1.0);
            switch(invert_position, "invertPosition")?;
        }
        Filter::HistogramRange { range, position } => {
            set(range, "range", 0.0, 1.0);
            set(position, "position", 0.0, 1.0);
        }
        Filter::HistogramShift { offset } => {
            set(offset, "offset", -1.0, 1.0);
            if let Some(position) = f32p(p, "position") {
                *offset = Filter::shift_offset(position);
            }
        }
        Filter::Warp { intensity, scale, seed: s, divider: d } => {
            set(intensity, "intensity", 0.0, 256.0);
            set(scale, "scale", 0.01, 1000.0);
            seed(s);
            divider(d)?;
        }
        Filter::DirectionalBlur { radius, angle: a } => {
            set(radius, "radius", 0.0, 256.0);
            blur_intensity(radius);
            angle(a);
        }
        Filter::SlopeBlur { radius, scale, seed: s, divider: d } => {
            set(radius, "radius", 0.0, 256.0);
            set(scale, "scale", 0.01, 1000.0);
            seed(s);
            divider(d)?;
        }
        Filter::HighPass { radius, space, alpha } => {
            set(radius, "radius", 0.0, 256.0);
            blur_intensity(radius);
            *space = choice(p, "colorSpace", &HighpassSpace::ALL, HighpassSpace::id, cmd)?.unwrap_or(*space);
            switch(alpha, "applyToAlpha")?;
        }
        Filter::EdgeDetect { width } => set(width, "width", 0.0, 256.0),
        Filter::Bevel { width, wrap: w } => {
            set(width, "width", 0.0, 256.0);
            wrap(w)?;
        }
        Filter::Mosaic { size, alpha } => {
            set(size, "size", 1.0, 2048.0);
            if let Some(amount) = f32p(p, "pixelsAmount") {
                *size = Filter::pixel_size(amount.round());
            }
            switch(alpha, "applyToAlpha")?;
        }
        Filter::Fxaa { alpha } => switch(alpha, "affectAlphaChannel")?,
        Filter::Glow { radius, amount } => {
            set(radius, "radius", 0.0, 256.0);
            set(amount, "amount", 0.0, 10.0);
        }
        Filter::HaloGlow { on_mask, precise, width, spread, opacity, color } => {
            if p.get("usedOnMask").is_some() {
                *on_mask = boolp(p, "usedOnMask").ok_or_else(|| bad(cmd, "`usedOnMask` must be true or false"))?;
            }
            if p.get("mode").is_some() {
                *precise = match strp(p, "mode") {
                    Some("soft") => false,
                    Some("precise") => true,
                    _ => return Err(bad(cmd, "`mode` must be \"soft\" or \"precise\"")),
                };
            }
            set(width, "width", -1.0, 1.0);
            set(spread, "spread", 0.0, 1.0);
            set(opacity, "opacity", 0.0, 1.0);
            shade(color)?;
        }
        Filter::DropShadow { angle: a, distance, size, spread, opacity, color } => {
            angle(a);
            set(distance, "distance", -0.5, 0.5);
            set(size, "size", 0.0, 1.0);
            set(spread, "spread", 0.0, 1.0);
            set(opacity, "opacity", 0.0, 1.0);
            shade(color)?;
        }
        Filter::ReplaceColor { from, to, tolerance, softness } => {
            for (key, slot) in [("from", from), ("to", to)] {
                if let Some(v) = color_param(cmd, p, key)? {
                    *slot = v.map(|x| x.clamp(0.0, 1.0));
                }
            }
            set(tolerance, "tolerance", 0.0, 1.0);
            set(softness, "softness", 0.0, 1.0);
        }
        Filter::BakedLighting { azimuth, elevation, light, ambient, occlusion, stylized } => {
            if let Some(v) = f32p(p, "azimuth") {
                *azimuth = v.rem_euclid(360.0);
            }
            // A light past the zenith comes from the far side: the angle wraps like the horizontal one
            if let Some(v) = f32p(p, "elevation") {
                *elevation = v.rem_euclid(360.0);
            }
            set(light, "light", 0.0, 10.0);
            set(ambient, "ambient", 0.0, 10.0);
            set(occlusion, "occlusion", 0.0, 1.0);
            // A Baked Lighting saved before the stylized settings takes them lit as it was
            if crate::stylized_lighting_params::sets_any(p) {
                crate::stylized_lighting_params::apply(p, stylized.get_or_insert_with(|| Box::new(StylizedLighting::PLAIN)), cmd)?;
            }
        }
        Filter::EnvironmentLighting { horizontal, vertical, exposure, height_intensity, ambient_occlusion, specular_occlusion } => {
            for (key, slot) in [("horizontalRotation", horizontal), ("verticalRotation", vertical)] {
                if let Some(v) = f32p(p, key) {
                    *slot = v.rem_euclid(360.0);
                }
            }
            set(exposure, "exposure", -5.0, 5.0);
            set(height_intensity, "heightIntensity", 0.0, 8.0);
            set(ambient_occlusion, "ambientOcclusion", 0.0, 1.0);
            set(specular_occlusion, "specularOcclusion", 0.0, 1.0);
        }
        Filter::DirectionalWarp { intensity, angle: a, scale, seed: s, divider: d } => {
            set(intensity, "intensity", 0.0, 256.0);
            angle(a);
            set(scale, "scale", 0.01, 1000.0);
            seed(s);
            divider(d)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_filter_builds_and_finds_by_id_or_label() {
        for f in FILTERS {
            let kind = build(f.id, &Value::Null).unwrap_or_else(|e| panic!("{}: {e}", f.id));
            // Material finishes are content only; other filters also run on masks, where some
            // (normal invert) change nothing and are not offered.
            assert_eq!(kind.content_only().is_none(), f.mask || matches!(kind, EffectKind::Filter(_)), "{}", f.id);
            assert_eq!(find(f.label).map(|d| d.id), Some(f.id));
        }
        assert!(build("nope", &Value::Null).is_err());
        let EffectKind::Filter(Filter::Posterize { levels, .. }) = build("posterize", &json!({"levels": 1000})).unwrap() else { panic!() };
        assert_eq!(levels, 64.0);
        assert!(build("gradientMap", &json!({"colors": []})).is_err());
        assert!(build("colorBalance", &json!({"shadows": "x"})).is_err());
        let EffectKind::Filter(Filter::GradientMap { colors, .. }) = build("gradientMap", &json!({"colors": [[0, 0, 0], "#ffffff"]})).unwrap() else {
            panic!()
        };
        assert_eq!(colors.len(), 2);
    }

    #[test]
    fn every_filter_row_reads_a_reference_name_without_an_internal_unit() {
        let old_saves = [
            Filter::Warp { intensity: 8.0, scale: 4.0, seed: 1, divider: 1 },
            Filter::SlopeBlur { radius: 8.0, scale: 4.0, seed: 1, divider: 1 },
            Filter::DirectionalWarp { intensity: 8.0, angle: 0.0, scale: 4.0, seed: 1, divider: 1 },
            Filter::EdgeDetect { width: 2.0 },
            Filter::Bevel { width: 8.0, wrap: false },
            Filter::Glow { radius: 16.0, amount: 1.0 },
        ];
        let library = FILTERS.iter().filter_map(|def| match build(def.id, &Value::Null) {
            Ok(EffectKind::Filter(filter)) => Some(filter),
            _ => None,
        });
        for filter in old_saves.into_iter().chain(library) {
            for (key, label, ..) in scalar_params(&filter) {
                let title_case = label.split([' ', '/']).filter(|word| !word.is_empty()).all(|word| word.starts_with(|c: char| !c.is_lowercase()));
                assert!(title_case && !label.contains("texels") && !label.contains("degrees"), "{key}: {label}");
            }
        }
    }
}
