//! Pictures of what a fill's settings do: one per projection, filtering and UV wrap choice, the window preview and the ring of radial copies

use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, pos2, vec2};
use texturecraft_engine::core::{Filtering, ShapeCrop, UvWrap};

use crate::fill_placement::Placement;
use crate::projection_kind::ProjectionKind;
use crate::projection_preview::{edge_markers, passed_sides};
use crate::warp_checker::warp_checker;

use crate::theme::Tokens;

/// The shape a projection wraps around, as an icon in the accent colour
pub(crate) fn projection_picture(painter: &Painter, rect: Rect, kind: ProjectionKind, tokens: &Tokens) {
    use egui_phosphor::regular as icon;
    let glyph = match kind {
        ProjectionKind::UvTile | ProjectionKind::Uv => icon::SQUARE,
        ProjectionKind::Triplanar => icon::CUBE,
        ProjectionKind::Planar | ProjectionKind::Warp => icon::SELECTION,
        ProjectionKind::Spherical => icon::GLOBE,
        ProjectionKind::Cylindrical => icon::CYLINDER,
    };
    painter.text(rect.center(), egui::Align2::CENTER_CENTER, glyph, egui::FontId::proportional(rect.height() * 0.85), tokens.accent);
}

/// Cells across a magnified edge in the Filtering picture
const EDGE_CELLS: usize = 6;

/// A magnified diagonal edge: smooth (HQ), crisp (Sharp) or stair-stepped (Nearest)
pub(crate) fn filtering_picture(painter: &Painter, rect: Rect, filtering: Filtering, tokens: &Tokens) {
    let cell = rect.width() / EDGE_CELLS as f32;
    for i in 0..EDGE_CELLS {
        for j in 0..EDGE_CELLS {
            let across = (i as f32 - j as f32) / EDGE_CELLS as f32;
            let amount = match filtering {
                Filtering::Nearest => f32::from(u8::from(i >= j)),
                Filtering::BilinearSharp => (0.5 + across * 3.0).clamp(0.0, 1.0),
                Filtering::BilinearHq => (0.5 + across * 1.2).clamp(0.0, 1.0),
            };
            let color = lerp_color(tokens.field, tokens.accent, amount);
            let min = pos2(rect.left() + i as f32 * cell, rect.top() + j as f32 * cell);
            painter.rect_filled(Rect::from_min_size(min, vec2(cell, cell)), 0, color);
        }
    }
}

/// Cells of the UV Wrap picture's grid
const WRAP_CELLS: usize = 3;

/// A 3 × 3 grid: the middle tile is the image's own; filled tiles are where it repeats
pub(crate) fn wrap_picture(painter: &Painter, rect: Rect, wrap: UvWrap, tokens: &Tokens) {
    let (across, down) = wrap.repeats();
    let cell = rect.width() / WRAP_CELLS as f32;
    let middle = WRAP_CELLS / 2;
    for i in 0..WRAP_CELLS {
        for j in 0..WRAP_CELLS {
            let own = i == middle && j == middle;
            let repeated = own || (across && j == middle) || (down && i == middle) || (across && down);
            let tile = Rect::from_min_size(pos2(rect.left() + i as f32 * cell, rect.top() + j as f32 * cell), vec2(cell, cell)).shrink(0.75);
            if own {
                painter.rect_filled(tile, 1, tokens.accent);
            } else if repeated {
                painter.rect_filled(tile, 1, tokens.accent.gamma_multiply(0.55));
            } else {
                painter.rect_stroke(tile, 1, Stroke::new(1.0, tokens.separator), egui::StrokeKind::Inside);
            }
        }
    }
}

/// Most tiles drawn across the window preview: beyond this the pattern is too fine to read
const PREVIEW_MAX_TILES: f32 = 24.0;
/// Most tiles drawn along one direction of a lattice preview, whatever its map
const PREVIEW_MAX_SPAN: i32 = 64;
/// The two shades of the preview's checker cells, as fractions of the way from `field` to `text`
const PREVIEW_DARK: f32 = 0.18;
const PREVIEW_LIGHT: f32 = 0.55;

/// A window on the surface that never moves: a UV checker (2 × 2 cells per tile, accent tile borders) tiled, turned and slid as the fill is, the ring in the middle where Offset 0 puts the projection, the filled dot where its Offset carried that point
pub(crate) fn window_preview(painter: &Painter, rect: Rect, placement: Placement, tokens: &Tokens) {
    let (columns, carried) = slid_window(placement);
    lattice_preview(painter, rect, (columns, placement.offset), true, tokens);
    let position = pos2(rect.left() + carried[0] * rect.width(), rect.top() + carried[1] * rect.height());
    let painter = painter.with_clip_rect(rect.intersect(painter.clip_rect()));
    crate::preview_dots::preview_dots(&painter, rect.center(), position, tokens);
    edge_markers(&painter, rect, passed_sides(std::iter::once(position), rect), tokens);
}

