//! One ID color of a color selection: its swatch (the eyedropper that picks it again), its R G B, and a × while hovered

use egui::{Align2, Color32, CornerRadius, FontId, Rect, Sense, Stroke, StrokeKind, pos2, vec2};
use egui_phosphor::regular as ic;

use super::group::CARD_BLEED;
use super::{IdColorRowResponse, PropertySheet, ROW_HEIGHT, SMALL_TEXT_SIZE, TEXT_SIZE};
use crate::drawn_icon::DrawnIcon;

/// Height of the swatch
const SWATCH_HEIGHT: f32 = 18.0;

/// Air between the swatch and the controls column, where the readouts start
const SWATCH_TO_COLUMN: f32 = 12.0;

/// Room the × keeps at the row's end
const REMOVE_ROOM: f32 = 24.0;

/// From the right edge to the ×'s centre, and the size of the row's glyphs
const REMOVE_INSET: f32 = 6.0;
const GLYPH_SIZE: f32 = 13.0;
/// Size of the pipette on a hovered swatch
const PIPETTE_SIZE: f32 = 14.0;

/// From a component's letter to its value
const LETTER_TO_VALUE: f32 = 12.0;

/// Corner radius of the hovered row's face and of the swatch
const ROW_RADIUS: u8 = 6;
const SWATCH_RADIUS: u8 = 3;

/// Above this luminance the swatch's eyedropper is drawn dark
const LIGHT_SWATCH: f32 = 0.5;

impl PropertySheet<'_> {
    /// The row of ID color `rgb`; `repicking` outlines it while its pick runs, `enabled` is false without an ID map to pick from
    pub(crate) fn id_color_row(&mut self, rgb: [u8; 3], repicking: bool, enabled: bool) -> IdColorRowResponse {
        let rect = self.take(ROW_HEIGHT);
        let center_y = rect.center().y;
        let face = Rect::from_min_max(pos2(self.left - CARD_BLEED, rect.top()), pos2(self.right + CARD_BLEED, rect.bottom()));
        let hovered = self.ui.rect_contains_pointer(face);
        let swatch =
            Rect::from_min_max(pos2(self.left, center_y - SWATCH_HEIGHT / 2.0), pos2(self.column() - SWATCH_TO_COLUMN, center_y + SWATCH_HEIGHT / 2.0));
        let [red, green, blue] = rgb;
        let readout = format!("R {red} G {green} B {blue}");

        let t = self.tokens;
        if hovered || repicking {
            let outline = if repicking { Stroke::new(1.0, t.accent) } else { Stroke::NONE };
            self.ui.painter().rect(face, CornerRadius::same(ROW_RADIUS), t.row, outline, StrokeKind::Inside);
        }

        let swatch_id = self.next_id();
        let sense = if enabled { Sense::click() } else { Sense::hover() };
        let pick_tip = crate::i18n::tr("Pick color");
        let swatch_response = self.tip(self.ui.interact(swatch, swatch_id, sense), pick_tip);
        swatch_response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, format!("{pick_tip} {readout}")));
        let color = Color32::from_rgb(red, green, blue);
        let painter = self.ui.painter().clone();
        painter.rect_filled(swatch, CornerRadius::same(SWATCH_RADIUS), color);
        if enabled && (hovered || repicking) {
            let luminance = (0.2126 * f32::from(red) + 0.7152 * f32::from(green) + 0.0722 * f32::from(blue)) / 255.0;
            let glyph = if luminance > LIGHT_SWATCH { Color32::from_gray(20) } else { Color32::from_gray(235) };
            DrawnIcon::Pipette.paint(&painter, swatch.center(), PIPETTE_SIZE, glyph);
        }

        let step = (self.right - REMOVE_ROOM - self.column()) / 3.0;
        for (index, (letter, value)) in ["R", "G", "B"].into_iter().zip(rgb).enumerate() {
            let x = self.column() + step * index as f32;
            painter.text(pos2(x, center_y), Align2::LEFT_CENTER, letter, FontId::proportional(SMALL_TEXT_SIZE), t.text_dim);
            painter.text(pos2(x + LETTER_TO_VALUE, center_y), Align2::LEFT_CENTER, value.to_string(), FontId::proportional(TEXT_SIZE), t.text);
        }

        // The × answers only while the row is hovered, so it never takes a click meant for the row under a moving pointer
        let remove = Rect::from_center_size(pos2(self.right - REMOVE_INSET, center_y), vec2(18.0, 18.0));
        let remove_id = self.next_id();
        let remove_tip = crate::i18n::tr("Remove");
        let remove_response = self.ui.interact(remove, remove_id, if hovered { Sense::click() } else { Sense::hover() });
        remove_response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, hovered, format!("{remove_tip} {readout}")));
        let remove_response = self.tip(remove_response, remove_tip);
        if hovered {
            painter.text(
                remove.center(),
                Align2::CENTER_CENTER,
                ic::X,
                FontId::proportional(GLYPH_SIZE - 1.0),
                if remove_response.hovered() { t.text } else { t.text_dim },
            );
        }
        self.drawn(swatch.bottom());
        IdColorRowResponse { repick: swatch_response.clicked(), remove: hovered && remove_response.clicked() }
    }
}
