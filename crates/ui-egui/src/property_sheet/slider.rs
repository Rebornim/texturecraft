use std::ops::RangeInclusive;

use egui::{Align2, Color32, CornerRadius, FontId, Id, Rect, Response, Sense, Stroke, StrokeKind, WidgetInfo, pos2, vec2};
use egui_phosphor::regular as ic;

use super::value_text::ValueText;
use super::{ICON_SIZE, PropertySheet, RowLead, SMALL_TEXT_SIZE, TEXT_SIZE};
use crate::theme::semibold;

/// Radius of a slider's knob
pub(super) const KNOB_RADIUS: f32 = 5.0;

/// Width of a slider's knob outline
const KNOB_STROKE: f32 = 1.5;

/// Width of a slider's track
const TRACK_WIDTH: f32 = 2.0;

/// Half the height of a signed slider's zero tick
const ZERO_TICK_HALF: f32 = 4.0;

/// Radius of an angle row's dial
const DIAL_RADIUS: f32 = 8.0;

/// Air between the dial and the value, and between the track's end and the dial
const DIAL_AIR: f32 = 6.0;

/// Air between a tinted slider's track end and its right-hand color name
const TINT_WORD_AIR: f32 = 8.0;

/// Half the height of Intensity's divider chip
const CHIP_HALF_HEIGHT: f32 = 10.0;

/// The chip's room around its number: the caret and the air on both sides
const CHIP_PADDING: f32 = 24.0;

/// Air between the chip and the value, beyond the panel's value column
const CHIP_TO_VALUE: f32 = 18.0;

/// What a slider's knob runs along and shows
pub(super) struct Track {
    pub(super) start: f32,
    pub(super) end: f32,
    pub(super) center_y: f32,
    /// Where a two-sided range's fill starts and its tick sits (0 for signed ranges, 0.5 for a 0..1 range centred on "unchanged")
    pub(super) zero: Option<f32>,

    /// The track is a picture of its range (a gray ramp) drawn by the caller: no line and no fill
    pub(super) ramp: bool,
}

