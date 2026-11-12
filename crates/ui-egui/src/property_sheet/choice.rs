use egui::{Align2, Color32, CornerRadius, FontId, Frame, Margin, Painter, Rect, RectAlign, Response, Role, Sense, Shadow, Stroke, WidgetInfo, pos2, vec2};
use egui_phosphor::regular as ic;

use super::{CONTROL_HEIGHT, ICON_SIZE, PropertySheet, RowLead, TEXT_SIZE};
use crate::theme::semibold;

/// Draws an option's picture into its square
type ChoicePicture<'a, T> = &'a dyn Fn(&Painter, Rect, T);

/// Size of a picture in a dropdown button and in its list
const PICTURE_SIZE: f32 = 18.0;

/// From a picture dropdown's left edge to its picture
const PICTURE_INSET: f32 = 6.0;

/// From a plain dropdown's left edge to its value
const VALUE_INSET: f32 = 8.0;

/// Air between a picture and the words after it
const PICTURE_TO_TEXT: f32 = 8.0;

/// From a dropdown's right edge to its caret's centre
const CARET_INSET: f32 = 10.0;

/// Room a dropdown keeps for its caret: its value is cut with "…" before it
const CARET_ROOM: f32 = 24.0;

/// The open list's border, drawn outside its options: the list's outer edge is its button's
const LIST_BORDER: f32 = 1.0;

/// Height of one option in an open list
const OPTION_HEIGHT: f32 = 26.0;

/// How far an option's hover fill sits inside the list's edges
const HOVER_INSET: f32 = 3.0;

/// From the list's right edge to the hovered option's preview eye
const EYE_INSET: f32 = 16.0;

/// Lists narrower than this have no room for the preview eye
const EYE_MIN_LIST_WIDTH: f32 = 80.0;

/// The lettering of a set-once choice, cut with "…" to fit `room`
fn truncated(painter: &Painter, text: &str, room: f32, color: Color32) -> std::sync::Arc<egui::Galley> {
    let font = semibold(TEXT_SIZE);
    let full = painter.layout_no_wrap(text.to_owned(), font.clone(), color);
    if full.size().x <= room {
        return full;
    }

    let mut cut: String = text.to_owned();
    while cut.pop().is_some() {
        let candidate = painter.layout_no_wrap(format!("{}…", cut.trim_end()), font.clone(), color);
        if candidate.size().x <= room {
            return candidate;
        }
    }
    painter.layout_no_wrap("…".to_owned(), font, color)
}