/// The window's centre (0..1) before any slide
const WINDOW_MIDDLE: f32 = 0.5;

/// The window preview's map, and where the Offset carried the window's middle (0..1): the checker slides with it, so a drag right moves both right
fn slid_window(placement: Placement) -> ([[f32; 2]; 2], [f32; 2]) {
    let tiling = [placement.tiling[0].clamp(1e-3, PREVIEW_MAX_TILES), placement.tiling[1].clamp(1e-3, PREVIEW_MAX_TILES)];
    let (sin, cos) = placement.rotation.to_radians().sin_cos();
    // Turn, scale, then offset: one step across the window moves (cos, sin) scaled in the texture
    let columns = [[cos * tiling[0], sin * tiling[1]], [-sin * tiling[0], cos * tiling[1]]];
    let [[a, b], [c, d]] = columns;
    let det = a * d - c * b;
    let [x, y] = placement.offset;
    // The window step the Offset is worth: the map undone; the image moves against the Offset it samples at
    let moved = [(d * x - c * y) / det, (a * y - b * x) / det];
    let carried = if moved.iter().all(|value| value.is_finite()) { [WINDOW_MIDDLE - moved[0], WINDOW_MIDDLE - moved[1]] } else { [WINDOW_MIDDLE; 2] };
    (columns, carried)
}

/// The window preview for any linear map: window point (u, v) shows the texture at `columns[0] · u + columns[1] · v + shift`; without `repeat` only tile 0..1
pub(crate) fn lattice_preview(painter: &Painter, rect: Rect, (columns, shift): ([[f32; 2]; 2], [f32; 2]), repeat: bool, tokens: &Tokens) {
    let painter = painter.with_clip_rect(rect);
    painter.rect_filled(rect, 0, tokens.field);
    let [[a, b], [c, d]] = columns;
    let det = a * d - c * b;
    if det.abs() > 1e-9 && det.is_finite() && shift.iter().all(|s| s.is_finite()) {
        // Texture coordinates → the window: undo the shift, then the map
        let to_window = |t: [f32; 2]| -> Pos2 {
            let (x, y) = (t[0] - shift[0], t[1] - shift[1]);
            let (u, v) = ((d * x - c * y) / det, (a * y - b * x) / det);
            pos2(rect.left() + u * rect.width(), rect.top() + v * rect.height())
        };
        let to_texture = |u: f32, v: f32| -> [f32; 2] { [a * u + c * v + shift[0], b * u + d * v + shift[1]] };
        let corners = [to_texture(0.0, 0.0), to_texture(1.0, 0.0), to_texture(0.0, 1.0), to_texture(1.0, 1.0)];
        let low = |axis: usize| corners.iter().map(|c| c[axis]).fold(f32::INFINITY, f32::min).floor() as i32;
        let high = |axis: usize| corners.iter().map(|c| c[axis]).fold(f32::NEG_INFINITY, f32::max).ceil() as i32;
        let span = |first: i32, last: i32| if repeat { first..last.min(first.saturating_add(PREVIEW_MAX_SPAN)) } else { first.max(0)..last.min(1) };
        let (dark, light) = (lerp_color(tokens.field, tokens.text, PREVIEW_DARK), lerp_color(tokens.field, tokens.text, PREVIEW_LIGHT));
        let border = Stroke::new(1.0, tokens.accent);
        for tile_x in span(low(0), high(0)) {
            for tile_y in span(low(1), high(1)) {
                for (cell_x, cell_y) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let (x0, y0) = (tile_x as f32 + cell_x as f32 * 0.5, tile_y as f32 + cell_y as f32 * 0.5);
                    let corners = [[x0, y0], [x0 + 0.5, y0], [x0 + 0.5, y0 + 0.5], [x0, y0 + 0.5]].map(to_window);
                    let color = if (cell_x + cell_y) % 2 == 0 { light } else { dark };
                    painter.add(Shape::convex_polygon(corners.to_vec(), color, Stroke::NONE));
                }
                let (x, y) = (tile_x as f32, tile_y as f32);
                painter.add(Shape::closed_line([[x, y], [x + 1.0, y], [x + 1.0, y + 1.0], [x, y + 1.0]].map(to_window).to_vec(), border));
            }
        }
    }
    painter.rect_stroke(rect, 0, Stroke::new(1.0, tokens.field_border), egui::StrokeKind::Inside);
}

/// The darker accent pictures fill with, beside the accent itself
fn accent_shade(tokens: &Tokens) -> Color32 {
    lerp_color(tokens.accent, Color32::BLACK, ACCENT_SHADE)
}

