//! The colour picker's rows: its colour space, the saturation/value field beside the hue strip, before | after with the hex code and eyedropper, the model's three sliders and the working-space values

use egui::ecolor::{hsv_from_rgb, rgb_from_hsv};
use egui::{Align2, Color32, CornerRadius, FontId, Painter, Rect, Response, Role, Sense, Stroke, StrokeKind, WidgetInfo, pos2, vec2};
use egui_phosphor::regular as ic;

use super::{CONTROL_HEIGHT, PropertySheet, ROW_HEIGHT, SMALL_TEXT_SIZE, TEXT_SIZE};
use crate::color_picker::{ColorModel, ColorPick, ColorSpace, PickerState, hex, parse_hex};
use crate::fill_material_panel::shown_color as screen_color;
use crate::i18n::tr;
use crate::theme::semibold;
use crate::widgets::gradient;

/// Height of the saturation/value field and the hue strip
const FIELD_HEIGHT: f32 = 150.0;

/// Air above and below the field, like a control's margin in its row
const FIELD_MARGIN: f32 = 2.0;

/// Width of the hue strip at the right edge
const HUE_STRIP_WIDTH: f32 = 14.0;

/// Air between the field and the hue strip
const FIELD_TO_STRIP: f32 = 8.0;

/// Radius of the ring marking the colour in the field
const FIELD_KNOB_RADIUS: f32 = 6.0;

/// How far the hue marker reaches past the strip's sides, and half its height
const HUE_MARKER_OVERHANG: f32 = 2.0;
const HUE_MARKER_HALF_HEIGHT: f32 = 3.0;

/// Width of the before and of the after swatch
const SWATCH_WIDTH: f32 = 30.0;

/// From the left edge to the hex field, and from the hex field to the right edge (the eyedropper's room)
const HEX_LEFT: f32 = 68.0;
const HEX_RIGHT_INSET: f32 = 26.0;

/// From the hex field's left edge to its `#`, and to its digits
const HASH_INSET: f32 = 8.0;
const DIGITS_INSET: f32 = 20.0;

/// From the right edge to the centre of the eyedropper and of the ⋯ menu
const EYEDROPPER_INSET: f32 = 7.0;
const MENU_INSET: f32 = 6.0;

/// Size of the eyedropper and ⋯ glyphs and of their click targets
const GLYPH_SIZE: f32 = 13.0;
const GLYPH_TARGET: f32 = 18.0;

/// A dropdown is as wide as its value plus this (its padding and caret)
const DROPDOWN_ROOM: f32 = 34.0;

/// Pitch of the three sliders (the stack rhythm's part rows)
const SLIDER_ROW: f32 = 22.0;

/// From the left edge to a slider's track, and from its track to the right edge (the value's room)
const TRACK_LEFT: f32 = 18.0;
const TRACK_RIGHT_INSET: f32 = 44.0;

/// Height of a slider's colour track and radius of its knob
const TRACK_HEIGHT: f32 = 6.0;
const KNOB_RADIUS: f32 = 5.0;

/// Width of a slider's typed value
const VALUE_WIDTH: f32 = 40.0;

/// Height of the working-space line (the stack rhythm's label line)
const WORKING_LINE: f32 = 18.0;

/// Ring strokes: the field's ring, the knobs' and the hue marker's outline
const RING_STROKE: f32 = 2.0;
const OUTLINE_STROKE: f32 = 1.5;

