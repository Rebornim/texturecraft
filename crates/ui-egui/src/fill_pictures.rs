//! Pictures of what a fill's settings do: one per projection, filtering and UV wrap choice, and the window preview

use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, pos2, vec2};
use texturecraft_engine::core::{Filtering, ShapeCrop, UvWrap};

use crate::fill_placement::Placement;
use crate::projection_kind::ProjectionKind;

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
/// The two shades of the preview's checker cells, as fractions of the way from `field` to `text`
const PREVIEW_DARK: f32 = 0.18;
const PREVIEW_LIGHT: f32 = 0.55;

/// A window on the surface: a UV checker (2 × 2 cells per tile, accent tile borders) tiled, turned and slid as the fill is
pub(crate) fn window_preview(painter: &Painter, rect: Rect, placement: Placement, tokens: &Tokens) {
    let painter = painter.with_clip_rect(rect);
    painter.rect_filled(rect, 0, tokens.field);
    let tiling = [placement.tiling[0].clamp(1e-3, PREVIEW_MAX_TILES), placement.tiling[1].clamp(1e-3, PREVIEW_MAX_TILES)];
    let (sin, cos) = placement.rotation.to_radians().sin_cos();
    // Texture coordinates → the window (0..1 across and down): undo the offset, the scale, the turn
    let to_window = |t: [f32; 2]| -> Pos2 {
        let (x, y) = ((t[0] - placement.offset[0]) / tiling[0], (t[1] - placement.offset[1]) / tiling[1]);
        let (u, v) = (x * cos + y * sin, -x * sin + y * cos);
        pos2(rect.left() + u * rect.width(), rect.top() + v * rect.height())
    };
    // The window's corners in texture coordinates (turn, scale, offset): the tiles to draw
    let to_texture = |u: f32, v: f32| -> [f32; 2] {
        let (x, y) = (u * cos - v * sin, u * sin + v * cos);
        [x * tiling[0] + placement.offset[0], y * tiling[1] + placement.offset[1]]
    };
    let corners = [to_texture(0.0, 0.0), to_texture(1.0, 0.0), to_texture(0.0, 1.0), to_texture(1.0, 1.0)];
    let low = |axis: usize| corners.iter().map(|c| c[axis]).fold(f32::INFINITY, f32::min).floor() as i32;
    let high = |axis: usize| corners.iter().map(|c| c[axis]).fold(f32::NEG_INFINITY, f32::max).ceil() as i32;
    let (dark, light) = (lerp_color(tokens.field, tokens.text, PREVIEW_DARK), lerp_color(tokens.field, tokens.text, PREVIEW_LIGHT));
    let (first_x, last_x, first_y, last_y) = (low(0), high(0), low(1), high(1));
    let border = Stroke::new(1.0, tokens.accent);
    for tile_x in first_x..last_x {
        for tile_y in first_y..last_y {
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

/// Checker cells across a warp preview's frame
const WARP_CELLS: usize = 4;

/// How much of the preview an unpulled frame spans
const WARP_FRAME_SPAN: f32 = 0.62;

/// Radius of a warp preview's corner handle
const WARP_HANDLE_RADIUS: f32 = 3.5;

/// A warp's frame with its corners where they are pulled, the checker bending with them
pub(crate) fn warp_preview(painter: &Painter, rect: Rect, corners: [[f32; 2]; 4], tokens: &Tokens) {
    let painter = painter.with_clip_rect(rect);
    painter.rect_filled(rect, 0, tokens.field);
    // Across and up in frame sizes from its centre -> the square
    let [top_left, top_right, bottom_right, bottom_left] =
        corners.map(|[x, y]| pos2(rect.center().x + x * WARP_FRAME_SPAN * rect.width(), rect.center().y - y * WARP_FRAME_SPAN * rect.height()));
    let at = |u: f32, v: f32| -> Pos2 {
        let top = top_left + (top_right - top_left) * u;
        let bottom = bottom_left + (bottom_right - bottom_left) * u;
        top + (bottom - top) * v
    };
    let (dark, light) = (lerp_color(tokens.field, tokens.text, PREVIEW_DARK), lerp_color(tokens.field, tokens.text, PREVIEW_LIGHT));
    let cells = WARP_CELLS as f32;
    for i in 0..WARP_CELLS {
        for j in 0..WARP_CELLS {
            let (u0, v0, u1, v1) = (i as f32 / cells, j as f32 / cells, (i + 1) as f32 / cells, (j + 1) as f32 / cells);
            let outline = vec![at(u0, v0), at(u1, v0), at(u1, v1), at(u0, v1)];
            let color = if (i + j) % 2 == 0 { light } else { dark };
            painter.add(Shape::convex_polygon(outline, color, Stroke::NONE));
        }
    }
    let edge = vec![top_left, top_right, bottom_right, bottom_left];
    painter.add(Shape::closed_line(edge.clone(), Stroke::new(1.5, tokens.accent)));
    for corner in edge {
        painter.circle(corner, WARP_HANDLE_RADIUS, tokens.text, Stroke::new(1.0, tokens.accent));
    }
    painter.rect_stroke(rect, 0, Stroke::new(1.0, tokens.field_border), egui::StrokeKind::Inside);
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let mix = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round().clamp(0.0, 255.0) as u8;
    Color32::from_rgb(mix(a.r(), b.r()), mix(a.g(), b.g()), mix(a.b(), b.b()))
}