/// How far the darker accent sits from the accent towards black
const ACCENT_SHADE: f32 = 0.36;

/// Shape crop: the projection inside its shape only, or past it too
pub(crate) fn crop_picture(painter: &Painter, rect: Rect, crop: ShapeCrop, tokens: &Tokens) {
    let inner = rect.shrink(3.0);
    match crop {
        ShapeCrop::Cropped => {
            painter.rect_filled(inner, 3, accent_shade(tokens));
        }
        ShapeCrop::ExtendsOutside => {
            painter.rect_filled(rect, 2, accent_shade(tokens));
        }
    }
    painter.rect_stroke(inner, 3, Stroke::new(1.0, tokens.text), egui::StrokeKind::Inside);
}

/// Depth culling, seen from the side: the surface the projection lands on and what lies under it
pub(crate) fn depth_picture(painter: &Painter, rect: Rect, tokens: &Tokens) {
    let top = rect.top() + rect.height() * 0.25;
    let under = Rect::from_min_max(pos2(rect.left(), top), pos2(rect.right(), top + rect.height() * 0.45 + 2.0));
    painter.rect_filled(under, 2, accent_shade(tokens));
    painter.line_segment([pos2(rect.left(), top), pos2(rect.right(), top)], Stroke::new(1.0, tokens.text));
    painter.text(
        pos2(rect.center().x, rect.top() + 3.0),
        egui::Align2::CENTER_CENTER,
        egui_phosphor::regular::ARROW_DOWN,
        egui::FontId::proportional(9.0),
        tokens.accent,
    );
}

/// Backface culling: a face turned away, darker while culled
pub(crate) fn backface_picture(painter: &Painter, rect: Rect, on: bool, tokens: &Tokens) {
    let (left, apex, right) =
        (pos2(rect.left() + 2.0, rect.bottom() - 3.0), pos2(rect.center().x, rect.top() + 3.0), pos2(rect.right() - 2.0, rect.bottom() - 3.0));
    painter.add(Shape::convex_polygon(vec![left, apex, right], if on { accent_shade(tokens) } else { tokens.accent }, Stroke::NONE));
    painter.line_segment([apex, right], Stroke::new(1.0, tokens.text));
}

/// Points around the Angle picture's arc per full turn
const ARC_POINTS: usize = 48;

/// A cylindrical projection's Angle seen from above: the slice of a turn one repeat spans
pub(crate) fn angle_picture(painter: &Painter, rect: Rect, degrees: f32, tokens: &Tokens) {
    let center = rect.center();
    let radius = rect.width() * 0.45;
    let sweep = degrees.clamp(0.0, 360.0) / 360.0;
    if sweep >= 1.0 {
        painter.circle_filled(center, radius, tokens.accent);
    } else {
        let steps = ((ARC_POINTS as f32 * sweep).ceil() as usize).max(1);
        let mut points = vec![center];
        for step in 0..=steps {
            let turn = (step as f32 / steps as f32 * sweep - 0.25) * std::f32::consts::TAU;
            points.push(pos2(center.x + radius * turn.cos(), center.y + radius * turn.sin()));
        }
        // A slice wider than half a turn is not convex: fill it as a fan from the centre
        for pair in points[1..].windows(2) {
            painter.add(Shape::convex_polygon(vec![center, pair[0], pair[1]], tokens.accent, Stroke::NONE));
        }
    }
    painter.circle_stroke(center, radius, Stroke::new(1.0, tokens.button_border));
}

/// How much of the preview an unpulled frame spans per mesh diagonal of its side (a new warp's frame round a ball spans about 0.62)
const WARP_SPAN_PER_DIAGONAL: f32 = 1.07;
/// The least and most of the preview a frame's side spans
const WARP_SPAN_RANGE: std::ops::RangeInclusive<f32> = 0.05..=0.95;

/// Radius of a warp preview's corner handle
const WARP_HANDLE_RADIUS: f32 = 3.5;

