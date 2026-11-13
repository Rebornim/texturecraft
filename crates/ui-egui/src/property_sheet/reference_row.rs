//! One row of a list of things to go to: a glyph, a name and its kind (dim); hovered, a face and an arrow show the click selects it

use egui::{Align2, CornerRadius, FontId, Rect, Sense, pos2};
use egui_phosphor::regular as ic;

use super::group::CARD_BLEED;
use super::{PropertySheet, ROW_HEIGHT, TEXT_SIZE};

/// From the left edge to the glyph's centre, and from there to the name
const GLYPH_CENTER: f32 = 6.0;
const GLYPH_TO_NAME: f32 = 14.0;

/// Size of the glyph and of the arrow
const GLYPH_SIZE: f32 = 13.0;
const ARROW_SIZE: f32 = 12.0;

/// Air between the name and its kind
const NAME_TO_KIND: f32 = 6.0;

/// From the right edge to the arrow's centre
const ARROW_INSET: f32 = 6.0;

/// Corner radius of the hovered row's face
const ROW_RADIUS: u8 = 6;

impl PropertySheet<'_> {
    /// The row of `name` (a `kind`, drawn with `glyph`); returns true when it was clicked
    pub(crate) fn reference_row(&mut self, glyph: &str, name: &str, kind: &str) -> bool {
        let rect = self.take(ROW_HEIGHT);
        let center_y = rect.center().y;
        let face = Rect::from_min_max(pos2(self.left - CARD_BLEED, rect.top()), pos2(self.right + CARD_BLEED, rect.bottom()));
        let id = self.next_id();
        let response = self.ui.interact(face, id, Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, format!("{name} {kind}")));

        let t = self.tokens;
        let painter = self.ui.painter().clone();
        if response.hovered() {
            painter.rect_filled(face, CornerRadius::same(ROW_RADIUS), t.button);
            painter.text(pos2(self.right - ARROW_INSET, center_y), Align2::CENTER_CENTER, ic::CARET_RIGHT, FontId::proportional(ARROW_SIZE), t.text);
        }
        painter.text(pos2(self.left + GLYPH_CENTER, center_y), Align2::CENTER_CENTER, glyph, FontId::proportional(GLYPH_SIZE), t.text_dim);
        let name_left = self.left + GLYPH_CENTER + GLYPH_TO_NAME;
        let name_rect = painter.text(pos2(name_left, center_y), Align2::LEFT_CENTER, name, FontId::proportional(TEXT_SIZE), t.text);
        painter.text(pos2(name_rect.right() + NAME_TO_KIND, center_y), Align2::LEFT_CENTER, kind, FontId::proportional(TEXT_SIZE), t.text_dim);
        self.drawn(rect.bottom());
        response.clicked()
    }
}
