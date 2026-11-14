//! The preview square's two dots: a ring where the projection sits with Offset 0, a filled dot where its Offset puts it, a dotted line between

use egui::{Painter, Pos2, Stroke};

use crate::theme::Tokens;

/// The ring's radius, its line and the line under it that keeps it readable on the checker (points)
const RING_RADIUS: f32 = 5.5;
const RING_LINE: f32 = 1.6;
const RING_UNDER: f32 = 3.6;
/// The filled dot's radius and the rim under it (points)
const DOT_RADIUS: f32 = 3.3;
const DOT_RIM: f32 = 1.0;
/// The dotted line: air between its dots, their radius and the rim under them (points)
const LINE_SPACING: f32 = 3.5;
const LINE_DOT: f32 = 0.9;
const LINE_RIM: f32 = 0.6;
/// Most dots the line draws: more than any preview square holds, the rest would be cut off anyway
const MAX_LINE_DOTS: usize = 256;

/// Paints the two dots and the dotted line between them, from the filled dot out so a ring far past the square still gets its line
pub(crate) fn preview_dots(painter: &Painter, center: Pos2, position: Pos2, tokens: &Tokens) {
    let span = center - position;
    let length = span.length();
    let start = DOT_RADIUS + LINE_SPACING;
    let end = length - RING_RADIUS - LINE_SPACING / 2.0;
    if length.is_finite() && end > start {
        let toward = span / length;
        let count = (((end - start) / LINE_SPACING).floor() as usize).min(MAX_LINE_DOTS);
        for step in 0..=count {
            let at = position + toward * (start + step as f32 * LINE_SPACING);
            painter.circle_filled(at, LINE_DOT + LINE_RIM, tokens.field);
            painter.circle_filled(at, LINE_DOT, tokens.text);
        }
    }
    painter.circle_stroke(center, RING_RADIUS, Stroke::new(RING_UNDER, tokens.field));
    painter.circle_stroke(center, RING_RADIUS, Stroke::new(RING_LINE, tokens.text));
    painter.circle_filled(position, DOT_RADIUS + DOT_RIM, tokens.field);
    painter.circle_filled(position, DOT_RADIUS, tokens.text);
}
