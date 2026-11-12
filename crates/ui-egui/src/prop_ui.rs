//! The Properties panel's building blocks: tabs on the panel's top edge, section headers, rows
//! with a label column, dropdowns whose options carry a picture of what they do, one-line value
//! sliders and folded groups that show their settings' pictures while closed.

use egui::{Align2, Color32, CornerRadius, FontId, Id, Painter, Rect, Response, Sense, Stroke, Ui, pos2, vec2};

use crate::theme::{Tokens, semibold};

/// Height of the tab strip on the panel's top edge
const TAB_STRIP_HEIGHT: f32 = 30.0;
/// Space between a tab's icon and its word, and on either side of them
const TAB_PADDING: f32 = 12.0;
const TAB_ICON_GAP: f32 = 6.0;
/// How far a chosen tab reaches down into the panel, joining it
const TAB_JOIN: f32 = 4.0;
const TAB_CORNER_RADIUS: u8 = 5;
const TAB_FONT_SIZE: f32 = 12.0;

/// Width of the label column of a row
pub(crate) const LABEL_WIDTH: f32 = 82.0;
/// Height of one control row
pub(crate) const ROW_HEIGHT: f32 = 26.0;
/// Space between rows
pub(crate) const ROW_GAP: f32 = 6.0;

const SECTION_FONT_SIZE: f32 = 11.0;
/// Space above a section header and between it and its first row
const SECTION_GAP_ABOVE: f32 = 8.0;
const SECTION_GAP_BELOW: f32 = 8.0;

const CONTROL_CORNER_RADIUS: u8 = 4;
const CONTROL_FONT_SIZE: f32 = 13.0;
/// Size of the picture in a dropdown's closed value and in its open options
pub(crate) const PICTURE_SIZE: f32 = 18.0;
const OPTION_PICTURE_SIZE: f32 = 22.0;
const OPTION_HEIGHT: f32 = 32.0;
/// Inner padding of a dropdown's face and of its options
const CONTROL_PADDING: f32 = 7.0;
const CARET_SIZE: f32 = 12.0;

/// Width kept for a slider's value at its track's end
const VALUE_WIDTH: f32 = 46.0;
const TRACK_WIDTH: f32 = 2.0;
const KNOB_RADIUS: f32 = 5.0;
const KNOB_STROKE: f32 = 1.5;

const FOLDED_HEIGHT: f32 = 26.0;
const FOLDED_PICTURE_SIZE: f32 = 16.0;
const FOLDED_PICTURE_GAP: f32 = 6.0;
const FOLDED_CORNER_RADIUS: u8 = 5;

