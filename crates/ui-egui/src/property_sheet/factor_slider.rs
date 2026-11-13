//! A factor slider: ×1 in the middle, halving to the left and doubling to the right

use std::ops::RangeInclusive;

use super::PropertySheet;
use super::slider::{KNOB_RADIUS, Track};
use super::value_text::ValueText;

impl PropertySheet<'_> {
    /// A factor (×1.50) on a track `doublings` halvings or doublings each way from ×1 (marked by the zero tick); a typed value may go past the track's ends, within `typed`. `Some(merge)` when it changed
    pub(crate) fn factor_slider(&mut self, label: &str, factor: &mut f32, doublings: f32, typed: RangeInclusive<f32>) -> Option<bool> {
        let (control, center_y) = self.row(label);
        let id = self.next_id();
        let track = Track { start: control.left() + KNOB_RADIUS, end: self.track_end(), center_y, signed: true, ramp: false };
        let mut along = factor.max(f32::MIN_POSITIVE).log2().clamp(-doublings, doublings);
        let dragged = self.track(id, label, &track, &mut along, &(-doublings..=doublings));
        if dragged.is_some() {
            *factor = along.exp2();
        }
        let shown = format!("×{:.2}", *factor);
        self.measure_value(&shown);
        let width = self.used.value_width.max(super::VALUE_MIN_WIDTH);
        let entered = self.value_text_field(id, ValueText { right: self.right, width, center_y, shown: &shown, suffix: "" }, *factor, &typed, 2);
        Self::apply_typed(factor, entered, dragged)
    }
}
