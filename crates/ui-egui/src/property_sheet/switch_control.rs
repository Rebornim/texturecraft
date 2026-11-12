use egui::{Align2, Color32, CornerRadius, FontId, Painter, Rect, Role, Sense, Stroke, StrokeKind, WidgetInfo, pos2, vec2};

use super::{PropertySheet, ROW_HEIGHT, ROW_PICTURE_AIR, ROW_PICTURE_SIZE, SMALL_TEXT_SIZE, TEXT_SIZE};
use crate::theme::semibold;

/// Draws a picture row's picture into its square
type RowPicture<'a> = &'a dyn Fn(&Painter, Rect);

/// Draws option `index`'s picture into its square
type SegmentPicture<'a> = &'a dyn Fn(&Painter, Rect, usize);

/// Side of a picture beside a switch's word, and the air between them
const SEGMENT_PICTURE_SIZE: f32 = 14.0;
const SEGMENT_PICTURE_AIR: f32 = 6.0;

/// Room a switch's word needs in its segment beyond its own width
const SEGMENT_AIR: f32 = 16.0;

/// How far the chosen segment sits inside the switch's box
const SEGMENT_INSET: f32 = 2.0;

/// Size of a toggle's pill
const PILL_SIZE: egui::Vec2 = vec2(22.0, 13.0);

/// Radius of a toggle's knob, and how far it sits from the pill's middle
const TOGGLE_KNOB_RADIUS: f32 = 4.0;
const TOGGLE_KNOB_TRAVEL: f32 = 4.5;

/// Air after a toggle's label, before the column the panel's toggles line up on
const TOGGLE_LABEL_AIR: f32 = 10.0;

/// From that column to the pill's centre
const TOGGLE_CENTER_OFFSET: f32 = 14.0;

/// From one of a pair's toggles to the next, and from its name to its pill's centre
const PAIR_PITCH: f32 = 76.0;
const PAIR_NAME_TO_PILL: f32 = 32.0;

/// Height of a channel pill, and the air between pills
const CHANNEL_PILL_HEIGHT: f32 = 22.0;
const CHANNEL_PILL_GAP: f32 = 4.0;

/// Air under the channel strip
const CHANNEL_STRIP_AIR: f32 = 6.0;

