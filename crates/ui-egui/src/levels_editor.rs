//! The levels editor: a histogram with draggable input handles above it, an output gradient
//! with draggable output handles below it, and a row of number fields for exact values.

use egui::accesskit::Role;
use egui::{Color32, CornerRadius, CursorIcon, Id, Pos2, Rect, Response, Sense, Stroke, Ui, WidgetInfo, pos2, vec2};
use texturecraft_engine::core::Levels;

use crate::levels_handle::{GAMMA_RANGE, LevelsHandle};
use crate::theme::Tokens;

const HISTOGRAM_HEIGHT: f32 = 56.0;
const HANDLE_HEIGHT: f32 = 11.0;
const HANDLE_HALF_WIDTH: f32 = 5.0;

/// Height of a handle's straight sides; the rest is its tip pointing at the track
const HANDLE_SIDE_HEIGHT: f32 = 6.0;

/// Extra width on each side of a handle that still grabs it
const HANDLE_GRAB_MARGIN: f32 = 2.0;

const GRADIENT_HEIGHT: f32 = 8.0;
const GAP_BELOW_HISTOGRAM: f32 = 3.0;
const VALUE_ROW_HEIGHT: f32 = 13.0;
const VALUE_FONT_SIZE: f32 = 10.5;

/// Padding of the histogram well around its bars
const WELL_PADDING: f32 = 3.0;

/// Gray of the gamma handle, between the black input-low and white input-high handles
const GAMMA_HANDLE_GRAY: u8 = 128;

/// Edits `levels` with the handles and number fields. `counts` are the histogram buckets over
/// 0..1 (none while it can't be computed). Returns `Some(merge)` on every frame that changes
/// `levels`; `merge` is true for the later changes of one drag, so the edit continues the drag's
/// first undo step
pub fn levels_editor(ui: &mut Ui, counts: Option<&[u32]>, levels: &mut Levels) -> Option<bool> {
    let height = VALUE_ROW_HEIGHT * 2.0 + HANDLE_HEIGHT * 2.0 + HISTOGRAM_HEIGHT + GAP_BELOW_HISTOGRAM + GRADIENT_HEIGHT;
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    let input_handles_top = rect.top() + VALUE_ROW_HEIGHT;
    let output_handles_top = rect.bottom() - VALUE_ROW_HEIGHT - HANDLE_HEIGHT;
    let histogram_top = input_handles_top + HANDLE_HEIGHT;
    let gradient_top = output_handles_top - GRADIENT_HEIGHT;
    let track = track(rect);
    let id = ui.make_persistent_id("levels_editor");

    // Gamma goes last so it stays grabbable between input handles that close in on it
    let handles = [
        (LevelsHandle::InputLow, input_handles_top),
        (LevelsHandle::InputHigh, input_handles_top),
        (LevelsHandle::Gamma, input_handles_top),
        (LevelsHandle::OutputLow, output_handles_top),
        (LevelsHandle::OutputHigh, output_handles_top),
    ];
    let mut edit = None;
    let mut responses = Vec::with_capacity(handles.len());
    for (handle, top) in handles {
        let x = track_x(&track, handle.position(levels));
        let grab_half_width = HANDLE_HALF_WIDTH + HANDLE_GRAB_MARGIN;
        let hit = Rect::from_x_y_ranges(x - grab_half_width..=x + grab_half_width, top..=top + HANDLE_HEIGHT);
        let response = ui.interact(hit, id.with(handle), Sense::drag()).on_hover_cursor(CursorIcon::PointingHand);
        if response.dragged()
            && let Some(pointer) = response.interact_pointer_pos()
        {
            let mut next = *levels;
            handle.set_position(&mut next, track_position(&track, pointer.x));
            if next != *levels {
                *levels = next;
                edit = Some(continues_drag(ui, response.id));
            }
        }
        if response.drag_stopped() {
            ui.data_mut(|data| data.remove_temp::<bool>(response.id));
        }

        let value = handle.value(levels);
        response.widget_info(|| WidgetInfo::slider(true, f64::from(value), handle.label()));
        responses.push((handle, response));
    }

    let well = Rect::from_x_y_ranges(rect.x_range().shrink(HANDLE_HALF_WIDTH), histogram_top..=histogram_top + HISTOGRAM_HEIGHT);
    paint_histogram(ui, well, counts.unwrap_or_default(), levels);
    paint_output_gradient(ui, Rect::from_x_y_ranges(track, gradient_top..=gradient_top + GRADIENT_HEIGHT));
    paint_handles_and_values(ui, rect, &responses, levels);
    edit.or(number_fields(ui, levels))
}

