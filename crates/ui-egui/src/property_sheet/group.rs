use egui::{Align2, CornerRadius, FontId, Rect, Sense, Shape, Stroke, pos2, vec2};
use egui_phosphor::regular as ic;

use super::row::{EDGE_GLYPH_INSET, RESET_ICON_SIZE};
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
pub(super) const CARD_RADIUS: u8 = 6;

/// From the right edge to the centre of a band's reset button
const BAND_RESET_INSET: f32 = 26.0;

/// From the right edge to where a band's or folded group's value ends
const BAND_VALUE_INSET: f32 = 22.0;

/// Side of the picture before a band's value, and the air between them
const BAND_PICTURE_SIZE: f32 = 16.0;
const BAND_PICTURE_AIR: f32 = 8.0;

/// A band's reset button click square
const BAND_RESET_TARGET: f32 = 20.0;

/// How far a title's text reaches below its centre
const TITLE_HALF_LINE: f32 = 7.0;

impl PropertySheet<'_> {
    /// A top-level group across the whole panel with a caret and a reset (`Some(changed)`) or a value; a click folds it
    pub(crate) fn band(&mut self, title: &str, default_open: bool, reset: Option<bool>, value: Option<&str>) -> BandResponse {
        self.band_drawn(title, default_open, reset, value, None)
    }

    /// [`Self::band`] whose value is led by a picture (Resolution: the size against the texture set's)
    pub(crate) fn band_pictured(&mut self, title: &str, default_open: bool, value: &str, picture: BandPicture<'_>) -> BandResponse {
        self.band_drawn(title, default_open, None, Some(value), Some(picture))
    }

    fn band_drawn(&mut self, title: &str, default_open: bool, reset: Option<bool>, value: Option<&str>, picture: Option<BandPicture<'_>>) -> BandResponse {
        if self.card.is_none() {
            self.space(GROUP_GAP);
        }

        // A band is top-level: it ends a held card too
        self.release_card();

        self.scope = self.id;
        let id = self.named_id("band", title);
        self.band_scope = id;
        self.scope = id;

        let rect = self.take(BAND_HEIGHT);
        let band = Rect::from_x_y_ranges(self.ui.max_rect().x_range(), rect.y_range());

        let mut open = self.ui.data(|data| data.get_temp::<bool>(id)).unwrap_or(default_open);
        let response = self.ui.interact(band, id, Sense::click());
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, title));

        let reset_rect = Rect::from_center_size(pos2(self.right - BAND_RESET_INSET, rect.center().y), vec2(BAND_RESET_TARGET, BAND_RESET_TARGET));
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

        let tokens = self.tokens;
        let painter = self.ui.painter();
        painter.rect_filled(band, CornerRadius::ZERO, self.band_fill());
        painter.line_segment([band.left_top(), band.right_top()], Stroke::new(1.0, tokens.separator));
        painter.line_segment([band.left_bottom(), band.right_bottom()], Stroke::new(1.0, tokens.separator));
        let center_y = rect.center().y;
        painter.text(pos2(self.left, center_y), Align2::LEFT_CENTER, title, semibold(TEXT_SIZE), tokens.text);
        if let Some(changed) = reset {
            // The round section reset, as on a section caption: faint where it does nothing
            let color = self.reset_glyph_color(changed, reset_hovered, tokens.text_strong);
            painter.text(reset_rect.center(), Align2::CENTER_CENTER, ic::CLOCK_COUNTER_CLOCKWISE, FontId::proportional(RESET_ICON_SIZE), color);
        }
        if let Some(value) = value {
            let shown =
                painter.text(pos2(self.right - BAND_VALUE_INSET, center_y), Align2::RIGHT_CENTER, value, FontId::proportional(TEXT_SIZE), tokens.text_dim);
            if let Some(draw) = picture {
                let left = shown.left() - BAND_PICTURE_AIR - BAND_PICTURE_SIZE;
                draw(painter, Rect::from_min_size(pos2(left, center_y - BAND_PICTURE_SIZE / 2.0), vec2(BAND_PICTURE_SIZE, BAND_PICTURE_SIZE)));
            }
        }
        self.caret(pos2(self.right - EDGE_GLYPH_INSET, center_y), open, tokens.text_dim);

        self.drawn(band.bottom());
        self.space(GROUP_GAP);
        self.air_at = Some(self.cursor_y());
        BandResponse { open, reset: reset_clicked }
    }

    /// A group's fold state kept under `id`, flipped by a click on `hit` (answering as `title`); returns whether it is open
    fn fold_toggle(&mut self, id: egui::Id, hit: Rect, title: &str, default_open: bool) -> bool {
        let mut open = self.ui.data(|data| data.get_temp::<bool>(id)).unwrap_or(default_open);
        let response = self.ui.interact(hit, id, Sense::click());
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, title));
        if response.clicked() {
            open = !open;
            self.ui.data_mut(|data| data.insert_temp(id, open));
        }
        open
    }

    /// A row of `rect`'s height across the open card, out to its bled edges
    pub(super) fn bled_row(&self, rect: Rect) -> Rect {
        Rect::from_min_max(pos2(self.left - CARD_BLEED, rect.top()), pos2(self.right + CARD_BLEED, rect.bottom()))
    }

    /// Ends [`Self::hold_card`] and closes the held card
    pub(crate) fn release_card(&mut self) {
        self.held = false;
        self.close_card();
    }

    /// Starts a card with no name row: a slot card, whose slot is its top row; inside a held card it joins that card
    pub(crate) fn open_card(&mut self) {
        if self.held && self.card.is_some() {
            return;
        }

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
        let id = self.named_id("fold", title);
        let rect = self.take(ROW_HEIGHT);
        let open = self.fold_toggle(id, Rect::from_x_y_ranges(self.left..=self.right, rect.y_range()), title, default_open);

        let tokens = self.tokens;
        let center_y = rect.center().y;
        self.ui.painter().text(pos2(self.left, center_y), Align2::LEFT_CENTER, title, semibold(TEXT_SIZE), tokens.text);
        if let Some(value) = value {
            self.ui.painter().text(
                pos2(self.right - BAND_VALUE_INSET, center_y),
                Align2::RIGHT_CENTER,
                value,
                FontId::proportional(TEXT_SIZE),
                tokens.text_dim,
            );
        }
        self.caret(pos2(self.right - EDGE_GLYPH_INSET, center_y), open, tokens.text_dim);
        self.drawn(center_y + TITLE_HALF_LINE);
        open
    }

    /// Ends the open card (not a held one: [`Self::release_card`] ends that): its face reaches [`CARD_PADDING`] below what its rows painted
    pub(crate) fn close_card(&mut self) {
        if self.held {
            return;
        }

        let Some(card) = self.card.take() else { return };

        self.scope = self.band_scope;

        let bottom = self.drawn_bottom + CARD_PADDING;
        let face = Rect::from_min_max(pos2(self.left - CARD_BLEED, card.top), pos2(self.right + CARD_BLEED, bottom));
        self.ui.painter().set(card.face, Shape::rect_filled(face, CornerRadius::same(CARD_RADIUS), self.tokens.row));
        let below = bottom + GROUP_GAP - self.cursor_y();
        self.space(below.max(0.0));
        self.air_at = Some(self.cursor_y());
    }
}
