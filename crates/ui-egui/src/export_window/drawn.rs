//! The Export textures window's own controls, drawn at the places its layout gives them: tabs,
//! checks, dropdowns, buttons and icon buttons in the window's colours ([`ExportLook`])

use egui::{Align2, Color32, CornerRadius, FontId, Id, Pos2, Rect, Response, Role, Sense, Stroke, StrokeKind, Ui, WidgetInfo, pos2, vec2};
use egui_phosphor::regular as ic;

use super::export_look::{CONTROL_HEIGHT, CONTROL_RADIUS, ExportLook, TEXT_SIZE};
use crate::theme::semibold;

/// Side and corner radius of a check box
const CHECK_SIZE: f32 = 13.0;
const CHECK_RADIUS: u8 = 2;

/// The tick inside a check box: its three points from the box's left edge and centre, and its width
const TICK_POINTS: [(f32, f32); 3] = [(3.2, 0.2), (5.6, 2.6), (9.8, -2.6)];
const TICK_WIDTH: f32 = 1.7;

/// Air between a check box and its label
pub(crate) const CHECK_LABEL_GAP: f32 = 5.0;

/// From a dropdown's left edge to its value, and from its right edge to its chevron's centre
pub(crate) const VALUE_INSET: f32 = 7.0;
pub(crate) const CHEVRON_INSET: f32 = 11.0;

/// Size of a chevron and of the window's icons
pub(crate) const CHEVRON_SIZE: f32 = 12.0;

/// The air on either side of a tab's name
const TAB_PADDING: f32 = 14.0;
/// A tab's corner radius
const TAB_RADIUS: u8 = 4;
/// Gap between tabs
const TAB_GAP: f32 = 3.0;
/// How far a tab reaches under the divider, where the body's colour covers its lower corners
const TAB_OVERLAP: f32 = 4.0;

/// A quiet dropdown's hover face: its padding round the value and chevron, and half its height
const TEXT_DROPDOWN_PADDING: f32 = 3.0;
const TEXT_DROPDOWN_HALF_HEIGHT: f32 = 9.0;

/// Hit area of an icon button around its glyph
const ICON_HIT_SIZE: f32 = 20.0;

/// Width of the keyboard focus outline on the window's drawn controls
const FOCUS_OUTLINE_WIDTH: f32 = 2.0;

/// Whether a control that cannot be used looks greyed; a built-in template's cards keep their look
pub(crate) fn looks_disabled(ui: &Ui, enabled: bool) -> bool {
    !enabled && ui.visuals().disabled_alpha() < 1.0
}

/// The window's regular and semibold text
pub(crate) fn font(bold: bool) -> FontId {
    if bold { semibold(TEXT_SIZE) } else { FontId::proportional(TEXT_SIZE) }
}

/// Width of `text` in the window's font
pub(crate) fn text_width(ui: &Ui, text: &str, bold: bool) -> f32 {
    ui.painter().layout_no_wrap(text.to_owned(), font(bold), Color32::WHITE).size().x
}

/// `text` with its left edge at `left` and its middle at `y`
pub(crate) fn text_at(ui: &Ui, left: f32, y: f32, text: &str, color: Color32, bold: bool) -> Rect {
    ui.painter().text(pos2(left, y), Align2::LEFT_CENTER, text, font(bold), color)
}

/// `text` cut with "…" at its end (or its start, for file names) to fit `room`
pub(crate) fn fitted(ui: &Ui, text: &str, room: f32, from_start: bool, bold: bool) -> String {
    if text_width(ui, text, bold) <= room {
        return text.to_owned();
    }

    let mut cuts: Vec<_> = text.char_indices().skip(1).map(|(offset, _)| offset).collect();
    if !from_start {
        cuts.reverse();
    }

    let truncate = |cut| if from_start { format!("…{}", &text[cut..]) } else { format!("{}…", &text[..cut]) };
    let first_fit = cuts.partition_point(|&cut| text_width(ui, &truncate(cut), bold) > room);
    cuts.get(first_fit).map_or_else(|| "…".to_owned(), |&cut| truncate(cut))
}

/// A glyph centred on `centre`
pub(crate) fn glyph(ui: &Ui, centre: Pos2, glyph: &str, size: f32, color: Color32) {
    ui.painter().text(centre, Align2::CENTER_CENTER, glyph, FontId::proportional(size), color);
}

/// The keyboard focus outline of a drawn control, in the text colour
pub(crate) fn paint_focus(ui: &Ui, response: &Response) {
    if response.has_focus() {
        let stroke = Stroke::new(FOCUS_OUTLINE_WIDTH, ExportLook::get(ui.ctx()).text);
        ui.painter().rect_stroke(response.rect, CONTROL_RADIUS, stroke, StrokeKind::Inside);
    }
}

