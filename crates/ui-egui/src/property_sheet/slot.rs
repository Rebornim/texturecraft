use egui::{Align2, CornerRadius, FontId, Painter, Rect, Sense, Stroke, StrokeKind, pos2, vec2};
use egui_phosphor::regular as ic;

use super::group::CARD_BLEED;
use super::{PropertySheet, ROW_HEIGHT, SlotResponse, TEXT_SIZE};
use crate::theme::semibold;

/// Air between a slot card's top and its slot row
const SLOT_CARD_TOP: f32 = 6.0;

/// Height of a slot row
const SLOT_HEIGHT: f32 = 36.0;

/// Size of a slot's thumbnail
const THUMB_SIZE: f32 = 26.0;

/// Air between the thumbnail and the slot's name
const THUMB_TO_NAME: f32 = 10.0;

/// Size of a slot's source line
const SOURCE_TEXT_SIZE: f32 = 10.0;

/// From the right edge to the slot toggle's centre
const SLOT_TOGGLE_INSET: f32 = 11.0;

/// From the right edge to the × with a toggle beside it, and without
const CLEAR_INSET_WITH_TOGGLE: f32 = 40.0;
const CLEAR_INSET: f32 = 6.0;

/// Distance between the × and the ⟲ left of it
const RESET_STEP: f32 = 22.0;

/// Size of the slot's × and ⟲ glyphs
const SLOT_ICON_SIZE: f32 = 12.0;

/// Room the seed's dice button takes at the row's end, and the button's width
const DICE_ROOM: f32 = 34.0;
const DICE_WIDTH: f32 = 28.0;

/// Air between a full-width button's icon and its words
const BUTTON_ICON_GAP: f32 = 8.0;

