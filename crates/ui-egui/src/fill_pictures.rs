//! Pictures of what a fill's settings do: one per projection, filtering and UV wrap choice,
//! and the window preview of where the fill's tiles land.

use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, pos2, vec2};
use texturecraft_engine::core::{Filtering, UvWrap};

use crate::theme::Tokens;

/// The projections a fill's Projection dropdown offers, with their command ids
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProjectionKind {
    Uv,
    Triplanar,
    Planar,
    Spherical,
    Cylindrical,
}

impl ProjectionKind {
    pub(crate) const ALL: [ProjectionKind; 5] =
        [ProjectionKind::Uv, ProjectionKind::Triplanar, ProjectionKind::Planar, ProjectionKind::Spherical, ProjectionKind::Cylindrical];

    /// The `projection` param of `layer.fill.projection`
    pub(crate) fn id(self) -> &'static str {
        match self {
            ProjectionKind::Uv => "uv",
            ProjectionKind::Triplanar => "triplanar",
            ProjectionKind::Planar => "planar",
            ProjectionKind::Spherical => "spherical",
            ProjectionKind::Cylindrical => "cylindrical",
        }
    }

    pub(crate) fn of(projection: &texturecraft_engine::core::Projection) -> Option<ProjectionKind> {
        use texturecraft_engine::core::Projection;
        match projection {
            Projection::Uv { .. } => Some(ProjectionKind::Uv),
            Projection::Triplanar { .. } => Some(ProjectionKind::Triplanar),
            Projection::Planar { .. } => Some(ProjectionKind::Planar),
            Projection::Spherical { .. } => Some(ProjectionKind::Spherical),
            Projection::Cylindrical { .. } => Some(ProjectionKind::Cylindrical),
            Projection::Decal(_) => None,
        }
    }

    /// The dropdown's name for it
    pub(crate) fn label(self) -> &'static str {
        use crate::i18n::n_;
        match self {
            ProjectionKind::Uv => n_("UV projection"),
            ProjectionKind::Triplanar => n_("Tri-planar projection"),
            ProjectionKind::Planar => n_("Planar projection"),
            ProjectionKind::Spherical => n_("Spherical projection"),
            ProjectionKind::Cylindrical => n_("Cylindrical projection"),
        }
    }
}

/// The shape a projection wraps around, as an icon in the accent colour
pub(crate) fn projection_picture(painter: &Painter, rect: Rect, kind: ProjectionKind, tokens: &Tokens) {
    use egui_phosphor::regular as icon;
    let glyph = match kind {
        ProjectionKind::Uv => icon::SQUARE,
        ProjectionKind::Triplanar => icon::CUBE,
        ProjectionKind::Planar => icon::SELECTION,
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

/// How the fill's image is laid on its tile: tiles across and down, rotation in degrees,
/// offset in tiles (the engine's UV placement: rotate, scale, then offset).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Placement {
    pub tiling: [f32; 2],
    pub rotation: f32,
    pub offset: [f32; 2],
}

/// Most tiles drawn across the window preview: beyond this the pattern is too fine to read
const PREVIEW_MAX_TILES: f32 = 24.0;
/// The two shades of the preview's checker cells, as fractions of the way from `field` to `text`
const PREVIEW_DARK: f32 = 0.18;
const PREVIEW_LIGHT: f32 = 0.55;

/// A window on the surface showing the fill as it lands: a UV checker (2 × 2 light and dark
/// cells per tile, thin accent tile borders), tiled, turned and slid exactly as the fill is.
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

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let mix = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round().clamp(0.0, 255.0) as u8;
    Color32::from_rgb(mix(a.r(), b.r()), mix(a.g(), b.g()), mix(a.b(), b.b()))
}
