//! A name typed in place: a field in the controls column, outlined in the accent while it is typed in

use egui::{CornerRadius, Stroke, StrokeKind};

use super::{PropertySheet, TEXT_SIZE};
use crate::theme::semibold;

/// Corner radius of the field, and the air before its text
const FIELD_RADIUS: u8 = 4;
const TEXT_INSET: i8 = 8;

impl PropertySheet<'_> {
    /// A field holding `name`; returns the new name once typing ends (Enter or a click elsewhere) with a changed, non-blank name. Escape keeps the old one
    pub(crate) fn name_field(&mut self, label: &str, name: &str) -> Option<String> {
        let (control, _) = self.row(label);
        let typing_id = self.next_id().with("typed");
        let typing = self.ui.memory(|memory| memory.has_focus(typing_id));
        let mut text = self.ui.data(|data| data.get_temp::<String>(typing_id)).filter(|_| typing).unwrap_or_else(|| name.to_owned());

        let t = self.tokens;
        let outline = Stroke::new(1.0, if typing { t.accent } else { t.field_border });
        self.ui.painter().rect(control, CornerRadius::same(FIELD_RADIUS), t.field, outline, StrokeKind::Inside);
        let edit = egui::TextEdit::singleline(&mut text)
            .id(typing_id)
            .font(semibold(TEXT_SIZE))
            .text_color(t.text)
            .frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(TEXT_INSET, 0)))
            .align(egui::Align2::LEFT_CENTER);
        let response = self.ui.put(control.shrink(1.0), edit);
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, true, label));
        self.drawn(control.bottom());

        if response.has_focus() {
            self.ui.data_mut(|data| data.insert_temp(typing_id, text));
            return None;
        }

        self.ui.data_mut(|data| data.remove::<String>(typing_id));
        let typed = text.trim();
        let escaped = self.ui.input(|input| input.key_pressed(egui::Key::Escape));
        (response.lost_focus() && !escaped && !typed.is_empty() && typed != name).then(|| typed.to_owned())
    }
}
