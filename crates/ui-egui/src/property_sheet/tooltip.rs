use egui::{Rect, RectAlign, Response};

use super::PropertySheet;

impl PropertySheet<'_> {
    /// A tooltip for `response` that covers no value: beside the panel at the row's height, on the side with room (left of the panel first)
    pub(crate) fn tip(&self, response: Response, text: &str) -> Response {
        let panel = self.ui.max_rect();
        let row = Rect::from_x_y_ranges(panel.x_range(), response.rect.y_range());
        let mut tooltip = egui::Tooltip::for_enabled(&response);
        tooltip.popup = tooltip.popup.anchor(row).align(RectAlign::LEFT_START).align_alternatives(&[RectAlign::RIGHT_START]);
        tooltip.show(|ui| ui.label(text));
        response
    }
}