impl PropertySheet<'_> {
    /// The picker's rows, editing `state`; `eyedropping` lights the eyedropper while a pick waits for its click
    pub(super) fn color_picker_rows(&mut self, state: &mut PickerState, eyedropping: bool, id: egui::Id) -> ColorPick {
        let mut pick = ColorPick::none(id);
        let mut edited: Option<bool> = None;

        // An option hovered in either list previews in the rows below for this frame only
        let (space, model) = (state.space, state.model);
        let hovered_space = self.space_row(state);
        if let Some(hovered) = hovered_space {
            state.set_space(hovered);
        }
        edited = edited.or(self.field_rows(state));
        let (restored, eyedropper) = self.before_after_row(state, eyedropping);
        edited = edited.or(restored);
        pick.eyedropper = eyedropper;
        let hovered_model = self.model_row(state);
        if let Some(hovered) = hovered_model {
            state.model = hovered;
        }
        edited = edited.or(self.model_sliders(state));
        self.working_line(state);
        if hovered_space.is_some() {
            state.set_space(space);
        }
        if hovered_model.is_some() {
            state.model = model;
        }

        if let Some(merge) = edited {
            pick.picked = Some(state.linear);
            pick.merge = merge;
        }
        pick
    }

    /// The colour space: a dropdown as wide as its longest option, so a hovered option doesn't resize it; the option hovered in its open list
    fn space_row(&mut self, state: &mut PickerState) -> Option<ColorSpace> {
        let center_y = self.take(ROW_HEIGHT).center().y;
        let longest = ColorSpace::ALL.iter().map(|space| self.text_width(space.name(), semibold(TEXT_SIZE))).fold(0.0, f32::max);
        let button = Rect::from_min_size(pos2(self.left, center_y - CONTROL_HEIGHT / 2.0), vec2(longest + DROPDOWN_ROOM, CONTROL_HEIGHT));
        let response = self.dropdown_button(button, state.space.name(), None::<fn(&Painter, Rect)>);
        self.drawn(button.bottom());
        let (picked, hovered) = self.hanging_list(&response, state.space, &ColorSpace::ALL, &|space: ColorSpace| space.name().to_owned(), None);
        if let Some(space) = picked {
            state.set_space(space);
        }
        hovered.filter(|space| *space != state.space)
    }

    /// Saturation across and value up in the field, hue down the strip; Some(merge) when dragged or clicked
    fn field_rows(&mut self, state: &mut PickerState) -> Option<bool> {
        let block = self.take(FIELD_HEIGHT + 2.0 * FIELD_MARGIN);
        let top = block.top() + FIELD_MARGIN;
        let field = Rect::from_min_max(pos2(self.left, top), pos2(self.right - HUE_STRIP_WIDTH - FIELD_TO_STRIP, top + FIELD_HEIGHT));
        let strip = Rect::from_min_max(pos2(self.right - HUE_STRIP_WIDTH, top), pos2(self.right, top + FIELD_HEIGHT));
        let mut edited = None;

        let field_id = self.next_id();
        let response = self.ui.interact(field, field_id, Sense::click_and_drag());
        response.widget_info(|| WidgetInfo::labeled(Role::Slider, true, tr("Saturation and value")));
        if let Some(at) = pressed_at(&response) {
            let [hue, ..] = state.hsv;
            let saturation = ((at.x - field.left()) / field.width()).clamp(0.0, 1.0);
            let value = 1.0 - ((at.y - field.top()) / field.height()).clamp(0.0, 1.0);
            state.set_hsv([hue, saturation, value]);
            edited = Some(continues(&response));
        }

        let strip_id = self.next_id();
        let response = self.ui.interact(strip, strip_id, Sense::click_and_drag());
        response.widget_info(|| WidgetInfo::labeled(Role::Slider, true, tr("Hue")));
        if let Some(at) = pressed_at(&response) {
            let [_, saturation, value] = state.hsv;
            state.set_hsv([((at.y - strip.top()) / strip.height()).clamp(0.0, 1.0), saturation, value]);
            edited = Some(continues(&response));
        }

        let [hue, saturation, value] = state.hsv;
        let space = state.space;
        let painter = self.ui.painter();
        gradient(painter, field, |x, y| space.display(rgb_from_hsv((hue, x, 1.0 - y))));
        painter.circle_stroke(field.lerp_inside(vec2(saturation, 1.0 - value)), FIELD_KNOB_RADIUS, Stroke::new(RING_STROKE, Color32::WHITE));
        gradient(painter, strip, |_, y| rgb_from_hsv((y, 1.0, 1.0)));
        let marker_y = strip.top() + hue * strip.height();
        let marker = Rect::from_min_max(
            pos2(strip.left() - HUE_MARKER_OVERHANG, marker_y - HUE_MARKER_HALF_HEIGHT),
            pos2(strip.right() + HUE_MARKER_OVERHANG, marker_y + HUE_MARKER_HALF_HEIGHT),
        );
        painter.rect_stroke(marker, CornerRadius::same(2), Stroke::new(OUTLINE_STROKE, Color32::WHITE), StrokeKind::Middle);
        self.drawn(field.bottom());
        edited
    }

    /// Before | after (a click on before restores it), the hex field and the eyedropper; Some(false) when restored or typed, and whether the eyedropper was clicked
    fn before_after_row(&mut self, state: &mut PickerState, eyedropping: bool) -> (Option<bool>, bool) {
        let center_y = self.take(ROW_HEIGHT).center().y;
        let t = self.tokens;
        let mut edited = None;

        let before = Rect::from_min_size(pos2(self.left, center_y - CONTROL_HEIGHT / 2.0), vec2(SWATCH_WIDTH, CONTROL_HEIGHT));
        let after = before.translate(vec2(SWATCH_WIDTH, 0.0));
        let before_id = self.next_id();
        let previous = tr("Previous color");
        let response = self.tip(self.ui.interact(before, before_id, Sense::click()), previous);
        response.widget_info(|| WidgetInfo::labeled(Role::Button, true, previous));
        if response.clicked() && state.linear != state.before {
            state.set_linear(state.before);
            edited = Some(false);
        }
        let painter = self.ui.painter();
        painter.rect_filled(before, CornerRadius::ZERO, screen_color(state.before));
        painter.rect_filled(after, CornerRadius::ZERO, screen_color(state.linear));

        let field = Rect::from_min_max(
            pos2(self.left + HEX_LEFT, center_y - CONTROL_HEIGHT / 2.0),
            pos2(self.right - HEX_RIGHT_INSET, center_y + CONTROL_HEIGHT / 2.0),
        );
        if let Some(typed) = self.hex_field(field, state.shown()) {
            state.set_linear(state.space.linear(typed));
            edited = Some(false);
        }

        let glyph = Rect::from_center_size(pos2(self.right - EYEDROPPER_INSET, center_y), vec2(GLYPH_TARGET, GLYPH_TARGET));
        let eyedropper = self.eyedropper_at(glyph, eyedropping, t.text_dim);
        self.drawn(before.bottom());
        (edited, eyedropper)
    }

    /// The bare eyedropper glyph in `glyph` (accent while a pick waits); true when clicked
    pub(super) fn eyedropper_at(&mut self, glyph: Rect, eyedropping: bool, idle: Color32) -> bool {
        let id = self.next_id();
        let label = tr("Pick color");
        let response = self.tip(self.ui.interact(glyph, id, Sense::click()), label);
        response.widget_info(|| WidgetInfo::selected(Role::Button, true, eyedropping, label));
        let t = self.tokens;
        let color = if eyedropping {
            t.accent
        } else if response.hovered() {
            t.text
        } else {
            idle
        };
        self.ui.painter().text(glyph.center(), Align2::CENTER_CENTER, ic::EYEDROPPER, FontId::proportional(GLYPH_SIZE), color);
        response.clicked()
    }

    /// The hex code of `shown` in a field; the values typed, once Enter or a click elsewhere ends the edit
    fn hex_field(&mut self, field: Rect, shown: [f32; 3]) -> Option<[f32; 3]> {
        let id = self.next_id();
        let t = self.tokens;
        let painter = self.ui.painter();
        painter.rect(field, CornerRadius::same(4), t.field, Stroke::new(1.0, t.field_border), StrokeKind::Inside);
        painter.text(pos2(field.left() + HASH_INSET, field.center().y), Align2::LEFT_CENTER, "#", FontId::proportional(TEXT_SIZE), t.text_dim);
        let digits = Rect::from_min_max(pos2(field.left() + DIGITS_INSET, field.top()), field.right_bottom());
        let text_id = id.with("typed");
        let editing = self.ui.data(|data| data.get_temp::<String>(text_id));

        let Some(mut text) = editing else {
            let response = self.ui.interact(digits, id, Sense::click()).on_hover_cursor(egui::CursorIcon::Text);
            response.widget_info(|| WidgetInfo::labeled(Role::TextInput, true, tr("Hex color")));
            self.ui.painter().text(pos2(digits.left(), digits.center().y), Align2::LEFT_CENTER, hex(shown), semibold(TEXT_SIZE), t.text);
            if response.clicked() {
                self.ui.data_mut(|data| data.insert_temp(text_id, hex(shown)));
                self.ui.memory_mut(|memory| memory.request_focus(text_id));
            }
            return None;
        };

        let edit = egui::TextEdit::singleline(&mut text)
            .id(text_id)
            .font(semibold(TEXT_SIZE))
            .frame(egui::Frame::NONE)
            .align(Align2::LEFT_CENTER)
            .margin(vec2(0.0, 0.0));
        let response = self.ui.put(digits, edit);
        if !response.lost_focus() {
            self.ui.data_mut(|data| data.insert_temp(text_id, text));
            return None;
        }

        self.ui.data_mut(|data| data.remove::<String>(text_id));
        if self.ui.input(|input| input.key_pressed(egui::Key::Escape)) {
            return None;
        }
        parse_hex(&text)
    }

    /// The model dropdown, sized to its value, and the ⋯ menu at the right edge; the model hovered in its open list
    fn model_row(&mut self, state: &mut PickerState) -> Option<ColorModel> {
        let center_y = self.take(ROW_HEIGHT).center().y;
        let width = self.text_width(state.model.name(), semibold(TEXT_SIZE)) + DROPDOWN_ROOM;
        let button = Rect::from_min_size(pos2(self.left, center_y - CONTROL_HEIGHT / 2.0), vec2(width, CONTROL_HEIGHT));
        let response = self.dropdown_button(button, state.model.name(), None::<fn(&Painter, Rect)>);
        self.drawn(button.bottom());
        let (picked, hovered) = self.hanging_list(&response, state.model, &ColorModel::ALL, &|model: ColorModel| model.name().to_owned(), None);
        if let Some(model) = picked {
            state.model = model;
        }

        let glyph = Rect::from_center_size(pos2(self.right - MENU_INSET, center_y), vec2(GLYPH_TARGET, GLYPH_TARGET));
        let menu_id = self.next_id();
        let more = tr("More");
        let response = self.ui.interact(glyph, menu_id, Sense::click());
        response.widget_info(|| WidgetInfo::labeled(Role::Button, true, more));
        let t = self.tokens;
        let color = if response.hovered() { t.text } else { t.text_dim };
        self.ui.painter().text(glyph.center(), Align2::CENTER_CENTER, ic::DOTS_THREE, FontId::proportional(GLYPH_SIZE + 1.0), color);
        let shown = state.shown();
        egui::Popup::menu(&response).show(|ui| {
            if ui.button(tr("Copy RGB values")).clicked() {
                let [r, g, b] = shown.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8);
                ui.copy_text(format!("{r}, {g}, {b}"));
            }
            if ui.button(tr("Copy hex color")).clicked() {
                ui.copy_text(format!("#{}", hex(shown)));
            }
        });
        hovered.filter(|model| *model != state.model)
    }

    /// The model's three sliders: each track shows the colours it reaches; Some(merge) when one was dragged or typed
    fn model_sliders(&mut self, state: &mut PickerState) -> Option<bool> {
        let mut edited = None;
        let parts = match state.model {
            ColorModel::Hsv => state.hsv,
            ColorModel::Rgb => state.shown(),
        };
        for (index, letter) in state.model.letters().into_iter().enumerate() {
            let row = self.take(SLIDER_ROW);
            let center_y = row.center().y;
            let t = self.tokens;
            self.ui.painter().text(pos2(self.left, center_y), Align2::LEFT_CENTER, letter, semibold(TEXT_SIZE), t.text_dim);
            let track = Rect::from_min_max(
                pos2(self.left + TRACK_LEFT, center_y - TRACK_HEIGHT / 2.0),
                pos2(self.right - TRACK_RIGHT_INSET, center_y + TRACK_HEIGHT / 2.0),
            );
            let space = state.space;
            let model = state.model;
            gradient(self.ui.painter(), track, |x, _| space.display(model_color(model, parts, index, x)));

            let id = self.next_id();
            let reach = Rect::from_x_y_ranges(track.left() - KNOB_RADIUS..=track.right() + KNOB_RADIUS, row.y_range());
            let response = self.ui.interact(reach, id, Sense::click_and_drag());
            response.widget_info(|| WidgetInfo::slider(true, f64::from(parts.get(index).copied().unwrap_or_default()), letter));
            let mut part = parts.get(index).copied().unwrap_or_default();
            let mut changed = None;
            if let Some(at) = pressed_at(&response) {
                part = ((at.x - track.left()) / track.width()).clamp(0.0, 1.0);
                changed = Some(continues(&response));
            }
            if let Some(typed) = self.value_field_at(id, self.right, VALUE_WIDTH, center_y, part, &(0.0..=1.0), 2, "") {
                part = typed;
                changed = Some(false);
            }
            let knob = pos2(track.left() + track.width() * part, center_y);
            self.ui.painter().circle(knob, KNOB_RADIUS, t.panel, Stroke::new(OUTLINE_STROKE, Color32::WHITE));
            if let Some(merge) = changed {
                set_part(state, index, part);
                edited = edited.or(Some(merge));
            }
            self.drawn(center_y + KNOB_RADIUS);
        }
        edited
    }

    /// The working (linear) values of the colour, small and dim
    fn working_line(&mut self, state: &PickerState) {
        let center_y = self.take(WORKING_LINE).center().y;
        let t = self.tokens;
        let font = FontId::proportional(SMALL_TEXT_SIZE);
        let painter = self.ui.painter();
        let label = painter.text(pos2(self.left, center_y), Align2::LEFT_CENTER, tr("Working color space"), font.clone(), t.text_dim);
        let [r, g, b] = state.linear;
        painter.text(pos2(self.right, center_y), Align2::RIGHT_CENTER, format!("{r:.2}  {g:.2}  {b:.2}"), font, t.text_dim);
        self.drawn(label.bottom());
    }
}

