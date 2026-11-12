//! A uniform colour's bar with its eyedropper, and the colour picker window it opens beside the panel

use egui::{
    Align2, Color32, CornerRadius, FontId, Id, Order, Pos2, Rect, Response, Role, Sense, Shadow, Stroke, StrokeKind, UiBuilder, Vec2, WidgetInfo, pos2, vec2,
};
use egui_phosphor::regular as ic;

use super::{PAD, PropertySheet};
use crate::color_picker::{ColorModel, ColorPick, ColorSpace, PickerState};
use crate::fill_material_panel::shown_color;
use crate::i18n::{tr, trf};

/// Height of a colour bar
const COLOR_BAR_HEIGHT: f32 = 16.0;

/// Room the bar leaves at the right edge for its eyedropper, and from the right edge to the eyedropper's centre
const EYEDROPPER_ROOM: f32 = 26.0;
const EYEDROPPER_INSET: f32 = 9.0;

/// Size of the eyedropper's click target
const EYEDROPPER_TARGET: f32 = 18.0;

/// Air between the panel and the picker standing beside it
const WINDOW_GAP: f32 = 6.0;

/// Height of the picker's title bar, size of its title, and from its right edge to the close glyph's centre
const TITLE_HEIGHT: f32 = 26.0;
const TITLE_TEXT_SIZE: f32 = 11.0;
const TITLE_INSET: f32 = 13.0;

/// Size of the close glyph and of its click target
const CLOSE_GLYPH_SIZE: f32 = 12.0;
const CLOSE_TARGET: f32 = 18.0;

/// From the title bar to the picker's first row, so that row's control sits 10 below it like a card's under its top
const PICKER_TOP: f32 = 8.0;

/// Air under the picker's last line, as under a card's last control
const PICKER_BOTTOM: f32 = 10.0;

/// The picker's height before it was first measured, so it stays on screen beside a bar near the window's bottom
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
            let inside = self.picker_popup(&response, label, &mut state, waiting, id);
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

    /// The open picker: a window the panel's width, its rows at the panel's edges, with `label`'s title, standing beside the panel level with `bar` (its title bar drags it, × or Escape closes it, and so does a press elsewhere unless an eyedropper pick waits)
    fn picker_popup(&mut self, bar: &Response, label: &str, state: &mut PickerState, waiting: bool, id: Id) -> ColorPick {
        let ctx = self.ui.ctx().clone();
        let width = self.right - self.left + 2.0 * PAD;
        let height_id = id.with("picker_height");
        let height = ctx.data(|data| data.get_temp::<f32>(height_id)).unwrap_or(PICKER_HEIGHT_GUESS);
        let screen = ctx.content_rect();
        let panel = Rect::from_x_y_ranges(self.ui.clip_rect().left().min(self.left - PAD)..=self.right + PAD, screen.y_range());
        let corner = state.moved_to.unwrap_or_else(|| beside(panel, bar.rect.top(), vec2(width, height), screen));
        // The title bar stays on screen, so a drag can always bring the window back
        let corner = pos2(
            corner.x.clamp(screen.left(), (screen.right() - width).max(screen.left())),
            corner.y.clamp(screen.top(), (screen.bottom() - TITLE_HEIGHT).max(screen.top())),
        );
        // A press that picks from one of the picker's own lists must not close it
        let list_open = egui::Popup::is_any_open(&ctx);
        let tokens = self.tokens;
        let title = trf("{channel} color", &[("channel", &label)]);

        let shown = egui::Area::new(id.with("picker_area")).order(Order::Foreground).fixed_pos(corner).show(&ctx, |ui| {
            let background = ui.painter().add(egui::Shape::Noop);
            let title_bar = Rect::from_min_size(corner, vec2(width, TITLE_HEIGHT));
            let close = Rect::from_center_size(pos2(title_bar.right() - TITLE_INSET, title_bar.center().y), vec2(CLOSE_TARGET, CLOSE_TARGET));
            let drag = ui.interact(Rect::from_min_max(title_bar.min, pos2(close.left(), title_bar.bottom())), id.with("picker_title"), Sense::drag());
            drag.widget_info(|| WidgetInfo::labeled(Role::TitleBar, true, &title));
            let drag = drag.on_hover_cursor(egui::CursorIcon::Grab);
            let closed = ui.interact(close, id.with("picker_close"), Sense::click());
            closed.widget_info(|| WidgetInfo::labeled(Role::Button, true, tr("Close")));
            let painter = ui.painter();
            painter.text(
                pos2(title_bar.left() + PAD, title_bar.center().y),
                Align2::LEFT_CENTER,
                &title,
                FontId::proportional(TITLE_TEXT_SIZE),
                tokens.text_dim,
            );
            let close_color = if closed.hovered() { tokens.text } else { tokens.text_dim };
            painter.text(close.center(), Align2::CENTER_CENTER, ic::X, FontId::proportional(CLOSE_GLYPH_SIZE), close_color);
            painter.hline(title_bar.x_range(), title_bar.bottom(), Stroke::new(1.0, tokens.separator));

            let top = title_bar.bottom();
            let rows = Rect::from_min_max(pos2(corner.x, top + PICKER_TOP), pos2(corner.x + width, top + PICKER_TOP + screen.height()));
            let (pick, bottom) = ui
                .scope_builder(UiBuilder::new().max_rect(rows), |ui| {
                    PropertySheet::show(ui, id.with("picker_rows"), |sheet| {
                        let pick = sheet.color_picker_rows(state, waiting, id);
                        (pick, sheet.drawn_bottom)
                    })
                })
                .inner;
            let face = Rect::from_min_max(corner, pos2(corner.x + width, bottom + PICKER_BOTTOM));
            let shadow = PICKER_SHADOW.as_shape(face, CornerRadius::same(PICKER_RADIUS));
            let frame =
                egui::epaint::RectShape::new(face, CornerRadius::same(PICKER_RADIUS), tokens.chrome, Stroke::new(1.0, tokens.field_border), StrokeKind::Inside);
            ui.painter().set(background, egui::Shape::Vec(vec![shadow.into(), frame.into()]));
            (pick, face, drag.dragged().then(|| drag.drag_delta()), closed.clicked())
        });
        let (pick, face, dragged, closed) = shown.inner;
        ctx.data_mut(|data| data.insert_temp(height_id, face.height()));
        if let Some(delta) = dragged {
            state.moved_to = Some(corner + delta);
        }

        let pressed_outside =
            ctx.input(|input| input.pointer.primary_pressed() && input.pointer.interact_pos().is_some_and(|at| !face.contains(at) && !bar.rect.contains(at)));
        // The app consumes Escape before the panels draw (painting cancels a stroke), so read the raw event; Escape in the hex field ends the typing first
        let escape = ctx.input(|input| input.raw.events.iter().any(|event| matches!(event, egui::Event::Key { key: egui::Key::Escape, pressed: true, .. })))
            && ctx.memory(|memory| memory.focused().is_none_or(|focused| focused == bar.id));
        if closed || (!list_open && !waiting && (pressed_outside || escape)) {
            state.open = false;
        }
        pick
    }
}

/// Top left corner of a window of `size` beside `panel` (left of it, or right of it when the screen ends first), level with `top` and on screen
fn beside(panel: Rect, top: f32, size: Vec2, screen: Rect) -> Pos2 {
    let left_of = panel.left() - WINDOW_GAP - size.x;
    let x = if left_of >= screen.left() { left_of } else { panel.right() + WINDOW_GAP };
    pos2(x, top.min(screen.bottom() - size.y).max(screen.top()))
}
