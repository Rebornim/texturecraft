//! A uniform colour's bar with its eyedropper, and the colour picker hanging from the bar over the rows below

use egui::{Color32, CornerRadius, Id, Order, Rect, Response, Role, Sense, Shadow, Stroke, StrokeKind, UiBuilder, WidgetInfo, pos2, vec2};

use super::group::CARD_BLEED;
use super::{PAD, PropertySheet};
use crate::color_picker::{ColorModel, ColorPick, ColorSpace, PickerState};
use crate::fill_material_panel::shown_color;

/// Height of a colour bar
const COLOR_BAR_HEIGHT: f32 = 16.0;

/// Room the bar leaves at the right edge for its eyedropper, and from the right edge to the eyedropper's centre
const EYEDROPPER_ROOM: f32 = 26.0;
const EYEDROPPER_INSET: f32 = 9.0;

/// Size of the eyedropper's click target
const EYEDROPPER_TARGET: f32 = 18.0;

/// Air between the bar and the picker hanging from it
const PICKER_GAP: f32 = 1.0;

/// From the picker's top to its first row, so that row's control sits 10 below the top like a card's
const PICKER_TOP: f32 = 8.0;

/// Air under the picker's last line, as under a card's last control
const PICKER_BOTTOM: f32 = 10.0;

/// The picker's height before it was first measured, so it can open above a bar near the window's bottom
const PICKER_HEIGHT_GUESS: f32 = 340.0;

/// Corner radius of the picker
const PICKER_RADIUS: u8 = 7;

/// The picker's shadow: offset down-right, no blur
const PICKER_SHADOW: Shadow = Shadow { offset: [2, 4], blur: 0, spread: 0, color: Color32::from_rgb(12, 14, 18) };

/// Where the space and model last picked are kept, so the next picker opens with them
const PREFERENCES_KEY: &str = "color_picker_preferences";