/// A horizontal or vertical divider line, one point thick
pub(crate) fn divider(ui: &Ui, rect: Rect) {
    ui.painter().rect_filled(rect, 0.0, ExportLook::get(ui.ctx()).divider);
}

/// Sentence-case rounded tabs along `strip`'s bottom from `left`, the open one joined to the body; returns the clicked one
pub(crate) fn tabs(ui: &mut Ui, strip: Rect, left: f32, labels: &[&str], open: usize) -> Option<usize> {
    let look = ExportLook::get(ui.ctx());
    let bottom = strip.bottom();
    divider(ui, Rect::from_min_max(pos2(strip.left(), bottom - 1.0), pos2(strip.right(), bottom)));
    let mut clicked = None;
    let mut x = left;
    for (index, label) in labels.iter().enumerate() {
        let selected = index == open;
        let width = text_width(ui, label, selected) + 2.0 * TAB_PADDING;
        let rect = Rect::from_min_max(pos2(x, strip.top()), pos2(x + width, bottom));
        let response = ui.interact(rect, ui.scope_id().with(("export_tab", index)), Sense::click());
        response.widget_info(|| WidgetInfo::selected(Role::Tab, true, selected, *label));

        // The tab's lower corners reach under the divider and are covered by the body's colour
        let painter = ui.painter();
        let face = Rect::from_min_max(rect.min, pos2(rect.right(), bottom + TAB_OVERLAP));
        let below = Rect::from_min_max(pos2(rect.left(), bottom - 1.0), pos2(rect.right(), bottom + TAB_OVERLAP + 1.0));
        if selected {
            painter.rect(face, TAB_RADIUS, look.window, Stroke::new(1.0, look.control_border), StrokeKind::Inside);
            painter.rect_filled(below.shrink2(vec2(1.0, 0.0)), 0.0, look.window);
        } else {
            let fill = if response.hovered() { look.raised } else { look.chrome };
            painter.rect_filled(face.with_min_y(face.top() + 1.0), TAB_RADIUS, fill);
            painter.rect_filled(below, 0.0, look.window);
            painter.rect_filled(below.with_max_y(bottom), 0.0, look.divider);
        }
        let color = if selected || response.hovered() { look.text } else { look.text_dim };
        painter.text(rect.center() + vec2(0.0, 1.0), Align2::CENTER_CENTER, *label, font(selected), color);
        paint_focus(ui, &response);
        if response.clicked() {
            clicked = Some(index);
        }
        x += width + TAB_GAP;
    }
    clicked
}

/// A check box from `left` with its middle at `y`: dim and ticked when on, dark with a light edge when off
pub(crate) fn paint_check(ui: &Ui, left: f32, y: f32, on: bool) {
    let look = ExportLook::get(ui.ctx());
    let rect = Rect::from_min_size(pos2(left, y - CHECK_SIZE / 2.0), vec2(CHECK_SIZE, CHECK_SIZE));
    let painter = ui.painter();
    if on {
        painter.rect_filled(rect, CHECK_RADIUS, look.text_dim);
        let points = TICK_POINTS.iter().map(|(dx, dy)| pos2(left + dx, y + dy)).collect();
        painter.add(egui::Shape::line(points, Stroke::new(TICK_WIDTH, look.window)));
    } else {
        painter.rect(rect, CHECK_RADIUS, look.control, Stroke::new(1.0, look.control_border), StrokeKind::Inside);
    }
}