impl PropertySheet<'_> {
    /// Two to four short choices as a boxed switch, or a dropdown when one doesn't fit; returns the picked index
    pub(crate) fn switch(&mut self, label: &str, options: &[&str], selected: usize) -> Option<usize> {
        self.switch_tinted(label, options, selected, &[])
    }

    /// [`Self::switch`] whose chosen word takes its colour from `tints` (axis colours)
    pub(crate) fn switch_tinted(&mut self, label: &str, options: &[&str], selected: usize, tints: &[Color32]) -> Option<usize> {
        let stacks = self.text_width(label, FontId::proportional(TEXT_SIZE)) > self.column() - self.left - super::LABEL_MIN_AIR;
        let x0 = if stacks { self.left } else { self.column() };
        let segment = (self.right - x0) / options.len().max(1) as f32;
        let fits = options.iter().all(|option| self.text_width(option, semibold(SMALL_TEXT_SIZE)) + SEGMENT_AIR <= segment);
        if !fits {
            let indices: Vec<usize> = (0..options.len()).collect();
            let name = |index: usize| options.get(index).copied().unwrap_or_default().to_owned();
            return self.dropdown(label, selected, &indices, name, None).0;
        }

        let (control, _) = self.row(label);
        self.segments(control, options, selected, tints, None)
    }

    /// A boxed switch across the whole panel (a mode switch at the top of a group), each word led by its picture; returns the picked index
    pub(crate) fn picture_switch(&mut self, options: &[&str], selected: usize, picture: SegmentPicture<'_>) -> Option<usize> {
        let row = self.take(ROW_HEIGHT);
        let center_y = row.center().y;
        self.drawn(center_y + super::CONTROL_HEIGHT / 2.0);
        let control = Rect::from_x_y_ranges(self.left..=self.right, (center_y - super::CONTROL_HEIGHT / 2.0)..=(center_y + super::CONTROL_HEIGHT / 2.0));
        self.segments(control, options, selected, &[], Some(picture))
    }

    /// A boxed switch's box and segments in `control`; returns the segment clicked when it was not the chosen one
    pub(super) fn segments(
        &mut self,
        control: Rect,
        options: &[&str],
        selected: usize,
        tints: &[Color32],
        picture: Option<SegmentPicture<'_>>,
    ) -> Option<usize> {
        let segment = control.width() / options.len().max(1) as f32;
        let t = self.tokens;
        self.ui.painter().rect(control, CornerRadius::same(6), t.field, Stroke::new(1.0, t.field_border), StrokeKind::Inside);
        let mut picked = None;
        for (index, option) in options.iter().enumerate() {
            let rect = Rect::from_min_size(pos2(control.left() + index as f32 * segment, control.top()), vec2(segment, control.height()));
            let id = self.next_id();
            let response = self.ui.interact(rect, id, Sense::click());
            let on = index == selected;
            response.widget_info(|| WidgetInfo::selected(Role::RadioButton, true, on, *option));
            if response.clicked() && !on {
                picked = Some(index);
            }

            let painter = self.ui.painter();
            if on {
                painter.rect_filled(rect.shrink(SEGMENT_INSET), CornerRadius::same(4), t.button);
            } else if response.hovered() {
                painter.rect_filled(rect.shrink(SEGMENT_INSET), CornerRadius::same(4), t.row_hover);
            }
            let tint = tints.get(index).copied().unwrap_or(t.text);
            let (font, color) = if on { (semibold(SMALL_TEXT_SIZE), tint) } else { (FontId::proportional(SMALL_TEXT_SIZE), t.text_dim) };
            let Some(draw) = picture else {
                painter.text(rect.center(), Align2::CENTER_CENTER, *option, font, color);
                continue;
            };

            // The picture and the word centred together in the segment
            let word = painter.layout_no_wrap((*option).to_owned(), font, color);
            let left = rect.center().x - (SEGMENT_PICTURE_SIZE + SEGMENT_PICTURE_AIR + word.size().x) / 2.0;
            draw(
                painter,
                Rect::from_min_size(pos2(left, rect.center().y - SEGMENT_PICTURE_SIZE / 2.0), vec2(SEGMENT_PICTURE_SIZE, SEGMENT_PICTURE_SIZE)),
                index,
            );
            let word_left = left + SEGMENT_PICTURE_SIZE + SEGMENT_PICTURE_AIR;
            painter.galley(pos2(word_left, rect.center().y - word.size().y / 2.0), word, color);
        }
        picked
    }

    /// An on/off beside its label; every toggle in a panel lines up past the longest toggle label
    pub(crate) fn toggle(&mut self, label: &str, on: &mut bool) -> bool {
        self.picture_toggle(None, label, on, false)
    }

    /// A toggle row led by a picture (culling rows); a `child` row is dim, its label under its parent's label
    pub(crate) fn picture_toggle(&mut self, picture: Option<RowPicture<'_>>, label: &str, on: &mut bool, child: bool) -> bool {
        let rect = self.take(ROW_HEIGHT);
        let center_y = rect.center().y;
        let label_x = if picture.is_some() || child { self.left + ROW_PICTURE_SIZE + ROW_PICTURE_AIR } else { self.left };
        let font = FontId::proportional(TEXT_SIZE);
        let label_end = label_x - self.left + self.text_width(label, font.clone()) + TOGGLE_LABEL_AIR;
        self.seen.toggle_label_end = self.seen.toggle_label_end.max(label_end);

        let column = (self.column() - self.left).max(self.used.toggle_label_end).max(label_end);
        let pill = Rect::from_center_size(pos2(self.left + column + TOGGLE_CENTER_OFFSET, center_y), PILL_SIZE);
        let hit = Rect::from_min_max(pos2(label_x, rect.top()), pos2(pill.right(), rect.bottom()));
        let id = self.next_id();
        let response = self.ui.interact(hit, id, Sense::click());
        if response.clicked() {
            *on = !*on;
        }
        response.widget_info(|| WidgetInfo::selected(Role::CheckBox, true, *on, label));

        let t = self.tokens;
        let painter = self.ui.painter();
        if let Some(draw) = picture {
            draw(painter, Rect::from_center_size(pos2(self.left + ROW_PICTURE_SIZE / 2.0, center_y), vec2(ROW_PICTURE_SIZE, ROW_PICTURE_SIZE)));
        }
        painter.text(pos2(label_x, center_y), Align2::LEFT_CENTER, label, font, if child { t.text_dim } else { t.text });
        self.pill(pill.center(), *on, true);
        let bottom = if picture.is_some() { ROW_PICTURE_SIZE } else { PILL_SIZE.y } / 2.0;
        self.drawn(center_y + bottom);
        response.clicked()
    }

    /// Two named toggles side by side after one label (Flip copy: U, V); returns the index clicked
    pub(crate) fn toggle_pair(&mut self, label: &str, names: [&str; 2], on: &mut [bool; 2]) -> Option<usize> {
        let (control, center_y) = self.row(label);
        let font = FontId::proportional(TEXT_SIZE);
        let mut clicked = None;
        for (index, (name, state)) in names.into_iter().zip(on.iter_mut()).enumerate() {
            let x = control.left() + index as f32 * PAIR_PITCH;
            let pill = pos2(x + PAIR_NAME_TO_PILL, center_y);
            let hit = Rect::from_min_max(pos2(x, control.top()), pos2(pill.x + PILL_SIZE.x / 2.0, control.bottom()));
            let id = self.next_id();
            let response = self.ui.interact(hit, id, Sense::click());
            response.widget_info(|| WidgetInfo::selected(Role::CheckBox, true, *state, format!("{label} {name}")));
            if response.clicked() {
                *state = !*state;
                clicked = Some(index);
            }
            let t = self.tokens;
            self.ui.painter().text(pos2(x, center_y), Align2::LEFT_CENTER, name, font.clone(), if *state { t.text } else { t.text_dim });
            self.pill(pill, *state, true);
        }
        clicked
    }

    /// A toggle's pill centred on `center`: on (accent), off, or greyed out when not `enabled`
    pub(super) fn pill(&self, center: egui::Pos2, on: bool, enabled: bool) {
        let t = self.tokens;
        let (fill, border, knob, knob_x) = match (enabled, on) {
            (false, _) => (t.field, t.separator, t.separator, center.x - TOGGLE_KNOB_TRAVEL),
            (true, true) => (t.accent_dim, t.accent, t.text, center.x + TOGGLE_KNOB_TRAVEL),
            (true, false) => (t.button, t.button_border, t.text_dim, center.x - TOGGLE_KNOB_TRAVEL),
        };
        let painter = self.ui.painter();
        painter.rect(Rect::from_center_size(center, PILL_SIZE), CornerRadius::same(6), fill, Stroke::new(1.0, border), StrokeKind::Inside);
        painter.circle_filled(pos2(knob_x, center.y), TOGGLE_KNOB_RADIUS, knob);
    }

    /// The channel strip: one pill per channel; returns the clicked index and whether Alt (solo) was held
    pub(crate) fn channel_pills(&mut self, names: &[&str], on: &[bool]) -> Option<(usize, bool)> {
        let rect = self.take(CHANNEL_PILL_HEIGHT);
        let count = names.len().max(1) as f32;
        let width = (self.right - self.left - (count - 1.0) * CHANNEL_PILL_GAP) / count;
        let size =
            if names.iter().all(|name| self.text_width(name, semibold(SMALL_TEXT_SIZE)) + 10.0 <= width) { SMALL_TEXT_SIZE } else { SMALL_TEXT_SIZE - 1.0 };

        let t = self.tokens;
        let mut clicked = None;
        for (index, name) in names.iter().enumerate() {
            let pill = Rect::from_min_size(pos2(self.left + index as f32 * (width + CHANNEL_PILL_GAP), rect.top()), vec2(width, CHANNEL_PILL_HEIGHT));
            let id = self.next_id();
            let response = self.ui.interact(pill, id, Sense::click());
            if response.clicked() {
                clicked = Some((index, self.ui.input(|input| input.modifiers.alt)));
            }

            let enabled = on.get(index).copied().unwrap_or(false);
            response.widget_info(|| WidgetInfo::selected(Role::CheckBox, true, enabled, *name));
            let (fill, border, color, font) =
                if enabled { (t.accent_dim, t.accent, t.text, semibold(size)) } else { (t.field, t.field_border, t.text_dim, FontId::proportional(size)) };
            let painter = self.ui.painter();
            painter.rect(
                pill,
                CornerRadius::same(11),
                if response.hovered() && !enabled { t.row_hover } else { fill },
                Stroke::new(1.0, border),
                StrokeKind::Inside,
            );
            painter.text(pill.center(), Align2::CENTER_CENTER, *name, font, color);
        }
        self.drawn(rect.bottom());
        self.space(CHANNEL_STRIP_AIR);
        self.air_at = Some(self.cursor_y());
        clicked
    }
}
