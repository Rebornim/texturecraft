//! # texturecraft-core
//!
//! The TextureCraft document model: a [`Project`] holds texture sets (one per mesh material),
//! each with a stack of [`Layer`]s (paint, fill, group) whose channels blend with one of
//! the [`BlendMode`]s. Pure data with serde; pixels live in `texturecraft-texel` images
//! owned by the engine. [`blend`] is the CPU reference for every blend mode.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod backface;
pub mod bake;
pub mod bevel;
pub mod bevel_blur;
pub mod bevel_corner;
pub mod bevel_direction;
pub mod bevel_smooth;
pub mod blend;
mod block_scales;
pub mod channel;
pub mod color_selection;
pub mod compare_source;
pub mod contents_offset;
pub mod cook_resolution;
pub mod curve_preset;
pub mod depth_culling;
pub mod directional_distance;
pub mod directional_light;
pub mod fill_symmetry;
pub mod gradient_input;
pub mod grayscale_mode;
pub mod image_input;
pub mod layer;
pub mod lighting_output;
pub mod mask_outline;
pub mod mirror;
pub mod mirror_corner;
pub mod mirror_mode;
pub mod oklab;
pub mod outline_position;
pub mod project;
pub mod projector;
mod saved_projection;
pub mod selection_background;
pub mod shape_crop;
pub mod source_warp;
pub mod stylized_lighting;
pub mod sun_and_sky;
pub mod symmetry_kind;
pub mod threshold_mode;
pub mod warp_blending;
pub mod warp_mode;
pub mod warp_source;
pub mod warp_source_mode;

pub use backface::Backface;
pub use bake::{
    AmbientOcclusionAttenuation, AmbientOcclusionSettings, BackfaceMode, BakeAntialiasing, BakePaintTarget, BakeResolution, BakeSettings, BakedMap,
    BentNormalSettings, CageSettings, CageType, CommonBakeSettings, CurvatureMethod, CurvatureSettings, GroundPlaneSettings, HeightNormalization,
    HeightSettings, HighPolySettings, IdColorGenerator, IdColorSource, IdSettings, MapSettings, MeshCurvatureSettings, MeshMapSettings, MeshMatchMode,
    NormalMapCurvatureSettings, PositionAxis, PositionMode, PositionNormalizationScale, PositionSettings, RayDistribution, RaySettings, SelfOcclusion,
    SkewCorrectionMethod, SkewSettings, ThicknessNormalization, ThicknessSettings,
};
pub use bevel::Bevel;
pub use bevel_blur::BevelBlur;
pub use bevel_corner::BevelCorner;
pub use bevel_direction::BevelDirection;
pub use bevel_smooth::BevelSmooth;
pub use blend::{BlendMode, blend1, blend3, lum, mix3};
pub use channel::{Channel, ChannelFormat, ChannelKind};
pub use color_selection::ColorSelection;
pub use compare_source::CompareSource;
pub use contents_offset::ContentsOffset;
pub use cook_resolution::CookResolution;
pub use curve_preset::CurvePreset;
pub use depth_culling::DepthCulling;
pub use directional_distance::DirectionalDistance;
pub use directional_light::DirectionalLight;
pub use fill_symmetry::FillSymmetry;
pub use gradient_input::GradientInput;
pub use grayscale_mode::GrayscaleMode;
pub use image_input::ImageInput;
pub use layer::{
    ChannelBlend, CompareOp, CurvatureDetail, CurvatureMode, Decal, Effect, EffectKind, Fill, Filter, Filtering, Generator, GeneratorKind, HeightMask, Layer,
    LayerId, LayerKind, Levels, LinkParam, LinkPreset, Mask, MaskBackground, MaterialLink, PathShape, Procedural, ProceduralKind, Projection, Sampling,
    ScaleMode, Source, UvTransform, UvWrap, gradient, hsl_adjust, per_unit,
};
pub use lighting_output::LightingOutput;
pub use mask_outline::MaskOutline;
pub use mirror::Mirror;
pub use mirror_corner::MirrorCorner;
pub use mirror_mode::MirrorMode;
pub use outline_position::OutlinePosition;
pub use project::{
    AoMixing, BlockScales, Camera, DepthOfField, DisplaySettings, FORMAT_VERSION, MAX_RESOLUTION, MAX_SWATCHES, MIN_RESOLUTION, NormalFormat, NormalMixing,
    Project, TextureSet, TileGrid, ToneMapping, UDIM_RANGE, udim_at,
};
pub use projector::Projector;
pub use selection_background::SelectionBackground;
pub use shape_crop::ShapeCrop;
pub use source_warp::SourceWarp;
pub use stylized_lighting::StylizedLighting;
pub use sun_and_sky::SunAndSky;
pub use symmetry_kind::SymmetryKind;
pub use threshold_mode::ThresholdMode;
pub use warp_blending::WarpBlending;
pub use warp_mode::WarpMode;
pub use warp_source::WarpSource;
pub use warp_source_mode::WarpSourceMode;

#[derive(Debug, thiserror::Error, Clone, PartialEq)]
pub enum CoreError {
    #[error("no layer with id {}", .0.0)]
    NoLayer(LayerId),
    #[error("no texture set {0}")]
    NoTextureSet(usize),
    #[error("{0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, CoreError>;

/// Product name for user-facing text.
pub const APP_NAME: &str = "TextureCraft";
/// Native project file extension.
pub const PROJECT_EXT: &str = "tcraft";
