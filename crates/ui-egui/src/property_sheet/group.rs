use egui::{Align2, CornerRadius, FontId, Rect, Sense, Shape, Stroke, pos2, vec2};
use egui_phosphor::regular as ic;

use super::row::{EDGE_GLYPH_INSET, RESET_AT_DEFAULT_OPACITY, RESET_ICON_SIZE};
use super::{BandResponse, GROUP_GAP, OpenCard, PropertySheet, ROW_HEIGHT, TEXT_SIZE};
use crate::theme::semibold;

/// Draws a band's picture in its rect
type BandPicture<'a> = &'a dyn Fn(&egui::Painter, Rect);

/// Height of a top-level band
const BAND_HEIGHT: f32 = 30.0;

/// A card's padding above its name and below its last control
const CARD_PADDING: f32 = 10.0;

/// How far a card's face reaches past its rows on both sides
pub(super) const CARD_BLEED: f32 = 6.0;

/// Corner radius of a card
const CARD_RADIUS: u8 = 6;

/// From a card's top to its name's centre
const CARD_NAME_CENTER: f32 = CARD_PADDING + 6.0;

/// From the right edge to the centre of a band's reset button
const BAND_RESET_INSET: f32 = 26.0;

/// From the right edge to where a band's or folded group's value ends
const BAND_VALUE_INSET: f32 = 22.0;

/// Side of the picture before a band's value, and the air between them
const BAND_PICTURE_SIZE: f32 = 16.0;
const BAND_PICTURE_AIR: f32 = 8.0;