/// A warp's frame with its corners where they are pulled, the checker bending with them and laid as `placement` lays the image (tiling, rotation, offset); `size` is the frame's width and height in mesh diagonals, `turn` its turn round its normal (degrees), so resizing or turning the frame shows
pub(crate) fn warp_preview(painter: &Painter, rect: Rect, corners: [[f32; 2]; 4], size: [f32; 2], turn: f32, placement: Placement, tokens: &Tokens) {
    let painter = painter.with_clip_rect(rect);
    painter.rect_filled(rect, 0, tokens.field);
    let span = size.map(|side| (side * WARP_SPAN_PER_DIAGONAL).clamp(*WARP_SPAN_RANGE.start(), *WARP_SPAN_RANGE.end()));
    let (sin, cos) = turn.to_radians().sin_cos();
    // Across and up in frame sizes from its centre -> turned -> the square
    let [top_left, top_right, bottom_right, bottom_left] = corners.map(|[x, y]| {
        let (x, y) = (x * span[0], y * span[1]);
        let (x, y) = (x * cos - y * sin, x * sin + y * cos);
        pos2(rect.center().x + x * rect.width(), rect.center().y - y * rect.height())
    });
    let (dark, light) = (lerp_color(tokens.field, tokens.text, PREVIEW_DARK), lerp_color(tokens.field, tokens.text, PREVIEW_LIGHT));
    for (outline, lit) in warp_checker([top_left, top_right, bottom_right, bottom_left], placement) {
        painter.add(Shape::convex_polygon(outline, if lit { light } else { dark }, Stroke::NONE));
    }
    let edge = vec![top_left, top_right, bottom_right, bottom_left];
    painter.add(Shape::closed_line(edge.clone(), Stroke::new(1.5, tokens.accent)));
    for corner in edge {
        painter.circle(corner, WARP_HANDLE_RADIUS, tokens.text, Stroke::new(1.0, tokens.accent));
    }
    painter.rect_stroke(rect, 0, Stroke::new(1.0, tokens.field_border), egui::StrokeKind::Inside);
}

/// Radial copies seen along their axis: the axis at the centre (its colour), the original (white) at the top and `count - 1` copies (accent) round the ring over `span` degrees
pub(crate) fn ring_picture(painter: &Painter, rect: Rect, count: u32, span: f32, axis_color: Color32, tokens: &Tokens) {
    painter.rect_filled(rect, 0, tokens.field);
    let center = rect.center();
    let radius = rect.width() * RING_RADIUS;
    let full = span >= 360.0;
    let step = if full { span / count.max(1) as f32 } else { span / count.saturating_sub(1).max(1) as f32 };
    let at = |degrees: f32| {
        let turn = (degrees - 90.0).to_radians();
        center + vec2(turn.cos(), turn.sin()) * radius
    };
    let ring_end = if full { 360.0 } else { span };
    let ring: Vec<Pos2> = (0..=RING_SEGMENTS).map(|k| at(ring_end * k as f32 / RING_SEGMENTS as f32)).collect();
    painter.add(Shape::line(ring, Stroke::new(1.0, tokens.separator)));
    painter.circle_filled(center, RING_AXIS_DOT, axis_color);
    for k in 0..count {
        let color = if k == 0 { tokens.text } else { tokens.accent };
        painter.circle_filled(at(k as f32 * step), RING_COPY_DOT, color);
    }
}

/// The ring's radius in the picture's widths, its dots' radii, and the segments it is drawn with
const RING_RADIUS: f32 = 0.34;
const RING_AXIS_DOT: f32 = 2.0;
const RING_COPY_DOT: f32 = 3.5;
const RING_SEGMENTS: usize = 64;

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let mix = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round().clamp(0.0, 255.0) as u8;
    Color32::from_rgb(mix(a.r(), b.r()), mix(a.g(), b.g()), mix(a.b(), b.b()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The UV view stands still: the checker slides with the Offset as on the model, and the filled dot (the window's middle, carried) slides with it, the way a drag moves them
    #[test]
    fn the_uv_view_stands_still_and_the_dot_slides_with_the_checker() {
        let placement = |offset: [f32; 2]| Placement { tiling: [2.0, 2.0], rotation: 30.0, offset };
        let (still, rest) = slid_window(placement([0.0, 0.0]));
        assert_eq!(rest, [0.5, 0.5], "at Offset 0 the dot sits in the ring");
        let slid = placement([-0.25, 0.0]);
        let (columns, dot) = slid_window(slid);
        assert_eq!(columns, still, "the view never zooms or turns for the Offset");
        // A pad drag right lowers the Offset: the dot goes right, an eighth of the window along the turned U
        let expected = [0.5 + 0.125 * 30f32.to_radians().cos(), 0.5 - 0.125 * 30f32.to_radians().sin()];
        assert!((dot[0] - expected[0]).abs() < 1e-5 && (dot[1] - expected[1]).abs() < 1e-5, "{dot:?} vs {expected:?}");
        // The checker point under the middle at Offset 0 shows under the dot now
        let [[a, b], [c, d]] = columns;
        let texture_at = |[u, v]: [f32; 2], offset: [f32; 2]| [a * u + c * v + offset[0], b * u + d * v + offset[1]];
        let (before, after) = (texture_at([0.5, 0.5], [0.0, 0.0]), texture_at(dot, slid.offset));
        assert!((before[0] - after[0]).abs() < 1e-5 && (before[1] - after[1]).abs() < 1e-5, "{before:?} vs {after:?}");
    }
}
