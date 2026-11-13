//! A comparison across two rows: the left source's dropdown with the operator switch beside it, then the right source's dropdown,
//! each source tagged by its colour on its left edge

use egui::{Align2, Color32, CornerRadius, FontId, Rect, Role, Sense, WidgetInfo, pos2};
use egui_phosphor::regular as ic;

use super::choice::{CARET_INSET, CARET_ROOM, truncated};
use super::{CompareRowResponse, ICON_SIZE, PropertySheet};

/// Width of the operator switch at the right end of the first row
const OPERATOR_SWITCH_WIDTH: f32 = 66.0;

/// Air between the left source and the operator switch
const OPERATOR_AIR: f32 = 6.0;

/// Width of a source's colour tag
const TAG_WIDTH: f32 = 3.0;

/// From a tagged dropdown's left edge to its value
const TAGGED_VALUE_INSET: f32 = 10.0;

impl PropertySheet<'_> {
    /// The two sources of a comparison, `left` tagged `tags[0]` with `operators` beside it, `right` tagged `tags[1]` under it; every list hangs from its own button
    pub(crate) fn compare_rows<T: Copy + PartialEq>(
        &mut self,
        (left, right): (T, T),
        options: &[T],
        name: impl Fn(T) -> String,
        tags: [Color32; 2],
        (operators, operator): (&[&str], usize),
    ) -> CompareRowResponse<T> {
        let first = self.wide_row();
        let switch = Rect::from_x_y_ranges((self.right - OPERATOR_SWITCH_WIDTH)..=self.right, first.y_range());
        let left_button = Rect::from_x_y_ranges(self.left..=(switch.left() - OPERATOR_AIR), first.y_range());
        let left_response = self.tagged_button(left_button, &name(left), tags[0]);
        let left_pick = self.hanging_list(&left_response, left, options, &name, None).0;
        let operator_pick = self.segments(switch, operators, operator, &[], None);

        let second = self.wide_row();
        let right_response = self.tagged_button(second, &name(right), tags[1]);
        let right_pick = self.hanging_list(&right_response, right, options, &name, None).0;
        CompareRowResponse { left: left_pick, right: right_pick, operator: operator_pick }
    }

    /// A dropdown button in `rect` tagged by `tag` along its left edge
    fn tagged_button(&mut self, rect: Rect, value: &str, tag: Color32) -> egui::Response {
        let id = self.next_id();
        let response = self.ui.interact(rect, id, Sense::click());
        response.widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, value));
        let t = self.tokens;
        let painter = self.ui.painter();
        painter.rect_filled(rect, CornerRadius::same(4), if response.hovered() { t.row_hover } else { t.button });
        painter.rect_filled(Rect::from_x_y_ranges(rect.left()..=(rect.left() + TAG_WIDTH), rect.y_range()), CornerRadius::same(1), tag);
        let text_x = rect.left() + TAGGED_VALUE_INSET;
        let galley = truncated(painter, value, rect.right() - CARET_ROOM - text_x, t.text);
        painter.galley(pos2(text_x, rect.center().y - galley.size().y / 2.0), galley, t.text);
        painter.text(pos2(rect.right() - CARET_INSET, rect.center().y), Align2::CENTER_CENTER, ic::CARET_DOWN, FontId::proportional(ICON_SIZE), t.text);
        response
    }
}
