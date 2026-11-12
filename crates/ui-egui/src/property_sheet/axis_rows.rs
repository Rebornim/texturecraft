//! Rows a symmetry needs: a section caption with its own switch, sliders lettered by axis, and a picture beside a stack of sliders

use std::ops::RangeInclusive;

use egui::{Align2, Color32, FontId, Painter, Rect, Response, Role, Sense, Stroke, StrokeKind, WidgetInfo, pos2, vec2};

use super::row::SECTION_CAPTION_CENTER;
use super::slider::{KNOB_RADIUS, Track};
use super::{PropertySheet, SMALL_TEXT_SIZE, StackSlider, TEXT_SIZE};
use crate::theme::semibold;

/// From the right edge to the centre of a caption's switch
const CAPTION_SWITCH_INSET: f32 = 14.0;

/// The click area around a caption's switch
const CAPTION_SWITCH_HIT: egui::Vec2 = vec2(30.0, 20.0);

/// From the controls column to where a lettered slider's track starts, past its axis letter
const LETTER_TO_TRACK: f32 = 16.0;

/// From one part of a lettered value to the next
const PART_LINE: f32 = 22.0;

/// A stacked slider's caption line, its track's line, and the air between stacked sliders
const STACK_CAPTION_LINE: f32 = 18.0;
const STACK_PART_LINE: f32 = 22.0;
const STACK_GAP: f32 = 6.0;

/// Air above and below a picture block, like a control's
const BLOCK_MARGIN: f32 = 2.0;

/// The picture's top below the block's (the first caption's letters), how much shorter it is than the stack, and its air before the controls column
const PICTURE_TOP: f32 = 4.0;
const PICTURE_SHORTER: f32 = 10.0;
const PICTURE_TO_COLUMN: f32 = 6.0;

impl PropertySheet<'_> {
    /// A section caption with its on/off switch at the right edge; greyed with both out when not `enabled`
    pub(crate) fn section_switch(&mut self, title: &str, on: bool, enabled: bool) -> Response {
        let t = self.tokens;
        self.section_in(title, false, if enabled { t.text } else { t.text_dim });
        let top = self.sections.last().map_or(self.cursor_y(), |(_, top)| *top);
        let center = pos2(self.right - CAPTION_SWITCH_INSET, top + SECTION_CAPTION_CENTER);
        let id = self.next_id();
        let sense = if enabled { Sense::click() } else { Sense::hover() };
        let response = self.ui.interact(Rect::from_center_size(center, CAPTION_SWITCH_HIT), id, sense);
        response.widget_info(|| WidgetInfo::selected(Role::CheckBox, enabled, on, title));
        self.pill(center, on && enabled, enabled);
        response
    }

    /// One value's parts on lettered signed sliders (Axis position X, Y): the label on the first row, each part's axis letter in its colour; returns the part edited and whether it merges
    pub(crate) fn lettered_sliders(
        &mut self,
        label: &str,
        parts: &mut [(&str, Color32, f32)],
        range: RangeInclusive<f32>,
        decimals: usize,
    ) -> Option<(usize, bool)> {
        let (control, first) = self.row(label);
        let extra = parts.len().saturating_sub(1) as f32 * PART_LINE;
        if extra > 0.0 {
            self.take(extra);
        }
        let mut edit = None;
        for (index, (letter, color, value)) in parts.iter_mut().enumerate() {
            let center_y = first + index as f32 * PART_LINE;
            self.ui.painter().text(pos2(control.left(), center_y), Align2::LEFT_CENTER, *letter, semibold(TEXT_SIZE), *color);
            let id = self.next_id();
            let track = Track { start: control.left() + LETTER_TO_TRACK + KNOB_RADIUS, end: self.track_end(), center_y, signed: true, ramp: false };
            let dragged = self.track(id, &format!("{label} {letter}"), &track, value, &range);
            let typed = self.value_field(id, center_y, *value, &range, decimals, "");
            if let Some(merge) = Self::apply_typed(value, typed, dragged) {
                edit = Some((index, merge));
            }
            self.drawn(center_y + super::CONTROL_HEIGHT / 2.0);
        }
        edit
    }

    /// A picture at the left edge beside captioned sliders in the controls column, as tall as their stack; returns the slider edited and whether it merges
    pub(crate) fn picture_sliders(&mut self, picture: &dyn Fn(&Painter, Rect), sliders: &mut [StackSlider<'_>]) -> Option<(usize, bool)> {
        let count = sliders.len().max(1) as f32;
        let height = count * (STACK_CAPTION_LINE + STACK_PART_LINE) + (count - 1.0) * STACK_GAP;
        self.space(BLOCK_MARGIN);
        let block = self.take(height);
        let x0 = self.column();
        let side = (height - PICTURE_SHORTER).min(x0 - self.left - PICTURE_TO_COLUMN).max(1.0);
        let square = Rect::from_min_size(pos2(self.left, block.top() + PICTURE_TOP), vec2(side, side));
        picture(self.ui.painter(), square);
        self.ui.painter().rect_stroke(square, 0, Stroke::new(1.0, self.tokens.field_border), StrokeKind::Inside);

        let mut edit = None;
        let mut y = block.top();
        for (index, slider) in sliders.iter_mut().enumerate() {
            let dim = self.tokens.text_dim;
            let caption = pos2(x0, y + STACK_CAPTION_LINE / 2.0);
            self.ui.painter().text(caption, Align2::LEFT_CENTER, slider.label, FontId::proportional(SMALL_TEXT_SIZE), dim);
            let center_y = y + STACK_CAPTION_LINE + STACK_PART_LINE / 2.0;
            let id = self.next_id();
            let track = Track { start: x0 + KNOB_RADIUS, end: self.track_end(), center_y, signed: false, ramp: false };
            let dragged = self.track(id, slider.label, &track, slider.value, &slider.range);
            let typed = self.value_field(id, center_y, *slider.value, &slider.range, slider.decimals, slider.suffix);
            if let Some(merge) = Self::apply_typed(slider.value, typed, dragged) {
                edit = Some((index, merge));
            }
            y += STACK_CAPTION_LINE + STACK_PART_LINE + STACK_GAP;
        }
        self.drawn(square.bottom().max(block.bottom() - STACK_PART_LINE / 2.0 + KNOB_RADIUS));
        self.space(BLOCK_MARGIN);
        edit
    }
}