/// Where the pointer presses or drags `response` this frame
fn pressed_at(response: &Response) -> Option<egui::Pos2> {
    response.interact_pointer_pos().filter(|_| response.dragged() || response.clicked())
}

/// A drag past its first frame continues the edit it started
fn continues(response: &Response) -> bool {
    response.dragged() && !response.drag_started()
}

/// The colour `parts` (in `model`) has with part `index` set to `x`, in the shown space; the hue track shows every hue at full strength
fn model_color(model: ColorModel, parts: [f32; 3], index: usize, x: f32) -> [f32; 3] {
    let mut parts = if model == ColorModel::Hsv && index == 0 { [0.0, 1.0, 1.0] } else { parts };
    if let Some(part) = parts.get_mut(index) {
        *part = x;
    }
    match model {
        ColorModel::Hsv => rgb_from_hsv((parts[0], parts[1], parts[2])),
        ColorModel::Rgb => parts,
    }
}

/// Sets part `index` of the colour in the picker's model
fn set_part(state: &mut PickerState, index: usize, part: f32) {
    match state.model {
        ColorModel::Hsv => {
            let mut hsv = state.hsv;
            if let Some(slot) = hsv.get_mut(index) {
                *slot = part;
            }
            state.set_hsv(hsv);
        }
        ColorModel::Rgb => {
            let mut shown = state.shown();
            if let Some(slot) = shown.get_mut(index) {
                *slot = part;
            }
            let (hue, saturation, value) = hsv_from_rgb(shown);
            state.linear = state.space.linear(shown);
            state.hsv = [hue, saturation, value];
        }
    }
}
