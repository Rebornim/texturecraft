use egui::{CornerRadius, Painter, Rect, pos2};

use crate::theme::Tokens;

/// The chosen tab starts this far below the strip's top and reaches this far into the panel
const TAB_BELOW: f32 = 6.0;

/// Corner rounding of the chosen tab
pub(crate) const TAB_ROUNDING: u8 = 5;

/// A tab from `left` to `right` filling `strip`'s whole height and reaching as far into the panel as a section tab: a docked group's shown tab
pub(crate) fn full_height_tab_rect(strip: Rect, left: f32, right: f32) -> Rect {
    Rect::from_min_max(pos2(left, strip.top()), pos2(right, strip.bottom() + TAB_BELOW))
}

/// The shown tab's box over `tab`: the panel's colour, rounded, so it rises from the panel below the strip
pub(crate) fn shown_tab_box(painter: &Painter, tokens: &Tokens, tab: Rect) {
    painter.rect_filled(tab, CornerRadius::same(TAB_ROUNDING), tokens.panel);
}
