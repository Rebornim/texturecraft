//! An asset the user marked as a favorite in the Assets panel: it shows a star and comes first in its type

use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, epaint::Mesh, vec2};
use serde::{Deserialize, Serialize};

use crate::shelf_entry::ShelfEntry;

/// The star's outer and inner radius and its centre's inset from the thumbnail's top-right corner, in points
const STAR_OUTER_RADIUS: f32 = 5.5;
const STAR_INNER_RADIUS: f32 = 2.4;
const STAR_INSET: f32 = 4.0;
/// The offset and colour of the star's shadow, which keeps it readable on light thumbnails
const STAR_SHADOW_OFFSET: f32 = 1.0;
const STAR_SHADOW: Color32 = Color32::from_black_alpha(170);
const STAR_POINTS: usize = 5;
/// The width of the outline that smooths the star's edges (a mesh has no anti-aliasing), in points
const STAR_EDGE_WIDTH: f32 = 0.6;

/// A favorite asset by its [`ShelfEntry::identity`]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShelfFavorite {
    pub kind: String,
    pub id: String,
}

impl ShelfFavorite {
    fn is(&self, entry: &ShelfEntry) -> bool {
        let (kind, id) = entry.identity();
        self.kind == kind && self.id == id
    }
}

/// Where `entry` is in `favorites` (most recently added first), or None if it isn't one
pub(crate) fn rank(favorites: &[ShelfFavorite], entry: &ShelfEntry) -> Option<usize> {
    favorites.iter().position(|favorite| favorite.is(entry))
}

/// Adds `entry` in front of `favorites`, or removes it if it is one
pub(crate) fn toggle(favorites: &mut Vec<ShelfFavorite>, entry: &ShelfEntry) {
    if let Some(index) = rank(favorites, entry) {
        favorites.remove(index);
        return;
    }

    let (kind, id) = entry.identity();
    favorites.insert(0, ShelfFavorite { kind: kind.to_owned(), id: id.to_owned() });
}

/// A filled star in `color` at the thumbnail's top-right corner, over a dark shadow
pub(crate) fn star(painter: &Painter, thumbnail: Rect, color: Color32) {
    let center = thumbnail.right_top() + vec2(-STAR_INSET, STAR_INSET);
    star_shape(painter, center + vec2(STAR_SHADOW_OFFSET, STAR_SHADOW_OFFSET), STAR_SHADOW);
    star_shape(painter, center, color);
}

/// A five-pointed star around `center`, point up: a triangle fan from the centre, outlined in its colour
fn star_shape(painter: &Painter, center: Pos2, color: Color32) {
    let corner_count = STAR_POINTS * 2;
    let corners: Vec<Pos2> = (0..corner_count)
        .map(|corner| {
            let radius = if corner % 2 == 0 { STAR_OUTER_RADIUS } else { STAR_INNER_RADIUS };
            let angle = corner as f32 * std::f32::consts::TAU / corner_count as f32 - std::f32::consts::FRAC_PI_2;
            center + radius * vec2(angle.cos(), angle.sin())
        })
        .collect();

    let mut mesh = Mesh::default();
    mesh.colored_vertex(center, color);
    for corner in &corners {
        mesh.colored_vertex(*corner, color);
    }
    let last = corner_count as u32;
    for corner in 1..=last {
        mesh.add_triangle(0, corner, corner % last + 1);
    }
    painter.add(Shape::mesh(mesh));
    painter.add(Shape::closed_line(corners, Stroke::new(STAR_EDGE_WIDTH, color)));
}
