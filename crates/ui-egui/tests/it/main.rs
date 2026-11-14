//! One integration-test executable avoids linking the same dependencies for every feature.
//! Keep feature tests in their own modules; the standalone ui_snapshots.rs target remains available.

mod anchor_point;
mod bake_job_mesh;
mod bake_mode;
mod brush_drag;
mod brush_section_reset;
mod close_unsaved;
mod color_picker;
mod color_selection_editor;
mod compare_mask_rows;
mod curvature_generator;
mod effect_rows;
mod effect_sliders;
mod file_dialogs;
mod fill_channel_image;
mod fill_material_rows;
mod fill_projection_rows;
mod fill_symmetry_rows;
mod filter_picker;
mod folder_properties;
mod generator_panel;
mod history_panel;
mod instance_properties;
mod layer_clipboard;
mod layer_rename;
mod layer_toolbar;
mod layers_panel;
mod levels_editor;
mod mask_fill_rows;
mod mask_paint_grayscale;
mod mask_value_swap;
mod new_project_baked_maps;
mod panel_header;
mod path_in_view;
mod path_list;
mod pen_pressure;
mod polygon_fill;
mod project_config;
mod properties_min_width;
mod properties_popup;
mod properties_scroll;
mod rail;
mod shelf_drop;
mod shelf_hover_preview;
mod shelf_panel;
mod shift_lines_2d;
mod shortcut_keys;
mod skew_paint;
mod source_picker;
mod split_divider;
mod start_window;
mod tool_availability;
mod toolbar;

mod filter_card;
mod filter_colour;
mod filter_gradient;
mod filter_inputs;
mod filter_light;
mod filter_shape;
mod filter_simple;
mod filter_stylize;
mod filter_support;
mod filter_transform;
mod symmetry;

/// Window of the Properties tests: tall enough that a whole sheet shows without scrolling
const PROPERTIES_WINDOW: egui::Vec2 = egui::vec2(1600.0, 1400.0);

// Small texture and bake sizes keep the sample session fast; the panels don't depend on them
const TEXTURE_RESOLUTION: u32 = 256;
const BAKE_RESOLUTION: u32 = 64;

/// One frame at 60 Hz, so timed steps run as on a common display
const FRAME_SECONDS: f32 = 1.0 / 60.0;

/// Frames for the first layout and fonts to settle
const STARTUP_FRAMES: usize = 4;

/// Frames for a click or key to reach the panels and its change to show
const SETTLE_FRAMES: usize = 3;

/// Frames between the moves of a drag, and between a press and its release
const DRAG_STEP_FRAMES: usize = 2;

/// How far in from a slider track's ends a drag's points stay, so they land on the track
const SLIDER_END_INSET: f32 = 5.0;

/// Frames for an input that redraws Properties (another selection, source or operator) to show
const REDRAW_FRAMES: usize = 4;