impl PropertySheet<'_> {
    /// A uniform colour (linear): a bar across the card that opens the picker, and the eyedropper; `eyedropping` is the bar whose pick waits for its click
    pub(crate) fn color_bar(&mut self, label: &str, linear: [f32; 3], eyedropping: Option<Id>) -> ColorPick {
        let center_y = self.take(super::ROW_HEIGHT).center().y;
        let bar = Rect::from_min_max(pos2(self.left, center_y - COLOR_BAR_HEIGHT / 2.0), pos2(self.right - EYEDROPPER_ROOM, center_y + COLOR_BAR_HEIGHT / 2.0));
        self.color_bar_at(bar, label, linear, eyedropping)
    }

    /// [`Self::color_bar`] in the controls column, beside `label`
    pub(crate) fn color_bar_led(&mut self, label: &str, linear: [f32; 3], eyedropping: Option<Id>) -> ColorPick {
        let (control, center_y) = self.row(label);
        let bar = Rect::from_min_max(
            pos2(control.left(), center_y - COLOR_BAR_HEIGHT / 2.0),
            pos2(control.right() - EYEDROPPER_ROOM, center_y + COLOR_BAR_HEIGHT / 2.0),
        );
        self.color_bar_at(bar, label, linear, eyedropping)
    }

    fn color_bar_at(&mut self, bar: Rect, label: &str, linear: [f32; 3], eyedropping: Option<Id>) -> ColorPick {
        let id = self.next_id();
        let response = self.ui.interact(bar, id, Sense::click());
        response.widget_info(|| WidgetInfo::labeled(Role::Button, true, label));
        let t = self.tokens;
        let border = if response.hovered() { t.accent } else { t.field_border };
        self.ui.painter().rect(bar, CornerRadius::same(3), shown_color(linear), Stroke::new(1.0, border), StrokeKind::Inside);

        let waiting = eyedropping == Some(id);
        let glyph = Rect::from_center_size(pos2(self.right - EYEDROPPER_INSET, bar.center().y), vec2(EYEDROPPER_TARGET, EYEDROPPER_TARGET));
        let mut pick = ColorPick::none(id);
        pick.eyedropper = self.eyedropper_at(glyph, waiting, t.text_dim);
        self.drawn(bar.bottom());

        let ctx = self.ui.ctx().clone();
        let state_id = id.with("picker");
        let mut state = ctx.data(|data| data.get_temp::<PickerState>(state_id)).unwrap_or_else(|| {
            let (space, model) = ctx.data(|data| data.get_temp::<(ColorSpace, ColorModel)>(Id::unique(PREFERENCES_KEY))).unwrap_or_default();
            PickerState { space, model, ..PickerState::default() }
        });
        state.follow(linear);
        if let Some(picked) = ColorPick::delivered(&ctx, id) {
            state.set_linear(picked);
            pick.picked = Some(picked);
        }
        if response.clicked() {
            if state.open {
                state.open = false;
            } else {
                state.open_on(linear);
            }
        }

        if state.open {
            let inside = self.picker_popup(&response, &mut state, waiting, id);
            pick.picked = pick.picked.or(inside.picked);
            pick.merge = inside.merge;
            pick.eyedropper |= inside.eyedropper;
        }
        ctx.data_mut(|data| {
            data.insert_temp(state_id, state);
            data.insert_temp(Id::unique(PREFERENCES_KEY), (state.space, state.model));
        });
        pick
    }

    /// The open picker: the card's width, 1 px under `bar` over the rows below (above it when the window ends first); a click elsewhere or Escape closes it unless an eyedropper pick waits
    fn picker_popup(&mut self, bar: &Response, state: &mut PickerState, waiting: bool, id: Id) -> ColorPick {
        let ctx = self.ui.ctx().clone();
        let (left, right) = (self.left - CARD_BLEED, self.right + CARD_BLEED);
        let height_id = id.with("picker_height");
        let height = ctx.data(|data| data.get_temp::<f32>(height_id)).unwrap_or(PICKER_HEIGHT_GUESS);
        let screen = ctx.content_rect();
        let below = bar.rect.bottom() + PICKER_GAP;
        let above = bar.rect.top() - PICKER_GAP - height;
        let top = if below + height > screen.bottom() && above >= screen.top() { above } else { below };
        // A press that picks from one of the picker's own lists must not close it
        let list_open = egui::Popup::is_any_open(&ctx);
        let tokens = self.tokens;

        let shown = egui::Area::new(id.with("picker_area")).order(Order::Foreground).fixed_pos(pos2(self.left - PAD, top)).show(&ctx, |ui| {
            let background = ui.painter().add(egui::Shape::Noop);
            let rows = Rect::from_min_max(pos2(self.left - PAD, top + PICKER_TOP), pos2(self.right + PAD, top + PICKER_TOP + ctx.content_rect().height()));
            let (pick, bottom) = ui
                .scope_builder(UiBuilder::new().max_rect(rows), |ui| {
                    PropertySheet::show(ui, id.with("picker_rows"), |sheet| {
                        let pick = sheet.color_picker_rows(state, waiting, id);
                        (pick, sheet.drawn_bottom)
                    })
                })
                .inner;
            let face = Rect::from_min_max(pos2(left, top), pos2(right, bottom + PICKER_BOTTOM));
            let shadow = PICKER_SHADOW.as_shape(face, CornerRadius::same(PICKER_RADIUS));
            let frame =
                egui::epaint::RectShape::new(face, CornerRadius::same(PICKER_RADIUS), tokens.chrome, Stroke::new(1.0, tokens.field_border), StrokeKind::Inside);
            ui.painter().set(background, egui::Shape::Vec(vec![shadow.into(), frame.into()]));
            (pick, face)
        });
        let (pick, face) = shown.inner;
        ctx.data_mut(|data| data.insert_temp(height_id, face.height()));

        let pressed_outside =
            ctx.input(|input| input.pointer.primary_pressed() && input.pointer.interact_pos().is_some_and(|at| !face.contains(at) && !bar.rect.contains(at)));
        let escape = ctx.input(|input| input.key_pressed(egui::Key::Escape)) && ctx.memory(|memory| memory.focused().is_none());
        if !list_open && !waiting && (pressed_outside || escape) {
            state.open = false;
        }
        pick
    }
}
