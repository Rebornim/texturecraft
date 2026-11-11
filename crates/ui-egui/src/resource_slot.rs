//! The box atop an effect's properties that names its resource and opens a picker: thumbnail, a caption over the name, a caret

use egui::{Align2, FontId, Rect, Stroke, Ui, pos2, vec2};
use egui_phosphor::regular as ic;

use crate::theme::Tokens;
use crate::widgets::elided;

/// The slot's height in points
pub(crate) const SLOT_HEIGHT: f32 = 52.0;
const SLOT_ROUNDING: f32 = 3.0;
const BORDER_WIDTH: f32 = 1.0;
const THUMBNAIL_SIZE: f32 = 40.0;
const THUMBNAIL_INSET: f32 = 6.0;
const CAPTION_FONT_SIZE: f32 = 11.0;
const NAME_FONT_SIZE: f32 = 13.0;
/// Gap between the caption's and the name's centers
const LINE_GAP: f32 = 15.0;
const CARET_INSET: f32 = 14.0;
const CARET_FONT_SIZE: f32 = 14.0;
/// Gap between the slot and its keyboard focus outline
const FOCUS_MARGIN: f32 = 2.0;

/// What the slot shows and how: highlighted while hovered or its picker is open, outlined while focused
pub(crate) struct ResourceSlot<'a> {
    pub highlighted: bool,
    pub focused: bool,
    pub thumbnail: Option<&'a egui::TextureHandle>,
    pub caption: &'a str,
    pub name: &'a str,
}

impl ResourceSlot<'_> {
    /// Paints the slot into `rect`
    pub(crate) fn paint(&self, ui: &Ui, rect: Rect) {
        let tk = Tokens::get(ui.ctx());
        let painter = ui.painter();
        let (fill, border) = if self.highlighted { (tk.row_hover, tk.button_border) } else { (tk.field, tk.field_border) };
        painter.rect(rect, SLOT_ROUNDING, fill, Stroke::new(BORDER_WIDTH, border), egui::StrokeKind::Inside);
        if self.focused {
            painter.rect_stroke(rect.expand(FOCUS_MARGIN), SLOT_ROUNDING, ui.visuals().selection.stroke, egui::StrokeKind::Outside);
        }

        if let Some(texture) = self.thumbnail {
            let image = Rect::from_min_size(pos2(rect.left() + THUMBNAIL_INSET, rect.center().y - THUMBNAIL_SIZE * 0.5), vec2(THUMBNAIL_SIZE, THUMBNAIL_SIZE));
            painter.image(texture.id(), image, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), egui::Color32::WHITE);
        }

        // Long names are cut short rather than run under the thumbnail or the caret
        let text_left = rect.left() + THUMBNAIL_INSET * 2.0 + THUMBNAIL_SIZE;
        let text_right = rect.right() - CARET_INSET * 2.0;
        let text_center_x = (text_left + text_right) * 0.5;
        let lines = [(self.caption, CAPTION_FONT_SIZE, tk.text_dim, -LINE_GAP * 0.5), (self.name, NAME_FONT_SIZE, tk.text_strong, LINE_GAP * 0.5)];
        for (text, size, color, offset) in lines {
            let galley = elided(painter, text, size, color, text_right - text_left);
            painter.galley(pos2(text_center_x, rect.center().y + offset) - galley.size() * 0.5, galley, color);
        }

        let caret_color = if self.highlighted { tk.text } else { tk.text_dim };
        painter.text(
            pos2(rect.right() - CARET_INSET, rect.center().y),
            Align2::CENTER_CENTER,
            ic::CARET_DOWN,
            FontId::proportional(CARET_FONT_SIZE),
            caret_color,
        );
    }
}
