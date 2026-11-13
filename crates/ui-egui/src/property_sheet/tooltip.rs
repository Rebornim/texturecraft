use egui::{RectAlign, Response};

use super::PropertySheet;

/// Where a tooltip goes when the window has no room above a control in the panel's right half: below it, then the other edge
const RIGHT_HALF_FALLBACKS: [RectAlign; 3] = [RectAlign::BOTTOM_END, RectAlign::TOP_START, RectAlign::BOTTOM_START];
/// The same for a control in the panel's left half
const LEFT_HALF_FALLBACKS: [RectAlign; 3] = [RectAlign::BOTTOM_START, RectAlign::TOP_END, RectAlign::BOTTOM_END];

impl PropertySheet<'_> {
    /// A tooltip for `response` right next to it: above the control, flush with its edge nearer the panel's side so it grows toward the middle
    pub(crate) fn tip(&self, response: Response, text: &str) -> Response {
        let (align, fallbacks) = if response.rect.center().x > self.ui.max_rect().center().x {
            (RectAlign::TOP_END, &RIGHT_HALF_FALLBACKS)
        } else {
            (RectAlign::TOP_START, &LEFT_HALF_FALLBACKS)
        };
        let mut tooltip = egui::Tooltip::for_enabled(&response);
        tooltip.popup = tooltip.popup.anchor(response.rect).align(align).align_alternatives(fallbacks);
        tooltip.show(|ui| ui.label(text));
        response
    }
}