impl PropertySheet<'_> {
    /// A top-level group across the whole panel with a caret and a reset (`Some(changed)`) or a value; a click folds it
    pub(crate) fn band(&mut self, title: &str, default_open: bool, reset: Option<bool>, value: Option<&str>) -> BandResponse {
        self.band_drawn(title, default_open, reset, value, None)
    }

    /// [`Self::band`] whose value is led by a picture (Resolution: the size against the texture set's)
    pub(crate) fn band_pictured(&mut self, title: &str, default_open: bool, value: &str, picture: &dyn Fn(&egui::Painter, Rect)) -> BandResponse {
        self.band_drawn(title, default_open, None, Some(value), Some(picture))
    }

    fn band_drawn(&mut self, title: &str, default_open: bool, reset: Option<bool>, value: Option<&str>, picture: Option<BandPicture<'_>>) -> BandResponse {
        if self.card.is_none() {
            self.space(GROUP_GAP);
        }
        self.close_card();

        let id = self.next_id();
        let rect = self.take(BAND_HEIGHT);
        let band = Rect::from_x_y_ranges(self.ui.max_rect().x_range(), rect.y_range());
        let mut open = self.ui.data(|data| data.get_temp::<bool>(id)).unwrap_or(default_open);
        let response = self.ui.interact(band, id, Sense::click());
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, title));

        let reset_rect = Rect::from_center_size(pos2(self.right - BAND_RESET_INSET, rect.center().y), vec2(20.0, 20.0));
        let mut reset_clicked = false;
        let mut reset_hovered = false;
        if let Some(changed) = reset {
            // At the defaults it is inert: a click there folds the band like the rest of it
            let sense = if changed { Sense::click() } else { Sense::hover() };
            let reset_response = self.ui.interact(reset_rect, id.with("reset"), sense);
            if changed {
                // Right above the glyph, as far right as it: over the card or toggles above the band, never a value
                let mut tooltip = egui::Tooltip::for_enabled(&reset_response);
                tooltip.popup = tooltip.popup.anchor(reset_rect).align(egui::RectAlign::TOP_END).align_alternatives(&[egui::RectAlign::LEFT]);
                tooltip.show(|ui| ui.label(crate::i18n::tr("Restore defaults")));
            }
            reset_response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, changed, crate::i18n::tr("Restore defaults")));
            reset_clicked = changed && reset_response.clicked();
            reset_hovered = reset_response.hovered();
        }
        if response.clicked() && !reset_clicked {
            open = !open;
            self.ui.data_mut(|data| data.insert_temp(id, open));
        }

        let t = self.tokens;
        let painter = self.ui.painter();
        painter.rect_filled(band, CornerRadius::ZERO, self.band_fill());
        painter.line_segment([band.left_top(), band.right_top()], Stroke::new(1.0, t.separator));
        painter.line_segment([band.left_bottom(), band.right_bottom()], Stroke::new(1.0, t.separator));
        let center_y = rect.center().y;
        painter.text(pos2(self.left, center_y), Align2::LEFT_CENTER, title, semibold(TEXT_SIZE), t.text);
        if let Some(changed) = reset {
            // The round section reset, as on a section caption: faint where it does nothing
            let color = match (changed, reset_hovered) {
                (false, _) => t.text_dim.gamma_multiply(RESET_AT_DEFAULT_OPACITY),
                (true, true) => t.text_strong,
                (true, false) => t.text_dim,
            };
            painter.text(reset_rect.center(), Align2::CENTER_CENTER, ic::CLOCK_COUNTER_CLOCKWISE, FontId::proportional(RESET_ICON_SIZE), color);
        }
        if let Some(value) = value {
            let shown = painter.text(pos2(self.right - BAND_VALUE_INSET, center_y), Align2::RIGHT_CENTER, value, FontId::proportional(TEXT_SIZE), t.text_dim);
            if let Some(draw) = picture {
                let left = shown.left() - BAND_PICTURE_AIR - BAND_PICTURE_SIZE;
                draw(painter, Rect::from_min_size(pos2(left, center_y - BAND_PICTURE_SIZE / 2.0), vec2(BAND_PICTURE_SIZE, BAND_PICTURE_SIZE)));
            }
        }
        self.caret(pos2(self.right - EDGE_GLYPH_INSET, center_y), open, t.text_dim);

        self.drawn(band.bottom());
        self.space(GROUP_GAP);
        self.air_at = Some(self.cursor_y());
        BandResponse { open, reset: reset_clicked }
    }

    /// A folded group inside a card: bold name, its value (dim) and a caret; returns whether it is open
    pub(crate) fn card(&mut self, title: &str, foldable: Option<bool>) -> bool {
        self.open_card();
        let id = self.next_id();
        let rect = self.take(CARD_NAME_CENTER + ROW_HEIGHT / 2.0);
        let center_y = rect.top() + CARD_NAME_CENTER;
        let mut open = true;
        if let Some(default_open) = foldable {
            open = self.ui.data(|data| data.get_temp::<bool>(id)).unwrap_or(default_open);
            let hit = Rect::from_x_y_ranges(self.left..=self.right, (center_y - 12.0)..=(center_y + 12.0));
            let response = self.ui.interact(hit, id, Sense::click());
            response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, title));
            if response.clicked() {
                open = !open;
                self.ui.data_mut(|data| data.insert_temp(id, open));
            }
        }

        let t = self.tokens;
        self.ui.painter().text(pos2(self.left, center_y), Align2::LEFT_CENTER, title, semibold(TEXT_SIZE), t.text);
        if foldable.is_some() {
            self.caret(pos2(self.right - EDGE_GLYPH_INSET, center_y), open, t.text_dim);
        }
        self.drawn(center_y + 7.0);
        open
    }

    /// Starts a card with no name row: a slot card, whose slot is its top row
    pub(crate) fn open_card(&mut self) {
        if self.card.is_none() && self.air_at != Some(self.cursor_y()) && !self.empty {
            self.space(GROUP_GAP);
        }
        self.close_card();
        let face = self.ui.painter().add(Shape::Noop);
        self.card = Some(OpenCard { top: self.cursor_y(), face });
        self.drawn_bottom = self.cursor_y();
    }

    /// A folded group inside a card: its bold name, the value it holds (dim) and a caret. Returns whether it is open
    pub(crate) fn fold(&mut self, title: &str, value: Option<&str>, default_open: bool) -> bool {
        let id = self.next_id();
        let rect = self.take(ROW_HEIGHT);
        let mut open = self.ui.data(|data| data.get_temp::<bool>(id)).unwrap_or(default_open);
        let hit = Rect::from_x_y_ranges(self.left..=self.right, rect.y_range());
        let response = self.ui.interact(hit, id, Sense::click());
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, title));
        if response.clicked() {
            open = !open;
            self.ui.data_mut(|data| data.insert_temp(id, open));
        }

        let t = self.tokens;
        let center_y = rect.center().y;
        self.ui.painter().text(pos2(self.left, center_y), Align2::LEFT_CENTER, title, semibold(TEXT_SIZE), t.text);
        if let Some(value) = value {
            self.ui.painter().text(pos2(self.right - BAND_VALUE_INSET, center_y), Align2::RIGHT_CENTER, value, FontId::proportional(TEXT_SIZE), t.text_dim);
        }
        self.caret(pos2(self.right - EDGE_GLYPH_INSET, center_y), open, t.text_dim);
        self.drawn(center_y + 7.0);
        open
    }

    /// Ends the open card: its face reaches [`CARD_PADDING`] below what its rows painted
    pub(crate) fn close_card(&mut self) {
        let Some(card) = self.card.take() else { return };

        let bottom = self.drawn_bottom + CARD_PADDING;
        let face = Rect::from_min_max(pos2(self.left - CARD_BLEED, card.top), pos2(self.right + CARD_BLEED, bottom));
        self.ui.painter().set(card.face, Shape::rect_filled(face, CornerRadius::same(CARD_RADIUS), self.tokens.row));
        let below = bottom + GROUP_GAP - self.cursor_y();
        self.space(below.max(0.0));
        self.air_at = Some(self.cursor_y());
    }
}