/// Tabs on the panel's top edge, each an icon and a word; the chosen one joins the panel below
/// it. The choice is kept per `id`; returns the chosen tab's index.
pub(crate) fn edge_tabs(ui: &mut Ui, id: Id, tabs: &[(&str, &str)]) -> usize {
    let tokens = Tokens::get(ui.ctx());
    let mut chosen = ui.data(|data| data.get_temp::<usize>(id)).unwrap_or(0).min(tabs.len().saturating_sub(1));
    let (strip, _) = ui.allocate_exact_size(vec2(ui.available_width(), TAB_STRIP_HEIGHT), Sense::hover());
    ui.painter().rect_filled(strip, CornerRadius::ZERO, tokens.chrome);
    let font = semibold(TAB_FONT_SIZE);
    let mut x = strip.left();
    for (index, (icon, word)) in tabs.iter().enumerate() {
        let on = index == chosen;
        let icon_galley = ui.painter().layout_no_wrap((*icon).to_string(), FontId::proportional(TAB_FONT_SIZE + 1.0), tokens.text);
        let word_galley = ui.painter().layout_no_wrap((*word).to_string(), font.clone(), tokens.text);
        let width = TAB_PADDING * 2.0 + icon_galley.size().x + TAB_ICON_GAP + word_galley.size().x;
        let tab = Rect::from_min_size(pos2(x, strip.top() + TAB_JOIN), vec2(width, TAB_STRIP_HEIGHT - TAB_JOIN));
        let response = ui.interact(tab, id.with(index), Sense::click());
        if on {
            let joined = Rect::from_min_max(tab.min, pos2(tab.right(), strip.bottom() + TAB_JOIN));
            ui.painter().rect_filled(joined, CornerRadius { nw: TAB_CORNER_RADIUS, ne: TAB_CORNER_RADIUS, sw: 0, se: 0 }, tokens.panel);
        } else if response.hovered() {
            ui.painter().rect_filled(tab, CornerRadius { nw: TAB_CORNER_RADIUS, ne: TAB_CORNER_RADIUS, sw: 0, se: 0 }, tokens.row_hover);
        }
        let color = if on { tokens.text } else { tokens.text_dim };
        let center_y = tab.center().y;
        let icon_x = tab.left() + TAB_PADDING;
        ui.painter().text(pos2(icon_x, center_y), Align2::LEFT_CENTER, *icon, FontId::proportional(TAB_FONT_SIZE + 1.0), color);
        let word_font = if on { font.clone() } else { FontId::proportional(TAB_FONT_SIZE) };
        ui.painter().text(pos2(icon_x + icon_galley.size().x + TAB_ICON_GAP, center_y), Align2::LEFT_CENTER, *word, word_font, color);
        if response.clicked() && !on {
            chosen = index;
            ui.data_mut(|data| data.insert_temp(id, chosen));
        }
        response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, on, *word));
        x += width;
    }
    ui.add_space(SECTION_GAP_BELOW);
    chosen
}

/// A section header: an uppercase SemiBold title with a divider above it (none for the
/// `first`); returns the response of its menu icon when `menu` is given.
pub(crate) fn section_header(ui: &mut Ui, title: &str, first: bool, menu: Option<&str>) -> Option<Response> {
    let tokens = Tokens::get(ui.ctx());
    if !first {
        ui.add_space(SECTION_GAP_ABOVE);
        let (line, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
        ui.painter().hline(line.x_range(), line.center().y, Stroke::new(1.0, tokens.separator));
        ui.add_space(SECTION_GAP_ABOVE);
    }
    let mut menu_response = None;
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(title.to_uppercase()).font(semibold(SECTION_FONT_SIZE)).color(tokens.text));
        if let Some(icon) = menu {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                menu_response = Some(crate::widgets::icon_button(ui, icon, title, false, 18.0));
            });
        }
    });
    ui.add_space(SECTION_GAP_BELOW);
    menu_response
}

/// A row: `label` in the label column, `controls` filling the rest of the row's width.
pub(crate) fn row<R>(ui: &mut Ui, label: &str, controls: impl FnOnce(&mut Ui) -> R) -> R {
    let tokens = Tokens::get(ui.ctx());
    let inner = ui
        .horizontal(|ui| {
            let (label_rect, _) = ui.allocate_exact_size(vec2(LABEL_WIDTH, ROW_HEIGHT), Sense::hover());
            ui.painter().text(label_rect.left_center(), Align2::LEFT_CENTER, label, FontId::proportional(CONTROL_FONT_SIZE), tokens.text);
            ui.vertical(|ui| {
                ui.set_width(ui.available_width());
                controls(ui)
            })
            .inner
        })
        .inner;
    ui.add_space(ROW_GAP);
    inner
}

/// Draws an option's picture in `rect`.
pub(crate) type Picture<T> = fn(&Painter, Rect, T, &Tokens);