/// The x range the handles move along in the editor's `rect`, lined up with the histogram bars
fn track(rect: Rect) -> egui::Rangef {
    rect.x_range().shrink(HANDLE_HALF_WIDTH + WELL_PADDING)
}

fn track_x(track: &egui::Rangef, position: f32) -> f32 {
    track.min + track.span() * position.clamp(0.0, 1.0)
}

fn track_position(track: &egui::Rangef, x: f32) -> f32 {
    ((x - track.min) / track.span().max(1.0)).clamp(0.0, 1.0)
}

/// True once the drag of `id` has already changed a value: its later changes share the first
/// change's undo step
fn continues_drag(ui: &Ui, id: Id) -> bool {
    ui.data_mut(|data| std::mem::replace(data.get_temp_mut_or_default::<bool>(id), true))
}

/// The histogram bars scaled to the fullest bucket in a field-colored well, the part outside the
/// input range dimmed, accent lines at the input ends and a dim line at gamma
fn paint_histogram(ui: &Ui, well: Rect, counts: &[u32], levels: &Levels) {
    let tokens = Tokens::get(ui.ctx());
    let painter = ui.painter_at(well);
    painter.rect(well, CornerRadius::same(tokens.radius), tokens.field, Stroke::new(1.0, tokens.field_border), egui::StrokeKind::Inside);

    let inner = well.shrink(WELL_PADDING);
    let fullest = counts.iter().copied().max().unwrap_or(0).max(1) as f32;
    let bar_width = inner.width() / counts.len().max(1) as f32;
    for (index, count) in counts.iter().enumerate() {
        let bar_height = inner.height() * *count as f32 / fullest;
        let x = inner.left() + index as f32 * bar_width;
        painter.rect_filled(Rect::from_min_max(pos2(x, inner.bottom() - bar_height), pos2(x + bar_width, inner.bottom())), 0.0, tokens.text_dim);
    }

    let x_at = |handle: LevelsHandle| track_x(&inner.x_range(), handle.position(levels));
    let (low, high) = (x_at(LevelsHandle::InputLow), x_at(LevelsHandle::InputHigh));
    let shade = tokens.field.gamma_multiply(0.7);
    painter.rect_filled(Rect::from_x_y_ranges(inner.left()..=low, inner.y_range()), 0.0, shade);
    painter.rect_filled(Rect::from_x_y_ranges(high..=inner.right(), inner.y_range()), 0.0, shade);
    for x in [low, high] {
        painter.line_segment([pos2(x, inner.top()), pos2(x, inner.bottom())], Stroke::new(1.0, tokens.accent));
    }

    let gamma = x_at(LevelsHandle::Gamma);
    painter.line_segment([pos2(gamma, inner.top()), pos2(gamma, inner.bottom())], Stroke::new(1.0, tokens.text_dim.gamma_multiply(0.6)));
}

/// A black-to-white strip: the output values the output handles pick from
fn paint_output_gradient(ui: &Ui, strip: Rect) {
    let mut mesh = egui::Mesh::default();
    for (corner, color) in
        [(strip.left_top(), Color32::BLACK), (strip.right_top(), Color32::WHITE), (strip.right_bottom(), Color32::WHITE), (strip.left_bottom(), Color32::BLACK)]
    {
        mesh.colored_vertex(corner, color);
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);

    let painter = ui.painter();
    painter.add(mesh);
    painter.rect_stroke(strip, 0.0, Stroke::new(1.0, Tokens::get(ui.ctx()).field_border), egui::StrokeKind::Outside);
}

