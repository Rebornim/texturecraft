use egui::{Align2, FontId, Id, Rect, Response, Sense, Stroke, pos2};
use egui_phosphor::regular as ic;

use super::{CONTROL_HEIGHT, ICON_SIZE, LABEL_MAX, LABEL_MIN_AIR, PropertySheet, ROW_HEIGHT, SMALL_TEXT_SIZE, TEXT_SIZE};
use crate::theme::semibold;

/// Air above a stacked label
const STACK_AIR: f32 = 4.0;

/// Height of a stacked label's line, from its air to the control row under it
const STACK_LABEL_LINE: f32 = 14.0;

/// Air above a section caption that follows other rows, and the hairline's distance below it
const SECTION_AIR: f32 = 6.0;

/// The hairline above a later section caption, to the caption's top
const SECTION_RULE_TO_CAPTION: f32 = 12.0;

/// Height of a section caption's line, including the air under it
const SECTION_CAPTION_LINE: f32 = 24.0;

/// Size of a section's menu glyph
const SECTION_MENU_ICON_SIZE: f32 = 14.0;

/// Distance from the right edge to the centre of a right-edge glyph (caret, menu)
pub(super) const EDGE_GLYPH_INSET: f32 = 6.0;

impl PropertySheet<'_> {
    /// A section caption in capitals, a hairline above a later one; `menu` adds the ≡ button
    pub(crate) fn section(&mut self, title: &str, menu: bool) -> Option<Response> {
        self.close_card();
        if !self.empty {
            self.space(SECTION_AIR);
            let y = self.cursor_y();
            let left = self.left;
            let right = self.right;
            let sep = self.tokens.separator;
            self.ui.painter().line_segment([pos2(left, y), pos2(right, y)], Stroke::new(1.0, sep));
            self.space(SECTION_RULE_TO_CAPTION);
        }

        let rect = self.take(SECTION_CAPTION_LINE);
        let center_y = rect.top() + 7.0;
        let text = self.tokens.text;
        self.ui.painter().text(pos2(self.left, center_y), Align2::LEFT_CENTER, title.to_uppercase(), semibold(SMALL_TEXT_SIZE), text);
        self.drawn(center_y + 6.0);
        self.air_at = None;
        if !menu {
            return None;
        }

        let glyph = Rect::from_center_size(pos2(self.right - EDGE_GLYPH_INSET, center_y), egui::vec2(20.0, 20.0));
        let id = self.id.with(("section_menu", title));
        let response = self.ui.interact(glyph, id, Sense::click());
        let color = if response.hovered() { self.tokens.text } else { self.tokens.text_dim };
        self.ui.painter().text(glyph.center(), Align2::CENTER_CENTER, ic::LIST, FontId::proportional(SECTION_MENU_ICON_SIZE), color);
        Some(response)
    }

    /// One setting's row: label beside the controls column or stacked above; returns the control's box and centre
    pub(crate) fn row(&mut self, label: &str) -> (Rect, f32) {
        let x0 = self.lead(label, false);
        let rect = self.take(ROW_HEIGHT);
        let center_y = rect.center().y;
        self.drawn(center_y + CONTROL_HEIGHT / 2.0);
        (self.control_box(x0, center_y), center_y)
    }

    /// Paints `label` and returns where its control starts: the controls column, or the left edge when it stacks
    pub(crate) fn lead(&mut self, label: &str, stack: bool) -> f32 {
        let font = FontId::proportional(TEXT_SIZE);
        let width = self.text_width(label, font.clone());
        if width <= LABEL_MAX {
            self.seen.label_width = self.seen.label_width.max(width);
        }

        let text = self.tokens.text;
        if width <= self.column() - self.left - LABEL_MIN_AIR && !stack {
            let center_y = self.cursor_y() + ROW_HEIGHT / 2.0;
            self.ui.painter().text(pos2(self.left, center_y), Align2::LEFT_CENTER, label, font, text);
            return self.column();
        }

        self.space(STACK_AIR);
        let rect = self.take(STACK_LABEL_LINE);
        self.ui.painter().text(pos2(self.left, rect.top() + 8.0), Align2::LEFT_CENTER, label, font, text);
        self.left
    }

    /// A dim caption line above its control (Offset, Size…)
    pub(crate) fn caption(&mut self, text: &str) {
        let rect = self.take(18.0);
        let dim = self.tokens.text_dim;
        self.ui.painter().text(pos2(self.left, rect.top() + 9.0), Align2::LEFT_CENTER, text, FontId::proportional(SMALL_TEXT_SIZE), dim);
        self.drawn(rect.bottom());
    }

    /// A dim line that says why a group is empty
    pub(crate) fn empty_note(&mut self, text: &str) {
        let rect = self.take(ROW_HEIGHT);
        let dim = self.tokens.text_dim;
        self.ui.painter().text(rect.center(), Align2::CENTER_CENTER, text, FontId::proportional(TEXT_SIZE), dim);
        self.drawn(rect.center().y + 7.0);
    }

    /// A caret pointing down (open) or right (folded), centred on `center`
    pub(super) fn caret(&self, center: egui::Pos2, open: bool, color: egui::Color32) {
        let glyph = if open { ic::CARET_DOWN } else { ic::CARET_RIGHT };
        self.ui.painter().text(center, Align2::CENTER_CENTER, glyph, FontId::proportional(ICON_SIZE), color);
    }

    /// The next control's id: stable while the panel shows the same rows
    pub(super) fn next_id(&mut self) -> Id {
        let id = self.ui.next_auto_id();
        self.ui.skip_ahead_auto_ids(1);
        id
    }
}