impl PropertySheet<'_> {
    /// A bounded number; `Some(merge)` when `value` changed, `merge` true for a drag's later steps
    pub(crate) fn slider(&mut self, label: &str, value: &mut f32, range: RangeInclusive<f32>, decimals: usize) -> Option<bool> {
        self.slider_with_suffix(label, value, range, decimals, "")
    }

    /// [`Self::slider`] with a unit after the value ("°", "%")
    pub(crate) fn slider_with_suffix(&mut self, label: &str, value: &mut f32, range: RangeInclusive<f32>, decimals: usize, suffix: &str) -> Option<bool> {
        self.slider_led(RowLead::label(label), value, range, decimals, suffix)
    }

    /// [`Self::slider_with_suffix`] led by `lead` (a picture row, a child setting)
    pub(crate) fn slider_led(&mut self, lead: RowLead<'_>, value: &mut f32, range: RangeInclusive<f32>, decimals: usize, suffix: &str) -> Option<bool> {
        let (control, center_y) = self.row_with(lead);
        let id = self.next_id();
        let track = Track { start: control.left() + KNOB_RADIUS, end: self.track_end(), center_y, zero: None, ramp: false };
        let dragged = self.track(id, lead.text(), &track, value, &range);
        let typed = self.value_field(id, center_y, *value, &range, decimals, suffix);
        Self::apply_typed(value, typed, dragged)
    }

    /// A signed range: fills from zero and marks it with a tick
    pub(crate) fn signed_slider(&mut self, label: &str, value: &mut f32, range: RangeInclusive<f32>, decimals: usize) -> Option<bool> {
        let (control, center_y) = self.row(label);
        let id = self.next_id();
        let track = Track { start: control.left() + KNOB_RADIUS, end: self.track_end(), center_y, zero: Some(0.0), ramp: false };
        let dragged = self.track(id, label, &track, value, &range);
        let typed = self.value_field(id, center_y, *value, &range, decimals, "");
        Self::apply_typed(value, typed, dragged)
    }

    /// A range whose middle `centre` leaves things as they are: fills from it and marks it with a tick, like a signed range
    pub(crate) fn centered_slider(&mut self, label: &str, value: &mut f32, range: RangeInclusive<f32>, centre: f32, decimals: usize) -> Option<bool> {
        let (control, center_y) = self.row(label);
        let id = self.next_id();
        let track = Track { start: control.left() + KNOB_RADIUS, end: self.track_end(), center_y, zero: Some(centre), ramp: false };
        let dragged = self.track(id, label, &track, value, &range);
        let typed = self.value_field(id, center_y, *value, &range, decimals, "");
        Self::apply_typed(value, typed, dragged)
    }

    /// A −1..1 balance between two tinted color names, left as the label and right after the track (`rights`: the group's right names)
    pub(crate) fn tinted_slider(&mut self, left: (&str, Color32), right: (&str, Color32), rights: &[&str], value: &mut f32) -> Option<bool> {
        let (control, center_y) = self.row_with(RowLead::Tinted { text: left.0, color: left.1 });
        let id = self.next_id();
        let widest = rights.iter().map(|name| self.text_width(name, FontId::proportional(TEXT_SIZE))).fold(0.0, f32::max);
        let word_x = self.track_end() - widest;
        let track = Track { start: control.left() + KNOB_RADIUS, end: word_x - TINT_WORD_AIR, center_y, zero: Some(0.0), ramp: false };
        let range = -1.0..=1.0;
        let dragged = self.track(id, left.0, &track, value, &range);
        self.ui.painter().text(pos2(word_x, center_y), Align2::LEFT_CENTER, right.0, FontId::proportional(TEXT_SIZE), right.1);
        let typed = self.value_field(id, center_y, *value, &range, 2, "");
        Self::apply_typed(value, typed, dragged)
    }

    /// An angle in degrees: the track, then a small dial just before the value
    pub(crate) fn angle(&mut self, label: &str, degrees: &mut f32, range: RangeInclusive<f32>) -> Option<bool> {
        self.angle_led(RowLead::label(label), degrees, range)
    }

    /// [`Self::angle`] led by `lead`
    pub(crate) fn angle_led(&mut self, lead: RowLead<'_>, degrees: &mut f32, range: RangeInclusive<f32>) -> Option<bool> {
        self.angle_shown(lead, degrees, range, false)
    }

    /// An angle added to others: a signed track from −180 to 180 with its zero tick, the dial, and the value with its sign ("+30°")
    pub(crate) fn relative_angle(&mut self, label: &str, degrees: &mut f32) -> Option<bool> {
        self.angle_shown(RowLead::label(label), degrees, -180.0..=180.0, true)
    }

    /// [`Self::angle_led`], the value shown with its sign when `plus`
    fn angle_shown(&mut self, lead: RowLead<'_>, degrees: &mut f32, range: RangeInclusive<f32>, plus: bool) -> Option<bool> {
        let (control, center_y) = self.row_with(lead);
        let id = self.next_id();
        let dial_x = self.right - self.used.value_width.max(super::VALUE_MIN_WIDTH) - DIAL_AIR - DIAL_RADIUS;
        let track = Track {
            start: control.left() + KNOB_RADIUS,
            end: dial_x - DIAL_RADIUS - DIAL_AIR,
            center_y,
            zero: (*range.start() < 0.0).then_some(0.0),
            ramp: false,
        };
        let dragged = self.track(id, lead.text(), &track, degrees, &range);

        let t = self.tokens;
        let painter = self.ui.painter();
        painter.circle_stroke(pos2(dial_x, center_y), DIAL_RADIUS, Stroke::new(KNOB_STROKE, t.button_border));
        let turn = (*degrees - 90.0).to_radians();
        let hand = pos2(dial_x + DIAL_RADIUS * turn.cos(), center_y + DIAL_RADIUS * turn.sin());
        painter.line_segment([pos2(dial_x, center_y), hand], Stroke::new(TRACK_WIDTH, t.text));

        let typed = if plus {
            let shown = format!("{:+.0}°", *degrees);
            self.measure_value(&shown);
            let width = self.used.value_width.max(super::VALUE_MIN_WIDTH);
            self.value_text_field(id, ValueText { right: self.right, width, center_y, shown: &shown, suffix: "°" }, *degrees, &range, 0)
        } else {
            self.value_field(id, center_y, *degrees, &range, 0, "°")
        };
        Self::apply_typed(degrees, typed, dragged)
    }

    /// A slider with a small dropdown chip before its value (Intensity's divider); returns the edit and the chip
    pub(crate) fn slider_with_chip(
        &mut self,
        label: &str,
        value: &mut f32,
        range: RangeInclusive<f32>,
        decimals: usize,
        chip: &str,
        widest_chip: &str,
    ) -> (Option<bool>, Response) {
        let (control, center_y) = self.row(label);
        let id = self.next_id();
        let chip_end = self.right - self.used.value_width.max(super::VALUE_MIN_WIDTH) - CHIP_TO_VALUE;
        let chip_width = self.text_width(widest_chip, FontId::proportional(TEXT_SIZE)) + CHIP_PADDING;
        let chip_rect = Rect::from_min_max(pos2(chip_end - chip_width, center_y - CHIP_HALF_HEIGHT), pos2(chip_end, center_y + CHIP_HALF_HEIGHT));
        let chip_response = self.ui.interact(chip_rect, id.with("chip"), Sense::click());
        chip_response.widget_info(|| WidgetInfo::labeled(egui::Role::ComboBox, true, chip));

        let t = self.tokens;
        let open = egui::Popup::is_id_open(self.ui.ctx(), egui::Popup::default_response_id(&chip_response));
        let painter = self.ui.painter();
        painter.rect_filled(chip_rect, CornerRadius::same(4), if chip_response.hovered() { t.row_hover } else { t.button });
        if open {
            painter.rect_stroke(chip_rect, CornerRadius::same(4), Stroke::new(1.0, t.accent), StrokeKind::Inside);
        }
        painter.text(pos2(chip_end - 9.0, center_y), Align2::CENTER_CENTER, ic::CARET_DOWN, FontId::proportional(ICON_SIZE), t.text);
        painter.text(pos2(chip_end - 18.0, center_y), Align2::RIGHT_CENTER, chip, semibold(SMALL_TEXT_SIZE), t.text);

        let track = Track { start: control.left() + KNOB_RADIUS, end: chip_rect.left() - DIAL_AIR - KNOB_RADIUS, center_y, zero: None, ramp: false };
        let dragged = self.track(id, label, &track, value, &range);
        let typed = self.value_field(id, center_y, *value, &range, decimals, "");
        (Self::apply_typed(value, typed, dragged), chip_response)
    }

    /// A typed value wins over a drag in the same frame and starts its own undo step
    pub(super) fn apply_typed(value: &mut f32, typed: Option<f32>, dragged: Option<bool>) -> Option<bool> {
        match typed {
            Some(typed) => {
                *value = typed;
                Some(false)
            }
            None => dragged,
        }
    }

    /// Paints a track with its knob and moves `value` while it is clicked or dragged
    pub(super) fn track(&mut self, id: Id, label: &str, track: &Track, value: &mut f32, range: &RangeInclusive<f32>) -> Option<bool> {
        let (minimum, maximum) = (*range.start(), *range.end());
        let span = (maximum - minimum).max(f32::EPSILON);
        let length = (track.end - track.start).max(1.0);
        let hit = Rect::from_min_max(pos2(track.start - KNOB_RADIUS, track.center_y - 10.0), pos2(track.end + KNOB_RADIUS, track.center_y + 10.0));
        let response = self.ui.interact(hit, id, Sense::click_and_drag());
        response.widget_info(|| WidgetInfo::slider(true, f64::from(*value), label));

        let mut edit = None;
        if (response.dragged() || response.clicked())
            && let Some(pointer) = response.interact_pointer_pos()
        {
            let picked = (minimum + (pointer.x - track.start) / length * span).clamp(minimum, maximum);
            if picked != *value {
                *value = picked;
                edit = Some(response.dragged() && !response.drag_started());
            }
        }

        let t = self.tokens;
        let at = |v: f32| track.start + ((v - minimum) / span).clamp(0.0, 1.0) * length;
        let knob_x = at(*value);
        let zero_x = track.zero.map_or(track.start, |zero| at(zero.clamp(minimum, maximum)));
        let painter = self.ui.painter();
        if !track.ramp {
            painter.line_segment([pos2(track.start, track.center_y), pos2(track.end, track.center_y)], Stroke::new(TRACK_WIDTH, t.button_border));
            painter.line_segment([pos2(zero_x.min(knob_x), track.center_y), pos2(zero_x.max(knob_x), track.center_y)], Stroke::new(TRACK_WIDTH, t.accent));
        }
        if track.zero.is_some() {
            let tick = [pos2(zero_x, track.center_y - ZERO_TICK_HALF), pos2(zero_x, track.center_y + ZERO_TICK_HALF)];
            painter.line_segment(tick, Stroke::new(1.0, t.text_dim));
        }
        let outline = if response.hovered() || response.dragged() { t.accent } else { t.text };
        painter.circle(pos2(knob_x, track.center_y), KNOB_RADIUS, t.panel, Stroke::new(KNOB_STROKE, outline));
        edit
    }

    /// The value at the row's end; a click turns it into a field, which returns the typed value when done
    pub(super) fn value_field(&mut self, id: Id, center_y: f32, value: f32, range: &RangeInclusive<f32>, decimals: usize, suffix: &str) -> Option<f32> {
        let shown = format!("{value:.decimals$}{suffix}");
        self.measure_value(&shown);
        let width = self.used.value_width.max(super::VALUE_MIN_WIDTH);
        self.value_field_at(id, self.right, width, center_y, value, range, decimals, suffix)
    }

    /// [`Self::value_field`] ending at `right`, `width` wide, outside the panel's value column
    #[allow(clippy::too_many_arguments)]
    pub(super) fn value_field_at(
        &mut self,
        id: Id,
        right: f32,
        width: f32,
        center_y: f32,
        value: f32,
        range: &RangeInclusive<f32>,
        decimals: usize,
        suffix: &str,
    ) -> Option<f32> {
        let shown = format!("{value:.decimals$}{suffix}");
        self.value_text_field(id, ValueText { right, width, center_y, shown: &shown, suffix }, value, range, decimals)
    }

    /// A value drawn as `text.shown` that turns into a field when clicked; returns the typed value (its `text.suffix` and a leading "×" or "x" allowed), kept in `range`
    pub(super) fn value_text_field(&mut self, id: Id, text: ValueText<'_>, value: f32, range: &RangeInclusive<f32>, decimals: usize) -> Option<f32> {
        let ValueText { right, width, center_y, shown, suffix } = text;
        let rect = Rect::from_min_max(pos2(right - width, center_y - 10.0), pos2(right, center_y + 10.0));
        let field_id = id.with("typed");
        let editing = self.ui.data(|data| data.get_temp::<String>(field_id));

        let t = self.tokens;
        let Some(mut text) = editing else {
            let response = self.ui.interact(rect, id.with("value"), Sense::click()).on_hover_cursor(egui::CursorIcon::Text);
            self.ui.painter().text(pos2(right, center_y), Align2::RIGHT_CENTER, shown, semibold(TEXT_SIZE), t.text);
            if response.clicked() {
                self.ui.data_mut(|data| data.insert_temp(field_id, format!("{value:.decimals$}")));
                self.ui.memory_mut(|memory| memory.request_focus(field_id));
            }
            return None;
        };

        let edit = egui::TextEdit::singleline(&mut text).id(field_id).font(semibold(TEXT_SIZE)).align(egui::Align2::RIGHT_CENTER).margin(vec2(2.0, 0.0));
        let response = self.ui.put(rect, edit);
        if !response.lost_focus() {
            self.ui.data_mut(|data| data.insert_temp(field_id, text));
            return None;
        }

        self.ui.data_mut(|data| data.remove::<String>(field_id));
        if self.ui.input(|input| input.key_pressed(egui::Key::Escape)) {
            return None;
        }
        let typed: f32 = text.trim().trim_end_matches(suffix).trim_start_matches(['×', 'x']).trim().parse().ok()?;
        typed.is_finite().then(|| typed.clamp(*range.start(), *range.end()))
    }
}