impl PropertySheet<'_> {
    /// Opens a slot card with its top row: thumbnail, name over source, then ⟲, × and the toggle on the right; the row opens the source list
    pub(crate) fn slot(
        &mut self,
        thumbnail: &dyn Fn(&Painter, Rect),
        name: &str,
        source: &str,
        on: Option<&mut bool>,
        clear: bool,
        reset: bool,
    ) -> SlotResponse {
        self.open_card();
        self.space(SLOT_CARD_TOP);
        let rect = self.take(SLOT_HEIGHT);
        let center_y = rect.top() + SLOT_HEIGHT / 2.0;
        let enabled = on.as_deref().copied().unwrap_or(true);
        let has_toggle = on.is_some();
        let thumb = Rect::from_min_size(pos2(self.left, center_y - THUMB_SIZE / 2.0), vec2(THUMB_SIZE, THUMB_SIZE));
        let clear_x = self.right - if has_toggle { CLEAR_INSET_WITH_TOGGLE } else { CLEAR_INSET };
        let reset_x = if clear { clear_x - RESET_STEP } else { clear_x };
        // The whole row across the card picks the source, so its list hangs at the card's width; the glyphs on the right take their clicks first
        let row = Rect::from_min_max(pos2(self.left - CARD_BLEED, rect.top()), pos2(self.right + CARD_BLEED, rect.bottom()));
        let pick_id = self.next_id();
        let pick = self.ui.interact(row, pick_id, Sense::click());
        pick.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, name));
        let mut result = SlotResponse { pick, clear: false, reset: false, toggled: false };

        let t = self.tokens;
        let painter = self.ui.painter().clone();
        thumbnail(&painter, thumb);
        painter.rect_stroke(thumb, CornerRadius::ZERO, Stroke::new(1.0, if result.pick.hovered() { t.accent } else { t.field_border }), StrokeKind::Inside);
        let text_x = thumb.right() + THUMB_TO_NAME;
        let (name_font, name_color) = if enabled { (semibold(TEXT_SIZE), t.text) } else { (FontId::proportional(TEXT_SIZE), t.text_dim) };
        painter.text(pos2(text_x, center_y - 7.0), Align2::LEFT_CENTER, name, name_font, name_color);
        painter.text(pos2(text_x, center_y + 8.0), Align2::LEFT_CENTER, source, FontId::proportional(SOURCE_TEXT_SIZE), t.text_dim);

        for (shown, x, glyph, tip, slot) in
            [(clear, clear_x, ic::X, "Clear", &mut result.clear), (reset, reset_x, ic::ARROW_COUNTER_CLOCKWISE, "Restore defaults", &mut result.reset)]
        {
            if !shown {
                continue;
            }
            let glyph_rect = Rect::from_center_size(pos2(x, center_y), vec2(18.0, 18.0));
            let id = self.next_id();
            let response = self.tip(self.ui.interact(glyph_rect, id, Sense::click()), crate::i18n::tr(tip));
            response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, crate::i18n::tr(tip)));
            *slot = response.clicked();
            painter.text(
                glyph_rect.center(),
                Align2::CENTER_CENTER,
                glyph,
                FontId::proportional(SLOT_ICON_SIZE),
                if response.hovered() { t.text } else { t.text_dim },
            );
        }

        if let Some(on) = on {
            let pill = Rect::from_center_size(pos2(self.right - SLOT_TOGGLE_INSET, center_y), vec2(22.0, 13.0));
            let id = self.next_id();
            if self.ui.interact(pill.expand(3.0), id, Sense::click()).clicked() {
                *on = !*on;
                result.toggled = true;
            }
            let (fill, border, knob, knob_x) =
                if *on { (t.accent_dim, t.accent, t.text, pill.center().x + 4.5) } else { (t.button, t.button_border, t.text_dim, pill.center().x - 4.5) };
            painter.rect(pill, CornerRadius::same(6), fill, Stroke::new(1.0, border), StrokeKind::Inside);
            painter.circle_filled(pos2(knob_x, center_y), 4.0, knob);
        }
        self.drawn(thumb.bottom());
        result
    }

    /// A full-width button: centred icon and words
    pub(crate) fn button(&mut self, icon: &str, text: &str) -> egui::Response {
        self.button_lit(icon, text, false)
    }

    /// A full-width button lit in the accent while what it turns on stays on
    pub(crate) fn button_lit(&mut self, icon: &str, text: &str, lit: bool) -> egui::Response {
        let rect = self.take(ROW_HEIGHT);
        let button = self.control_box(self.left, rect.center().y);
        let id = self.next_id();
        let response = self.ui.interact(button, id, Sense::click());
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, text));

        let t = self.tokens;
        let painter = self.ui.painter();
        painter.rect(
            button,
            CornerRadius::same(4),
            match (lit, response.hovered()) {
                (true, _) => t.accent_dim,
                (false, true) => t.row_hover,
                (false, false) => t.button,
            },
            Stroke::new(1.0, if lit { t.accent } else { t.button_border }),
            StrokeKind::Inside,
        );
        let font = FontId::proportional(TEXT_SIZE);
        let icon_galley = painter.layout_no_wrap(icon.to_owned(), FontId::proportional(14.0), t.text);
        let text_galley = painter.layout_no_wrap(text.to_owned(), font, t.text);
        let total = icon_galley.size().x + BUTTON_ICON_GAP + text_galley.size().x;
        let x = button.center().x - total / 2.0;
        painter.galley(pos2(x, button.center().y - icon_galley.size().y / 2.0), icon_galley.clone(), t.text);
        painter.galley(pos2(x + icon_galley.size().x + BUTTON_ICON_GAP, button.center().y - text_galley.size().y / 2.0), text_galley, t.text);
        self.drawn(button.bottom());
        response
    }

    /// A seed: its field (click to type) and the dice button, which picks a new one. Returns true when the seed changed
    pub(crate) fn seed(&mut self, label: &str, seed: &mut u32) -> bool {
        let (control, center_y) = self.row(label);
        let field = Rect::from_min_max(control.min, pos2(self.right - DICE_ROOM, control.bottom()));
        let dice = Rect::from_min_max(pos2(self.right - DICE_WIDTH, control.top()), control.max);
        let id = self.next_id();
        let typing_id = id.with("typed");
        let t = self.tokens;
        let painter = self.ui.painter().clone();
        painter.rect(field, CornerRadius::same(4), t.field, Stroke::new(1.0, t.field_border), StrokeKind::Inside);

        let mut changed = false;
        if let Some(mut text) = self.ui.data(|data| data.get_temp::<String>(typing_id)) {
            let edit = egui::TextEdit::singleline(&mut text)
                .id(typing_id)
                .font(semibold(TEXT_SIZE))
                .frame(egui::Frame::NONE)
                .align(egui::Align2::LEFT_CENTER)
                .margin(vec2(8.0, 0.0));
            let response = self.ui.put(field.shrink(1.0), edit);
            if response.lost_focus() {
                self.ui.data_mut(|data| data.remove::<String>(typing_id));
                if let Ok(typed) = text.trim().parse::<u32>()
                    && typed != *seed
                    && !self.ui.input(|input| input.key_pressed(egui::Key::Escape))
                {
                    *seed = typed;
                    changed = true;
                }
            } else {
                self.ui.data_mut(|data| data.insert_temp(typing_id, text));
            }
        } else {
            let response = self.ui.interact(field, id, Sense::click()).on_hover_cursor(egui::CursorIcon::Text);
            painter.text(pos2(field.left() + 8.0, center_y), Align2::LEFT_CENTER, seed.to_string(), semibold(TEXT_SIZE), t.text);
            if response.clicked() {
                self.ui.data_mut(|data| data.insert_temp(typing_id, seed.to_string()));
                self.ui.memory_mut(|memory| memory.request_focus(typing_id));
            }
        }

        let dice_id = self.next_id();
        let dice_response = self.tip(self.ui.interact(dice, dice_id, Sense::click()), crate::i18n::tr("Random"));
        painter.rect(
            dice,
            CornerRadius::same(4),
            if dice_response.hovered() { t.row_hover } else { t.button },
            Stroke::new(1.0, t.button_border),
            StrokeKind::Inside,
        );
        painter.text(dice.center(), Align2::CENTER_CENTER, ic::DICE_FIVE, FontId::proportional(14.0), t.text);
        dice_response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, crate::i18n::tr("Random")));
        if dice_response.clicked() {
            *seed = next_seed(*seed);
            changed = true;
        }
        changed
    }
}

/// A new seed that looks random but follows from the old one (SplitMix32's mixing step), so tests and replays get the same sequence
pub(crate) fn next_seed(seed: u32) -> u32 {
    let mut z = seed.wrapping_add(0x9e37_79b9);
    z = (z ^ (z >> 16)).wrapping_mul(0x85eb_ca6b);
    z = (z ^ (z >> 13)).wrapping_mul(0xc2b2_ae35);
    z ^ (z >> 16)
}
