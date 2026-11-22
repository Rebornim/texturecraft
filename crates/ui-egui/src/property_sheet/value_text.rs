//! Where a slider's value sits and how it reads

/// A value drawn right-aligned at `right` in a `width` wide box centred on `center_y`, reading `shown`; typing may end in `suffix`; it answers as `label`'s value
pub(super) struct ValueText<'a> {
    pub(super) label: &'a dyn std::fmt::Display,
    pub(super) right: f32,
    pub(super) width: f32,
    pub(super) center_y: f32,
    pub(super) shown: &'a str,
    pub(super) suffix: &'a str,
}

/// How far below a value's centre its typing line runs
const LINE_DROP: f32 = 9.0;

/// Where the field `field_id` keeps the text its typing opened with
fn opened_id(field_id: egui::Id) -> egui::Id {
    field_id.with("opened")
}

impl super::PropertySheet<'_> {
    /// Starts typing `text` into the field `field_id`, all of it selected so the first key replaces it
    pub(super) fn start_typing(&mut self, field_id: egui::Id, text: String) {
        let mut state = egui::text_edit::TextEditState::load(self.ui.ctx(), field_id).unwrap_or_default();
        state.cursor.set_char_range(Some(egui::text::CCursorRange::select_all_str(&text)));
        state.store(self.ui.ctx(), field_id);
        self.ui.data_mut(|data| {
            data.insert_temp(opened_id(field_id), text.clone());
            data.insert_temp(field_id, text);
        });
        self.ui.memory_mut(|memory| memory.request_focus(field_id));
    }

    /// Ends typing into the field `field_id`; whether `typed` is still the text it opened with (left so, it changes nothing, even after the value changed meanwhile)
    pub(super) fn end_typing(&mut self, field_id: egui::Id, typed: &str) -> bool {
        let opened = self.ui.data_mut(|data| {
            data.remove::<String>(field_id);
            data.remove_temp::<String>(opened_id(field_id))
        });
        opened.is_some_and(|opened| opened.trim() == typed.trim())
    }

    /// Shows `edit` from `left` to `right` centred on `center_y` (an edit is never shorter than the theme's interact height), so its value sits where the shown value sat; leaves the sheet's rows where they are
    pub(super) fn typing_edit(&mut self, left: f32, right: f32, center_y: f32, edit: egui::TextEdit<'_>) -> egui::Response {
        let line = self.ui.painter().layout_no_wrap("0".to_owned(), crate::theme::semibold(super::TEXT_SIZE), self.tokens.text).size().y;
        let height = line.max(self.ui.spacing().interact_size.y);
        let rect = egui::Rect::from_min_max(egui::pos2(left, center_y - height / 2.0), egui::pos2(right, center_y + height / 2.0));
        let layout = egui::Layout::centered_and_justified(egui::Direction::TopDown);
        let mut child = self.ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(layout));
        child.add(edit.frame(egui::Frame::NONE).margin(egui::vec2(0.0, 0.0)))
    }

    /// Takes the one auto id [`Self::typing_edit`]'s child takes, for a value shown instead of typed: the controls after it keep their ids when typing starts or ends, so a click from one field to the next types into the one clicked
    pub(super) fn skip_typing_edit(&mut self) {
        self.ui.skip_ahead_auto_ids(1);
    }

    /// The 1 px line under a typeable value centred on `center_y`, from `left` to `right`: grey on hover, accent while typing
    pub(super) fn value_line(&self, left: f32, right: f32, center_y: f32, color: egui::Color32) {
        let y = (center_y + LINE_DROP).round() + 0.5;
        self.ui.painter().line_segment([egui::pos2(left, y), egui::pos2(right, y)], egui::Stroke::new(1.0, color));
    }
}
