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
pub(super) const CARD_RADIUS: u8 = 6;

/// From a card's top to its name's centre
const CARD_NAME_CENTER: f32 = CARD_PADDING + 6.0;

/// Air above a sub-group's name inside a held card, and the name row under it
const INNER_CAPTION_AIR: f32 = 6.0;
const INNER_CAPTION_ROW: f32 = 22.0;

/// From an inner caption's row top to its name's centre
const INNER_CAPTION_CENTER: f32 = 10.0;

/// Air between an inner caption's name and its hairline, and from the hairline's end to the right edge
const INNER_RULE_AIR: f32 = 8.0;
const INNER_RULE_END: f32 = 16.0;

/// Air above the folded row that ends a held card
const CLOSING_FOLD_AIR: f32 = 2.0;

/// The room a rule under a card's slot takes: the hairline 1 px down, then air (the next row starts 6 under the slot)
const SLOT_RULE_ROOM: f32 = 6.0;

/// From the right edge to the centre of a band's reset button
const BAND_RESET_INSET: f32 = 26.0;

/// From the right edge to where a band's or folded group's value ends
const BAND_VALUE_INSET: f32 = 22.0;

/// Side of the picture before a band's value, and the air between them
const BAND_PICTURE_SIZE: f32 = 16.0;
const BAND_PICTURE_AIR: f32 = 8.0;

/// A band's reset button click square
const BAND_RESET_TARGET: f32 = 20.0;

/// How far above and below a foldable card's name a click folds it
const CARD_HIT_HALF_HEIGHT: f32 = 12.0;

/// How far a title's text reaches below its centre
const TITLE_HALF_LINE: f32 = 7.0;

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

        // A band is top-level: it ends a held card too
        self.held = false;
        self.close_card();

        let id = self.next_id();
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
        if self.held && self.card.is_some() {
            return self.inner_caption(title, foldable);
        }

        self.open_card();
        let id = self.next_id();
        let rect = self.take(CARD_NAME_CENTER + ROW_HEIGHT / 2.0);
        let center_y = rect.top() + CARD_NAME_CENTER;
        let mut open = true;
        if let Some(default_open) = foldable {
            open = self.ui.data(|data| data.get_temp::<bool>(id)).unwrap_or(default_open);
            let hit = Rect::from_x_y_ranges(self.left..=self.right, (center_y - CARD_HIT_HALF_HEIGHT)..=(center_y + CARD_HIT_HALF_HEIGHT));
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
        self.drawn(center_y + TITLE_HALF_LINE);
        open
    }

    /// A sub-group inside the held card: its bold name on a hairline (no card in a card), a caret when it folds
    fn inner_caption(&mut self, title: &str, foldable: Option<bool>) -> bool {
        self.space(INNER_CAPTION_AIR);
        let id = self.next_id();
        let rect = self.take(INNER_CAPTION_ROW);
        let center_y = rect.top() + INNER_CAPTION_CENTER;
        let mut open = true;
        if let Some(default_open) = foldable {
            open = self.ui.data(|data| data.get_temp::<bool>(id)).unwrap_or(default_open);
            let hit = Rect::from_x_y_ranges(self.left..=self.right, rect.y_range());
            let response = self.ui.interact(hit, id, Sense::click());
            response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, title));
            if response.clicked() {
                open = !open;
                self.ui.data_mut(|data| data.insert_temp(id, open));
            }
        }

        let t = self.tokens;
        let painter = self.ui.painter();
        let name = painter.text(pos2(self.left, center_y), Align2::LEFT_CENTER, title, semibold(TEXT_SIZE), t.text);
        let rule_end = self.right - INNER_RULE_END;
        if name.right() + INNER_RULE_AIR < rule_end {
            painter.line_segment([pos2(name.right() + INNER_RULE_AIR, center_y), pos2(rule_end, center_y)], Stroke::new(1.0, t.separator));
        }
        if foldable.is_some() {
            self.caret(pos2(self.right - EDGE_GLYPH_INSET, center_y), open, t.text_dim);
        }
        self.drawn(center_y + TITLE_HALF_LINE);
        open
    }

    /// Keeps the card open now open through what follows: groups inside it become inner captions, and slots and cards join it, until [`Self::release_card`]
    pub(crate) fn hold_card(&mut self) {
        self.held = self.card.is_some();
    }

    /// Ends [`Self::hold_card`] and closes the held card
    pub(crate) fn release_card(&mut self) {
        self.held = false;
        self.close_card();
    }

    /// A hairline across the open card under its slot row, then air before its settings
    pub(crate) fn slot_rule(&mut self) {
        let rect = self.take(SLOT_RULE_ROOM);
        let y = rect.top() + 1.0;
        self.ui.painter().line_segment([pos2(self.left, y), pos2(self.right, y)], Stroke::new(1.0, self.tokens.separator));
        self.drawn(y);
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
        self.drawn(center_y + TITLE_HALF_LINE);
        open
    }

    /// The folded row that ends a held card (its Attributes), [`CLOSING_FOLD_AIR`] under the rows before it; returns whether it is open
    pub(crate) fn closing_fold(&mut self, title: &str) -> bool {
        self.space(CLOSING_FOLD_AIR);
        self.fold(title, None, false)
    }

    /// Ends the open card (not a held one: [`Self::release_card`] ends that): its face reaches [`CARD_PADDING`] below what its rows painted
    pub(crate) fn close_card(&mut self) {
        if self.held {
            return;
        }

        let Some(card) = self.card.take() else { return };

        let bottom = self.drawn_bottom + CARD_PADDING;
        let face = Rect::from_min_max(pos2(self.left - CARD_BLEED, card.top), pos2(self.right + CARD_BLEED, bottom));
        self.ui.painter().set(card.face, Shape::rect_filled(face, CornerRadius::same(CARD_RADIUS), self.tokens.row));
        let below = bottom + GROUP_GAP - self.cursor_y();
        self.space(below.max(0.0));
        self.air_at = Some(self.cursor_y());
    }
}
