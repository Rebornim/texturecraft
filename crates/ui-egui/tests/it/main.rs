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
mod fill_support;
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
mod popup_press_close;
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

use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use texturecraft_ui_egui::TextureCraftApp;

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

/// One wheel notch over the view, in points
const WHEEL_NOTCH: f32 = 60.0;

/// How far left of a list's button a click lands, clear of the list and its popup
const OUTSIDE_OFFSET: f32 = 400.0;

/// A Properties list that `open` opens (returning its button's rect) leaves the view its wheel, and closes on Escape and on a click outside
fn assert_list_leaves_the_wheel_and_closes(
    harness: &mut egui_kittest::Harness<'_, texturecraft_ui_egui::TextureCraftApp>,
    open: impl Fn(&mut egui_kittest::Harness<'_, texturecraft_ui_egui::TextureCraftApp>) -> egui::Rect,
) {
    open(harness);
    assert!(egui::Popup::is_any_open(&harness.ctx), "the list opens");
    let distance = harness.state().session.camera.distance;
    harness.hover_at(harness.state().viewport_rect.center());
    harness.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, WHEEL_NOTCH),
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Wheel,
        modifiers: egui::Modifiers::NONE,
    });
    harness.run_steps(REDRAW_FRAMES);
    assert_ne!(harness.state().session.camera.distance, distance, "the wheel over the view zooms with the list open");

    harness.key_press(egui::Key::Escape);
    harness.run_steps(SETTLE_FRAMES);
    assert!(!egui::Popup::is_any_open(&harness.ctx), "Escape closes the list");

    let button = open(harness);
    let outside = egui::pos2(button.min.x - OUTSIDE_OFFSET, button.min.y);
    harness.hover_at(outside);
    for pressed in [true, false] {
        harness.event(egui::Event::PointerButton { pos: outside, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE });
        harness.run_steps(DRAG_STEP_FRAMES);
    }
    assert!(!egui::Popup::is_any_open(&harness.ctx), "a click outside closes the list");
}

/// Presses and releases the primary button at `at` without moving, as a pointer click does
fn click_at(harness: &mut Harness<'_, TextureCraftApp>, at: egui::Pos2) {
    harness.hover_at(at);
    for pressed in [true, false] {
        harness.event(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE });
        harness.run_steps(1);
    }

    harness.run_steps(SETTLE_FRAMES);
}

/// Clicks the middle of the widget with `role` and `label` through the pointer
fn click_widget(harness: &mut Harness<'_, TextureCraftApp>, role: egui::Role, label: &str) {
    let at = harness.get_by_role_and_label(role, label).rect().center();
    click_at(harness, at);
}

/// The close button nearest the right of a docked panel's `title` (its tab or header title), on the same row
fn panel_close_button(harness: &Harness<'_, TextureCraftApp>, title: egui::Rect) -> egui::Rect {
    harness
        .get_all_by_role(egui::Role::Button)
        .filter(|button| button.accesskit_node().label().as_deref() == Some("Close Panel"))
        .map(|button| button.rect())
        .filter(|close| close.min.x > title.max.x && title.y_range().contains(close.center().y))
        .min_by(|a, b| a.min.x.total_cmp(&b.min.x))
        .expect("a close button right of the title")
}