/// A check box at `at` (its left edge's middle) with `label` after it, toggled by a click in `hit`
pub(crate) fn check(ui: &mut Ui, id: Id, hit: Rect, at: Pos2, on: &mut bool, label: Option<&str>, name: &str) -> Response {
    let look = ExportLook::get(ui.ctx());
    let mut response = ui.interact(hit, id, Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    let checked = *on;
    response.widget_info(|| WidgetInfo::selected(Role::CheckBox, true, checked, name));
    paint_check(ui, at.x, at.y, checked);
    if let Some(label) = label {
        text_at(ui, at.x + CHECK_SIZE + CHECK_LABEL_GAP, at.y, label, look.text, false);
    }
    paint_focus(ui, &response);
    response
}

/// The rect of a control `left`..`right` with its middle at `y`
pub(crate) fn control_rect(left: f32, right: f32, y: f32) -> Rect {
    Rect::from_min_max(pos2(left, y - CONTROL_HEIGHT / 2.0), pos2(right, y + CONTROL_HEIGHT / 2.0))
}

/// A dropdown's chevron, pointing up while its list is open, drawn just inside the right edge of the button `rect`
pub(crate) fn chevron_icon(ui: &Ui, rect: Rect, visuals: &egui::style::WidgetVisuals, open: bool) {
    let centre = pos2(rect.right() + VALUE_INSET - CHEVRON_INSET, rect.center().y);
    glyph(ui, centre, if open { ic::CARET_UP } else { ic::CARET_DOWN }, CHEVRON_SIZE, visuals.fg_stroke.color);
}

/// A dropdown filling `rect`, showing `shown`, its list as wide as it and flush under it
pub(crate) fn combo<R>(
    ui: &mut Ui,
    rect: Rect,
    id: impl std::hash::Hash + std::fmt::Debug,
    shown: impl Into<String>,
    list: impl FnOnce(&mut Ui) -> R,
) -> egui::InnerResponse<Option<R>> {
    let look = ExportLook::get(ui.ctx());
    let shown: String = shown.into();
    ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
        egui::ComboBox::from_id_salt(id)
            .width(rect.width())
            .height(f32::INFINITY)
            .selected_text(shown)
            .wrap_mode(egui::TextWrapMode::Truncate)
            .icon(chevron_icon)
            .popup_style(look.list_style())
            .show_ui(ui, list)
    })
    .inner
}

/// A button drawn as a dropdown showing `shown`, for a menu that is no option list; lighter when it cannot be used
pub(crate) fn combo_button(ui: &mut Ui, rect: Rect, id: Id, shown: &str, enabled: bool) -> Response {
    let look = ExportLook::get(ui.ctx());
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let response = ui.interact(rect, id, sense);
    response.widget_info(|| WidgetInfo::labeled(Role::Button, enabled, shown));
    let (fill, color) = match (enabled, response.hovered()) {
        (false, _) => (look.raised, look.text_dim),
        (true, true) => (look.raised, look.text),
        (true, false) => (look.control, look.text),
    };
    let painter = ui.painter();
    painter.rect(rect, CONTROL_RADIUS, fill, Stroke::new(1.0, look.control_border), StrokeKind::Inside);
    painter.text(rect.left_center() + vec2(VALUE_INSET, 0.0), Align2::LEFT_CENTER, shown, font(false), color);
    glyph(ui, rect.right_center() - vec2(CHEVRON_INSET, 0.0), ic::CARET_DOWN, CHEVRON_SIZE, color);
    paint_focus(ui, &response);
    response
}

/// A field showing `text` cut with "…" at its end that acts as a button, as the output directory does
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn field_button(ui: &mut Ui, rect: Rect, id: Id, text: &str) -> Response {
    let look = ExportLook::get(ui.ctx());
    let response = ui.interact(rect, id, Sense::click());
    response.widget_info(|| WidgetInfo::labeled(Role::Button, true, text));
    let border = if response.hovered() { look.outline } else { look.control_border };
    ui.painter().rect(rect, CONTROL_RADIUS, look.control, Stroke::new(1.0, border), StrokeKind::Inside);
    let shown = fitted(ui, text, rect.width() - 2.0 * VALUE_INSET, false, false);
    text_at(ui, rect.left() + VALUE_INSET, rect.center().y, &shown, look.text, false);
    paint_focus(ui, &response);
    response
}

/// A dropdown that cannot be used: a step lighter than the others, its value and chevron dim; `id` tells it from the rows' other ones
pub(crate) fn disabled_combo(ui: &mut Ui, rect: Rect, id: impl std::hash::Hash + std::fmt::Debug, shown: &str, name: &str) -> Response {
    let look = ExportLook::get(ui.ctx());
    let response = ui.interact(rect, ui.scope_id().with(("export_disabled_combo", id)), Sense::hover());
    response.widget_info(|| {
        let mut info = WidgetInfo::labeled(Role::ComboBox, false, name);
        info.current_text_value = Some(shown.to_owned());
        info
    });
    let painter = ui.painter();
    painter.rect(rect, CONTROL_RADIUS, look.raised, Stroke::new(1.0, look.control_border), StrokeKind::Inside);
    painter.text(rect.left_center() + vec2(VALUE_INSET, 0.0), Align2::LEFT_CENTER, shown, font(false), look.text_dim);
    glyph(ui, rect.right_center() - vec2(CHEVRON_INSET, 0.0), ic::CARET_DOWN, CHEVRON_SIZE, look.text_dim);
    response
}

