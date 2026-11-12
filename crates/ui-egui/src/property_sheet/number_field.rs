use egui::{Align2, Color32, CornerRadius, FontId, Rect, Sense, Stroke, StrokeKind, pos2, vec2};
use egui_phosphor::regular as ic;

use super::{ICON_SIZE, PropertySheet, SMALL_TEXT_SIZE, TEXT_SIZE};
use crate::theme::semibold;

/// Air between neighbouring fields of one row
const FIELD_GAP: f32 = 4.0;

/// Room kept at the row's end for the link button, on every row of a group that has one
const LINK_SLOT: f32 = 26.0;

/// Size of the link button
const LINK_SIZE: f32 = 18.0;

/// From a field's left edge to its axis letter's left edge
const TAG_LETTER_INSET: f32 = 7.0;

/// From a field's left edge to its glyph's centre
const TAG_GLYPH_CENTER: f32 = 10.0;

/// From a field's right edge to its value's right end
const VALUE_INSET: f32 = 6.0;

/// One part of a row of number fields: its tag (an axis letter in its colour, or a glyph) and value
pub(crate) struct NumberField<'a> {
    pub(crate) tag: &'a str,

    /// The axis letter's colour; `None` draws `tag` as a dim glyph
    pub(crate) tag_color: Option<Color32>,

    pub(crate) value: f32,

    /// Change per pixel dragged
    pub(crate) speed: f32,

    pub(crate) decimals: usize,
}

impl PropertySheet<'_> {
    /// A row of number fields (drag or type); returns the edited index and whether it continues a drag
    pub(crate) fn number_fields(&mut self, label: &str, fields: &mut [NumberField], link: Option<&mut bool>, keep_link_slot: bool) -> Option<(usize, bool)> {
        let (control, center_y) = self.row(label);
        let slot = if link.is_some() || keep_link_slot { LINK_SLOT } else { 0.0 };
        let count = fields.len().max(1) as f32;
        let width = (control.width() - slot - (count - 1.0) * FIELD_GAP) / count;

        let mut edited = None;
        for (index, field) in fields.iter_mut().enumerate() {
            let left = control.left() + index as f32 * (width + FIELD_GAP);
            let rect = Rect::from_min_size(pos2(left, control.top()), vec2(width, control.height()));
            if let Some(merge) = self.number_field(rect, field) {
                edited = Some((index, merge));
            }
        }

        if let Some(linked) = link {
            let rect = Rect::from_center_size(pos2(self.right - LINK_SIZE / 2.0, center_y), vec2(LINK_SIZE, LINK_SIZE));
            let id = self.next_id();
            let response = self.ui.interact(rect, id, Sense::click());
            if response.clicked() {
                *linked = !*linked;
            }

            let t = self.tokens;
            let (fill, border) = if *linked { (t.accent_dim, t.accent) } else { (t.button, t.button_border) };
            let painter = self.ui.painter();
            painter.rect(rect, CornerRadius::same(4), fill, Stroke::new(1.0, border), StrokeKind::Inside);
            let glyph = if *linked { ic::LINK } else { ic::LINK_BREAK };
            painter.text(rect.center(), Align2::CENTER_CENTER, glyph, FontId::proportional(ICON_SIZE), t.text);
        }
        edited
    }

    /// One field: its tag on the left, the value on the right; drag or click to type
    fn number_field(&mut self, rect: Rect, field: &mut NumberField) -> Option<bool> {
        let id = self.next_id();
        let typing_id = id.with("typed");
        let t = self.tokens;
        let painter = self.ui.painter().clone();
        painter.rect(rect, CornerRadius::same(4), t.field, Stroke::new(1.0, t.field_border), StrokeKind::Inside);
        match field.tag_color {
            Some(color) => {
                painter.text(pos2(rect.left() + TAG_LETTER_INSET, rect.center().y), Align2::LEFT_CENTER, field.tag, semibold(SMALL_TEXT_SIZE), color);
            }
            None => {
                painter.text(
                    pos2(rect.left() + TAG_GLYPH_CENTER, rect.center().y),
                    Align2::CENTER_CENTER,
                    field.tag,
                    FontId::proportional(ICON_SIZE),
                    t.text_dim,
                );
            }
        }

        let decimals = field.decimals;
        if let Some(mut text) = self.ui.data(|data| data.get_temp::<String>(typing_id)) {
            let inner = Rect::from_min_max(pos2(rect.left() + 2.0 * TAG_LETTER_INSET + 6.0, rect.top() + 2.0), pos2(rect.right() - 2.0, rect.bottom() - 2.0));
            let edit = egui::TextEdit::singleline(&mut text)
                .id(typing_id)
                .font(semibold(TEXT_SIZE))
                .align(egui::Align2::RIGHT_CENTER)
                .frame(egui::Frame::NONE)
                .margin(vec2(VALUE_INSET - 2.0, 0.0));
            let response = self.ui.put(inner, edit);
            if !response.lost_focus() {
                self.ui.data_mut(|data| data.insert_temp(typing_id, text));
                return None;
            }

            self.ui.data_mut(|data| data.remove::<String>(typing_id));
            if self.ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                return None;
            }
            let typed: f32 = text.trim().parse().ok()?;
            if !typed.is_finite() || typed == field.value {
                return None;
            }
            field.value = typed;
            return Some(false);
        }

        let response = self.ui.interact(rect, id, Sense::click_and_drag()).on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
        let shown = format!("{:.decimals$}", field.value);
        painter.text(pos2(rect.right() - VALUE_INSET, rect.center().y), Align2::RIGHT_CENTER, shown, semibold(TEXT_SIZE), t.text);
        if response.hovered() || response.dragged() {
            painter.rect_stroke(rect, CornerRadius::same(4), Stroke::new(1.0, t.button_border), StrokeKind::Inside);
        }

        if response.clicked() {
            self.ui.data_mut(|data| data.insert_temp(typing_id, format!("{:.decimals$}", field.value)));
            self.ui.memory_mut(|memory| memory.request_focus(typing_id));
            return None;
        }
        let delta = response.drag_delta().x;
        if !response.dragged() || delta == 0.0 {
            return None;
        }
        field.value += delta * field.speed;
        Some(!response.drag_started())
    }
}
