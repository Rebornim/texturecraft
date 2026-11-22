use egui::{CornerRadius, FontId, Sense, Stroke, StrokeKind, pos2};

use super::{ButtonIcon, CONTROL_RADIUS, PropertySheet, ROW_HEIGHT, TEXT_SIZE};

/// Air between a full-width button's icon and its words
const BUTTON_ICON_GAP: f32 = 8.0;

/// Size of a full-width button's icon
const BUTTON_ICON_SIZE: f32 = 14.0;

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
        self.slot_row(thumbnail, name, source, on, clear, reset.then_some(true))
    }

    /// [`Self::slot`]'s row inside the card open now (an input a mode adds, under that mode), not a card of its own
    pub(crate) fn slot_in_card(&mut self, thumbnail: &dyn Fn(&Painter, Rect), name: &str, source: &str, clear: bool) -> SlotResponse {
        self.slot_row(thumbnail, name, source, None, clear, None)
    }

    /// A slot row: thumbnail, name over source, then ⟲ (`Some(changed)`), × and the toggle on the right; the row opens the source list
    fn slot_row(
        &mut self,
        thumbnail: &dyn Fn(&Painter, Rect),
        name: &str,
        source: &str,
        on: Option<&mut bool>,
        clear: bool,
        reset: Option<bool>,
    ) -> SlotResponse {
        let rect = self.take(SLOT_HEIGHT);
        let center_y = rect.top() + SLOT_HEIGHT / 2.0;
        let enabled = on.as_deref().copied().unwrap_or(true);
        let has_toggle = on.is_some();
        let thumb = Rect::from_min_size(pos2(self.left, center_y - THUMB_SIZE / 2.0), vec2(THUMB_SIZE, THUMB_SIZE));
        let clear_x = self.right - if has_toggle { CLEAR_INSET_WITH_TOGGLE } else { CLEAR_INSET };
        let reset_x = if clear { clear_x - RESET_STEP } else { clear_x };

        // The whole row across the card picks the source, so its list hangs at the card's width; the glyphs on the right take their clicks first
        let row = self.bled_row(rect);
        let pick_id = self.next_id();
        let pick = self.ui.interact(row, pick_id, Sense::click());
        pick.widget_info(|| egui::WidgetInfo { current_text_value: Some(source.to_owned()), ..egui::WidgetInfo::labeled(egui::Role::ComboBox, true, name) });
        let mut result = SlotResponse { pick, clear: false, reset: false, toggled: false };

        let tokens = self.tokens;
        let painter = self.ui.painter().clone();
        thumbnail(&painter, thumb);
        painter.rect_stroke(
            thumb,
            CornerRadius::ZERO,
            Stroke::new(1.0, if result.pick.hovered() { tokens.accent } else { tokens.field_border }),
            StrokeKind::Inside,
        );
        let text_x = thumb.right() + THUMB_TO_NAME;
        let (name_font, name_color) = if enabled { (semibold(TEXT_SIZE), tokens.text) } else { (FontId::proportional(TEXT_SIZE), tokens.text_dim) };
        painter.text(pos2(text_x, center_y - NAME_ABOVE_CENTER), Align2::LEFT_CENTER, name, name_font, name_color);
        painter.text(pos2(text_x, center_y + SOURCE_BELOW_CENTER), Align2::LEFT_CENTER, source, FontId::proportional(SOURCE_TEXT_SIZE), tokens.text_dim);

        for (shown, live, x, glyph, tip, slot) in [
            (clear, true, clear_x, ic::X, crate::i18n::n_("Clear"), &mut result.clear),
            (reset.is_some(), reset.unwrap_or(false), reset_x, ic::CLOCK_COUNTER_CLOCKWISE, crate::i18n::n_("Restore defaults"), &mut result.reset),
        ] {
            if !shown {
                continue;
            }

            let glyph_rect = Rect::from_center_size(pos2(x, center_y), Vec2::splat(SLOT_GLYPH_BOX));
            let id = self.next_id();

            // At the defaults the ⟲ is faint and inert, as on a band: a click there picks the source like the rest of the row
            let response = if live {
                self.tip(self.ui.interact(glyph_rect, id, Sense::click()), crate::i18n::tr(tip))
            } else {
                self.ui.interact(glyph_rect, id, Sense::hover())
            };
            response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, live, crate::i18n::tr(tip)));
            *slot = live && response.clicked();
            let color = self.reset_glyph_color(live, response.hovered(), tokens.text);
            painter.text(glyph_rect.center(), Align2::CENTER_CENTER, glyph, FontId::proportional(SLOT_ICON_SIZE), color);
        }

        if let Some(on) = on {
            let pill = Rect::from_center_size(pos2(self.right - SLOT_TOGGLE_INSET, center_y), PILL_SIZE);
            let id = self.next_id();
            let response = self.ui.interact(pill.expand(TOGGLE_HIT_MARGIN), id, Sense::click());
            if response.clicked() {
                *on = !*on;
                result.toggled = true;
            }
            response.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, true, *on, name));
            self.pill(pill.center(), *on, true);
        }
        self.drawn(thumb.bottom());
        result
    }

    /// A full-width button: centred icon and words
    pub(crate) fn button<'a>(&mut self, icon: impl Into<ButtonIcon<'a>>, text: &str) -> egui::Response {
        self.button_lit(icon, text, false)
    }

    /// A full-width button lit in the accent while what it turns on stays on
    pub(crate) fn button_lit<'a>(&mut self, icon: impl Into<ButtonIcon<'a>>, text: &str, lit: bool) -> egui::Response {
        self.button_enabled(icon, text, lit, true)
    }

    /// [`Self::button_lit`] that answers no clicks and dims its words while not `enabled`
    pub(crate) fn button_enabled<'a>(&mut self, icon: impl Into<ButtonIcon<'a>>, text: &str, lit: bool, enabled: bool) -> egui::Response {
        self.button_sensing(icon.into(), text, lit, if enabled { Sense::click() } else { Sense::hover() })
    }

    fn button_sensing(&mut self, icon: ButtonIcon<'_>, text: &str, lit: bool, sense: Sense) -> egui::Response {
        let enabled = sense != Sense::hover();
        let rect = self.take(ROW_HEIGHT);
        let button = self.control_box(self.left, rect.center().y);
        let id = self.next_id();
        let response = self.ui.interact(button, id, sense);
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, text));

        let tokens = self.tokens;
        let painter = self.ui.painter();
        painter.rect(
            button,
            CornerRadius::same(CONTROL_RADIUS),
            match (lit, enabled && response.hovered()) {
                (true, _) => tokens.accent_dim,
                (false, true) => tokens.row_hover,
                (false, false) => tokens.button,
            },
            Stroke::new(1.0, if lit { tokens.accent } else { tokens.button_border }),
            StrokeKind::Inside,
        );
        let color = if enabled { tokens.text } else { tokens.text_dim };
        let font = FontId::proportional(TEXT_SIZE);
        let (icon_width, icon_galley) = match icon {
            ButtonIcon::Glyph(glyph) => {
                let galley = painter.layout_no_wrap(glyph.to_owned(), FontId::proportional(BUTTON_ICON_SIZE), color);
                (galley.size().x, Some(galley))
            }
            ButtonIcon::Drawn(_) => (BUTTON_ICON_SIZE, None),
        };
        let text_galley = painter.layout_no_wrap(text.to_owned(), font, color);
        let total = icon_width + BUTTON_ICON_GAP + text_galley.size().x;
        let x = button.center().x - total / 2.0;
        if let Some(galley) = icon_galley {
            painter.galley(pos2(x, button.center().y - galley.size().y / 2.0), galley, color);
        }
        if let ButtonIcon::Drawn(drawn) = icon {
            drawn.paint(painter, pos2(x + icon_width / 2.0, button.center().y), BUTTON_ICON_SIZE, color);
        }
        painter.galley(pos2(x + icon_width + BUTTON_ICON_GAP, button.center().y - text_galley.size().y / 2.0), text_galley, color);
        self.drawn(button.bottom());
        response
    }
}
