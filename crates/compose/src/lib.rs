//! # texturecraft-compose
//!
//! The CPU layer-stack evaluator: [`eval::composite_tiles`] turns a texture set's layers, masks,
//! fills and painted pixels into one image per channel, tile by tile in parallel, including the
//! live preview of a stroke being drawn. [`display`] converts composited tiles into the texture
//! formats the viewport samples; [`pattern`] evaluates procedurals through projections.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod baked_maps;
pub mod below;
mod curvature_size_level;
pub mod curvature_sizes;
pub mod display;
mod dripping_rust;
pub mod eval;
mod generator_maps;
mod held_fields;
mod histogram_scan;
mod image_identity;
mod micro_details;
pub mod pattern;
mod sections;
pub mod store;
mod texture_layer;
mod three_d_distance;
pub mod thumb;
mod uv_border_distance;
mod wear;
mod wear_inputs;
mod wear_scan;
mod working_field;

pub use below::{BelowCache, StackKey};
pub use display::{DisplayKind, tile_bytes};
pub use eval::{
    BLOCK, Composite, Inputs, PaintTarget, Stencil, StrokePreview, all_tiles, composite_tiles, composite_tiles_cached, covered_tiles, effect_histogram,
    flatten_layer, mask_at, mask_image, pad_tiles,
};
pub use store::{BakedMaps, ImageRes, ImageStore, PixelStore};
pub use thumb::{THUMB_TEXELS, ThumbImage, ThumbTarget};