impl PropertySheet<'_> {
    /// A set-once choice whose list hangs from its button; returns the picked and the hovered option
    pub(crate) fn dropdown<T: Copy + PartialEq>(
        &mut self,
        label: &str,
        current: T,
        options: &[T],
        name: impl Fn(T) -> String,
        picture: Option<ChoicePicture<'_, T>>,
    ) -> (Option<T>, Option<T>) {
        self.dropdown_led(RowLead::label(label), current, options, name, picture)
    }

    /// [`Self::dropdown`] led by `lead` (a label stacked above it, when its values differ only at their end)
    pub(crate) fn dropdown_led<T: Copy + PartialEq>(
        &mut self,
        lead: RowLead<'_>,
        current: T,
        options: &[T],
        name: impl Fn(T) -> String,
        picture: Option<ChoicePicture<'_, T>>,
    ) -> (Option<T>, Option<T>) {
        let x0 = self.lead_with(lead);
        let row = self.take(super::ROW_HEIGHT);
        let center_y = row.center().y;
        self.drawn(center_y + CONTROL_HEIGHT / 2.0);
        let button = self.control_box(x0, center_y);
        let response = self.dropdown_button(button, &name(current), picture.map(|draw| move |painter: &Painter, rect: Rect| draw(painter, rect, current)));
        self.hanging_list(&response, current, options, &name, picture)
    }

    /// Paints a dropdown's button in `rect` and returns its response
    pub(super) fn dropdown_button(&mut self, rect: Rect, value: &str, picture: Option<impl Fn(&Painter, Rect)>) -> Response {
        let id = self.next_id();
        let response = self.ui.interact(rect, id, Sense::click());
        response.widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, value));
        let t = self.tokens;
        let painter = self.ui.painter();
        painter.rect_filled(rect, CornerRadius::same(4), if response.hovered() { t.row_hover } else { t.button });

        let mut text_x = rect.left() + VALUE_INSET;
        if let Some(draw) = picture {
            let picture_rect = Rect::from_min_size(pos2(rect.left() + PICTURE_INSET, rect.center().y - PICTURE_SIZE / 2.0), vec2(PICTURE_SIZE, PICTURE_SIZE));
            draw(painter, picture_rect);
            text_x = picture_rect.right() + PICTURE_TO_TEXT;
        }
        let galley = truncated(painter, value, rect.right() - CARET_ROOM - text_x, t.text);
        painter.galley(pos2(text_x, rect.center().y - galley.size().y / 2.0), galley, t.text);
        painter.text(pos2(rect.right() - CARET_INSET, rect.center().y), Align2::CENTER_CENTER, ic::CARET_DOWN, FontId::proportional(ICON_SIZE), t.text);
        response
    }

    /// The open list of `button`: 1 px below it, its width, over the rows below; returns the picked and the hovered option
    pub(crate) fn hanging_list<T: Copy + PartialEq>(
        &mut self,
        button: &Response,
        current: T,
        options: &[T],
        name: &impl Fn(T) -> String,
        picture: Option<ChoicePicture<'_, T>>,
    ) -> (Option<T>, Option<T>) {
        let t = self.tokens;
        let width = button.rect.width() - 2.0 * LIST_BORDER;
        let frame = Frame::NONE
            .fill(t.chrome)
            .stroke(Stroke::new(LIST_BORDER, t.field_border))
            .corner_radius(CornerRadius::same(6))
            .inner_margin(Margin::symmetric(0, 4))
            .shadow(Shadow { offset: [2, 3], blur: 0, spread: 0, color: Color32::from_black_alpha(140) });
        let shown = egui::Popup::from_toggle_button_response(button).align(RectAlign::BOTTOM_START).gap(1.0).width(width).frame(frame).show(|ui| {
            let mut picked = None;
            let mut hovered = None;
            for &option in options {
                let (rect, response) = ui.allocate_exact_size(vec2(width, OPTION_HEIGHT), Sense::click());
                let painter = ui.painter();
                let chosen = option == current;
                response.widget_info(|| WidgetInfo::selected(Role::ListBoxOption, true, chosen, name(option)));
                if response.hovered() {
                    hovered = Some(option);
                    let fill = Rect::from_min_max(pos2(rect.left() + HOVER_INSET, rect.top()), pos2(rect.right() - HOVER_INSET, rect.bottom() - 2.0));
                    painter.rect_filled(fill, CornerRadius::same(4), t.button);
                }
                if chosen {
                    let bar = Rect::from_min_max(pos2(rect.left() + 2.0, rect.top() + 5.0), pos2(rect.left() + 4.0, rect.bottom() - 7.0));
                    painter.rect_filled(bar, CornerRadius::same(1), t.accent);
                }

                let center_y = rect.top() + OPTION_HEIGHT / 2.0 - 1.0;
                let mut text_x = rect.left() + VALUE_INSET;
                if let Some(draw) = picture {
                    draw(
                        painter,
                        Rect::from_min_size(pos2(rect.left() + PICTURE_INSET, center_y - PICTURE_SIZE / 2.0), vec2(PICTURE_SIZE, PICTURE_SIZE)),
                        option,
                    );
                    text_x = rect.left() + PICTURE_INSET + PICTURE_SIZE + PICTURE_TO_TEXT;
                }
                let font = if chosen { semibold(TEXT_SIZE) } else { FontId::proportional(TEXT_SIZE) };
                painter.text(pos2(text_x, center_y), Align2::LEFT_CENTER, name(option), font, t.text);
                if response.hovered() && width >= EYE_MIN_LIST_WIDTH {
                    painter.text(pos2(rect.right() - EYE_INSET, center_y), Align2::CENTER_CENTER, ic::EYE, FontId::proportional(13.0), t.accent);
                }
                if response.clicked() && !chosen {
                    picked = Some(option);
                }
            }
            (picked, hovered)
        });
        shown.map(|inner| inner.inner).unwrap_or((None, None))
    }
}
