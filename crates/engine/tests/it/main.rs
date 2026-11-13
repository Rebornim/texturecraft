//! One integration-test executable avoids linking the same dependencies for every feature.
//! Keep feature tests in their own modules; the standalone session.rs target remains available.

mod add_mask_items;
mod bake;
mod bake_resolution;
mod bake_settings;
mod bake_tiles;
mod color_selection;
mod compare_mask;
mod content_effects;
mod effect_blending;
mod export_colour_space;
mod fill_alpha_source;
mod fill_patterns;
mod fill_projections;
mod fill_sampling;
mod filter_swap;
mod filters;
mod folder_contents;
mod high_poly_missed;
mod instances;
mod mask_fill_source;
mod mask_paint_image;
mod materialcraft_link;
mod materialx;
mod materialx_import;
mod meshmap_import;
mod painter_gaps;
mod path_perf;
mod path_tool;
mod perf;
mod polygon_fill;
mod project_recovery;
mod sample;
mod thumbs;
mod tools;
mod udim;
mod uv_strokes;

mod fill_whole_material;
mod filter_card;
mod filter_colour;
mod swatches;
mod symmetry;