/// A dropdown whose closed value and every option carry a picture of what they do; returns the
/// option picked this frame.
pub(crate) fn picture_combo<T: Copy + PartialEq>(
    ui: &mut Ui,
    id: Id,
    current: T,
    options: &[T],
    name: fn(T) -> &'static str,
    picture: Picture<T>,
) -> Option<T> {
    let tokens = Tokens::get(ui.ctx());
    let (face, response) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_HEIGHT), Sense::click());
    let fill = if response.hovered() { tokens.row_hover } else { tokens.button };
    ui.painter().rect_filled(face, CONTROL_CORNER_RADIUS, fill);
    let picture_rect = Rect::from_center_size(pos2(face.left() + CONTROL_PADDING + PICTURE_SIZE / 2.0, face.center().y), vec2(PICTURE_SIZE, PICTURE_SIZE));
    picture(ui.painter(), picture_rect, current, &tokens);
    let text_x = picture_rect.right() + CONTROL_PADDING;
    ui.painter().text(pos2(text_x, face.center().y), Align2::LEFT_CENTER, crate::i18n::tr(name(current)), semibold(CONTROL_FONT_SIZE), tokens.text);
    ui.painter().text(pos2(face.right() - CONTROL_PADDING, face.center().y), Align2::RIGHT_CENTER, egui_phosphor::regular::CARET_DOWN, FontId::proportional(CARET_SIZE), tokens.text);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, true, crate::i18n::tr(name(current))));
    if response.clicked() {
        egui::Popup::toggle_id(ui.ctx(), id);
    }

    let mut picked = None;
    egui::Popup::from_response(&response)
        .id(id)
        .open_memory(None)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .align(egui::RectAlign::BOTTOM_START)
        .width(face.width())
        .show(|ui| {
            ui.set_width(face.width() - 2.0 * CONTROL_PADDING);
            for option in options {
                let (rect, option_response) = ui.allocate_exact_size(vec2(ui.available_width(), OPTION_HEIGHT), Sense::click());
                let chosen = *option == current;
                if option_response.hovered() {
                    ui.painter().rect_filled(rect, CONTROL_CORNER_RADIUS, tokens.row_hover);
                }
                if chosen {
                    let mark = Rect::from_min_size(pos2(rect.left(), rect.top() + 7.0), vec2(3.0, rect.height() - 14.0));
                    ui.painter().rect_filled(mark, 1, tokens.accent);
                }
                let picture_rect =
                    Rect::from_center_size(pos2(rect.left() + CONTROL_PADDING + 4.0 + OPTION_PICTURE_SIZE / 2.0, rect.center().y), vec2(OPTION_PICTURE_SIZE, OPTION_PICTURE_SIZE));
                picture(ui.painter(), picture_rect, *option, &tokens);
                let font = if chosen { semibold(CONTROL_FONT_SIZE) } else { FontId::proportional(CONTROL_FONT_SIZE) };
                ui.painter().text(pos2(picture_rect.right() + CONTROL_PADDING + 2.0, rect.center().y), Align2::LEFT_CENTER, crate::i18n::tr(name(*option)), font, tokens.text);
                option_response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, chosen, crate::i18n::tr(name(*option))));
                if option_response.clicked() {
                    picked = Some(*option);
                }
            }
        });
    if picked.is_some() {
        egui::Popup::close_id(ui.ctx(), id);
    }
    picked.filter(|option| *option != current)
}