/// Text that reads as a label to assistive technology and tests: `text` from `left`, its middle at `y`; its response takes a tooltip
pub(crate) fn label_at(ui: &mut Ui, left: f32, y: f32, text: &str, color: Color32, bold: bool) -> Response {
    let rect = text_at(ui, left, y, text, color, bold);
    let response = ui.interact(rect, ui.scope_id().with(("export_label", text, left.to_bits(), y.to_bits())), Sense::hover());
    response.widget_info(|| WidgetInfo::labeled(Role::Label, true, text));
    response
}

/// A button filling `rect`: a step darker than the window with a light edge, or the accent for the one primary action
pub(crate) fn button(ui: &mut Ui, rect: Rect, label: &str, primary: bool, enabled: bool) -> Response {
    let id = ui.scope_id().with(("export_button", label));
    button_with_id(ui, id, rect, label, label, primary, enabled)
}

/// [`button`] for one of several buttons with the same label (each output map card's "$")
pub(crate) fn button_with_id(ui: &mut Ui, id: Id, rect: Rect, label: &str, name: &str, primary: bool, enabled: bool) -> Response {
    let look = ExportLook::get(ui.ctx());
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let response = ui.interact(rect, id, sense);
    response.widget_info(|| WidgetInfo::labeled(Role::Button, enabled, name));
    let (fill, border) = match (primary, enabled && response.hovered()) {
        (true, _) => (look.accent, look.accent),
        (false, true) => (look.raised, look.outline),
        (false, false) => (look.control, look.control_border),
    };
    let painter = ui.painter();
    let fade = if enabled { 1.0 } else { ui.visuals().disabled_alpha() };
    painter.rect(rect, CONTROL_RADIUS, fill.gamma_multiply(fade), Stroke::new(1.0, border.gamma_multiply(fade)), StrokeKind::Inside);
    let color = if looks_disabled(ui, enabled) {
        look.text_dim
    } else if primary {
        look.lettering_on(fill)
    } else {
        look.text
    };
    painter.text(rect.center(), Align2::CENTER_CENTER, label, font(primary), color);
    paint_focus(ui, &response);
    response
}

/// A glyph that acts as a button, centred on `centre`; `lit` frames it as switched on
pub(crate) fn icon_button(ui: &mut Ui, centre: Pos2, icon: &str, size: f32, name: &str, enabled: bool, lit: bool) -> Response {
    let look = ExportLook::get(ui.ctx());
    let rect = Rect::from_center_size(centre, vec2(ICON_HIT_SIZE, ICON_HIT_SIZE));
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let response = ui.interact(rect, ui.scope_id().with(("export_icon", name)), sense);
    response.widget_info(|| WidgetInfo::selected(Role::Button, enabled, lit, name));
    if lit {
        ui.painter().rect(rect, CONTROL_RADIUS, look.raised, Stroke::new(1.0, look.control_border), StrokeKind::Inside);
    }
    let color = match (enabled, lit || response.hovered()) {
        (false, _) if looks_disabled(ui, enabled) => look.control_border,
        (false, _) => look.text_dim,
        (true, true) => look.text,
        (true, false) => look.text_dim,
    };
    glyph(ui, centre, icon, size, color);
    paint_focus(ui, &response);
    response
}

/// A card's quiet dropdown: its value from `value_left` and a chevron at `chevron_x`, no box
pub(crate) fn text_dropdown(ui: &mut Ui, id: Id, value_left: f32, chevron_x: f32, y: f32, value: &str) -> Response {
    let look = ExportLook::get(ui.ctx());
    let enabled = ui.is_enabled();
    let rect = Rect::from_min_max(
        pos2(value_left - TEXT_DROPDOWN_PADDING, y - TEXT_DROPDOWN_HALF_HEIGHT),
        pos2(chevron_x + CHEVRON_SIZE / 2.0 + TEXT_DROPDOWN_PADDING, y + TEXT_DROPDOWN_HALF_HEIGHT),
    );
    let response = ui.interact(rect, id, if enabled { Sense::click() } else { Sense::hover() });
    let name = format!("{value} {}", ic::CARET_DOWN);
    response.widget_info(|| WidgetInfo::labeled(Role::Button, enabled, &name));
    if response.hovered() && enabled {
        ui.painter().rect_filled(rect, CornerRadius::same(CONTROL_RADIUS), look.raised);
    }
    let color = if looks_disabled(ui, enabled) { look.text_dim } else { look.text };
    text_at(ui, value_left, y, value, color, false);
    glyph(ui, pos2(chevron_x, y), ic::CARET_DOWN, CHEVRON_SIZE, color);
    paint_focus(ui, &response);
    response
}