/// Each handle as a pointer filled with the gray it stands for, its value printed beside it (the
/// handle being dragged or hovered first, a value that would overlap one already printed left out)
fn paint_handles_and_values(ui: &Ui, rect: Rect, responses: &[(LevelsHandle, Response)], levels: &Levels) {
    let track = track(rect);
    let input_top = rect.top() + VALUE_ROW_HEIGHT;
    let output_top = rect.bottom() - VALUE_ROW_HEIGHT - HANDLE_HEIGHT;
    let tokens = Tokens::get(ui.ctx());
    let painter = ui.painter();
    let font = egui::FontId::proportional(VALUE_FONT_SIZE);
    let is_active = |response: &Response| response.dragged() || response.hovered();
    let mut printed: Vec<Rect> = Vec::new();
    let mut by_priority: Vec<&(LevelsHandle, Response)> = responses.iter().collect();
    by_priority.sort_by_key(|(_, response)| !is_active(response));

    for (handle, response) in by_priority {
        let x = track_x(&track, handle.position(levels));
        let points_down = LevelsHandle::INPUT.contains(handle);
        let (top, value_anchor, align) = if points_down {
            (input_top, pos2(x, input_top), egui::Align2::CENTER_BOTTOM)
        } else {
            (output_top, pos2(x, output_top + HANDLE_HEIGHT), egui::Align2::CENTER_TOP)
        };
        let fill = match handle {
            LevelsHandle::InputLow => Color32::BLACK,
            LevelsHandle::Gamma => Color32::from_gray(GAMMA_HANDLE_GRAY),
            LevelsHandle::InputHigh => Color32::WHITE,
            LevelsHandle::OutputLow | LevelsHandle::OutputHigh => Color32::from_gray((handle.value(levels).clamp(0.0, 1.0) * 255.0).round() as u8),
        };
        let outline = if response.dragged() {
            tokens.accent
        } else if response.hovered() {
            tokens.text_strong
        } else {
            tokens.text
        };
        painter.add(egui::Shape::convex_polygon(handle_outline(x, top, points_down), fill, Stroke::new(1.0, outline)));

        let color = if is_active(response) { tokens.text_strong } else { tokens.text_dim };
        let galley = painter.layout_no_wrap(format!("{:.2}", handle.value(levels)), font.clone(), color);
        let text_rect = align.anchor_size(value_anchor, galley.size());
        // Values at the ends stay inside the editor
        let shift = (rect.left() - text_rect.left()).max(0.0) + (rect.right() - text_rect.right()).min(0.0);
        let text_rect = text_rect.translate(vec2(shift, 0.0));
        if printed.iter().any(|other| other.expand(2.0).intersects(text_rect)) {
            continue;
        }
        painter.galley(text_rect.min, galley, color);
        printed.push(text_rect);
    }
}

/// A pentagon pointer centred on `x`: flat side away from the track, tip touching it
fn handle_outline(x: f32, top: f32, points_down: bool) -> Vec<Pos2> {
    let bottom = top + HANDLE_HEIGHT;
    let (flat, side_end, tip) = if points_down { (top, top + HANDLE_SIDE_HEIGHT, bottom) } else { (bottom, bottom - HANDLE_SIDE_HEIGHT, top) };
    vec![
        pos2(x - HANDLE_HALF_WIDTH, flat),
        pos2(x + HANDLE_HALF_WIDTH, flat),
        pos2(x + HANDLE_HALF_WIDTH, side_end),
        pos2(x, tip),
        pos2(x - HANDLE_HALF_WIDTH, side_end),
    ]
}

/// The five values as number fields: the input ones on the left, the output ones on the right
fn number_fields(ui: &mut Ui, levels: &mut Levels) -> Option<bool> {
    ui.horizontal(|ui| {
        let mut edit = None;
        for handle in LevelsHandle::INPUT {
            edit = edit.or(number_field(ui, handle, levels));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Right to left: the last output field comes first
            for handle in LevelsHandle::OUTPUT.into_iter().rev() {
                edit = edit.or(number_field(ui, handle, levels));
            }
        });
        edit
    })
    .inner
}

fn number_field(ui: &mut Ui, handle: LevelsHandle, levels: &mut Levels) -> Option<bool> {
    let mut value = handle.value(levels);
    let (range, speed) = if handle == LevelsHandle::Gamma { (GAMMA_RANGE, 0.01) } else { (0.0..=1.0, 0.005) };
    let response = ui
        .add(egui::DragValue::new(&mut value).range(range).speed(speed).fixed_decimals(2).clamp_existing_to_range(false).update_while_editing(false))
        .on_hover_text(handle.label());
    response.widget_info(|| {
        let mut info = WidgetInfo::labeled(Role::SpinButton, true, handle.label());
        info.value = Some(f64::from(value));
        info
    });

    let mut edit = None;
    if response.changed() {
        let mut next = *levels;
        handle.set_value(&mut next, value);
        if next != *levels {
            *levels = next;
            edit = Some(response.dragged() && continues_drag(ui, response.id));
        }
    }
    if response.drag_stopped() {
        ui.data_mut(|data| data.remove_temp::<bool>(response.id));
    }
    edit
}