/// A slim one-line slider: the track, then the value at its end in SemiBold. A `centered`
/// track fills from its middle (0 in the middle). Returns `Some(merge)` on every frame that
/// changes `value`, like [`crate::widgets::slider_edit`].
pub(crate) fn value_slider(ui: &mut Ui, value: &mut f32, range: std::ops::RangeInclusive<f32>, shown: &str, centered: bool) -> Option<bool> {
    let tokens = Tokens::get(ui.ctx());
    let (rect, mut response) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_HEIGHT), Sense::click_and_drag());
    let track = Rect::from_min_max(pos2(rect.left() + KNOB_RADIUS, rect.top()), pos2(rect.right() - VALUE_WIDTH, rect.bottom()));
    let (minimum, maximum) = (*range.start(), *range.end());
    let mut edit = None;
    if (response.dragged() || response.clicked())
        && let Some(pointer) = response.interact_pointer_pos()
        && pointer.x <= track.right() + KNOB_RADIUS
    {
        let fraction = ((pointer.x - track.left()) / track.width().max(1.0)).clamp(0.0, 1.0);
        let next = minimum + (maximum - minimum) * fraction;
        if (next - *value).abs() > f32::EPSILON {
            *value = next;
            response.mark_changed();
            edit = Some(ui.data_mut(|data| std::mem::replace(data.get_temp_mut_or_default::<bool>(response.id), true)));
        }
    }
    if response.drag_stopped() || response.clicked() {
        ui.data_mut(|data| data.remove_temp::<bool>(response.id));
    }
    let fraction = if maximum > minimum { ((*value - minimum) / (maximum - minimum)).clamp(0.0, 1.0) } else { 0.0 };
    let y = rect.center().y;
    let knob_x = track.left() + track.width() * fraction;
    let painter = ui.painter();
    painter.hline(track.x_range(), y, Stroke::new(TRACK_WIDTH, tokens.button_border));
    let filled = if centered { (track.center().x.min(knob_x), track.center().x.max(knob_x)) } else { (track.left(), knob_x) };
    painter.hline(filled.0..=filled.1, y, Stroke::new(TRACK_WIDTH, tokens.accent));
    let ring = if response.hovered() || response.dragged() { tokens.text_strong } else { tokens.text };
    painter.circle(pos2(knob_x, y), KNOB_RADIUS, tokens.panel, Stroke::new(KNOB_STROKE, ring));
    painter.text(pos2(rect.right(), y), Align2::RIGHT_CENTER, shown, semibold(CONTROL_FONT_SIZE), tokens.text);
    response.widget_info(|| egui::WidgetInfo::slider(true, f64::from(*value), shown));
    edit
}

/// A folded group: a row with a chevron and a SemiBold title; while closed it draws `pictures`
/// (one per setting inside) at its right. Returns whether it is open, after drawing `body`.
pub(crate) fn folded(ui: &mut Ui, id: Id, title: &str, pictures: &[&dyn Fn(&Painter, Rect, &Tokens)], body: impl FnOnce(&mut Ui)) -> bool {
    let tokens = Tokens::get(ui.ctx());
    let mut open = ui.data(|data| data.get_temp::<bool>(id)).unwrap_or(false);
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), FOLDED_HEIGHT), Sense::click());
    if response.clicked() {
        open = !open;
        ui.data_mut(|data| data.insert_temp(id, open));
    }
    let fill = if response.hovered() { tokens.row_hover } else { tokens.row };
    ui.painter().rect_filled(rect, FOLDED_CORNER_RADIUS, fill);
    let chevron = if open { egui_phosphor::regular::CARET_DOWN } else { egui_phosphor::regular::CARET_RIGHT };
    ui.painter().text(pos2(rect.left() + CONTROL_PADDING + 3.0, rect.center().y), Align2::CENTER_CENTER, chevron, FontId::proportional(CARET_SIZE), tokens.text_dim);
    ui.painter().text(pos2(rect.left() + CONTROL_PADDING + 14.0, rect.center().y), Align2::LEFT_CENTER, title, semibold(CONTROL_FONT_SIZE), tokens.text);
    if !open {
        let mut x = rect.right() - CONTROL_PADDING;
        for picture in pictures.iter().rev() {
            let picture_rect = Rect::from_min_size(pos2(x - FOLDED_PICTURE_SIZE, rect.center().y - FOLDED_PICTURE_SIZE / 2.0), vec2(FOLDED_PICTURE_SIZE, FOLDED_PICTURE_SIZE));
            picture(ui.painter(), picture_rect, &tokens);
            x -= FOLDED_PICTURE_SIZE + FOLDED_PICTURE_GAP;
        }
    }
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::CollapsingHeader, true, title));
    ui.add_space(ROW_GAP);
    if open {
        ui.indent(id.with("body"), body);
    }
    open
}

/// `color` with its alpha scaled by `opacity`
pub(crate) fn faded(color: Color32, opacity: f32) -> Color32 {
    color.gamma_multiply(opacity)
}
