//! The tool shelf's variant menus: a tool with variants shows its current variant's icon and a corner arrow, and a click on
//! the arrow opens a menu of the variants beside the shelf; picking one runs that variant's command

use egui::{Align2, Color32, CornerRadius, FontId, Rect, Response, Sense, Ui, vec2};
use serde_json::json;
use texturecraft_engine::Tool;
use texturecraft_engine::tool_variant::ToolVariant;

use crate::TextureCraftApp;
use crate::i18n::{tr, trf};
use crate::theme::Tokens;
use crate::tool_variant_icon;
use crate::widgets::painted_icon_button;

/// The corner arrow's size, and its centre's inset from the button's lower-right corner, as fractions of the button height
const ARROW_SCALE: f32 = 0.32;
const ARROW_INSET_SCALE: f32 = 0.16;

/// The button's lower-right square a press must start in to open the menu rather than select the tool, as a fraction of its height
const ARROW_CORNER_SCALE: f32 = 0.36;

/// The menu's distance from the button: past the shelf card's padding
const MENU_GAP: f32 = 6.0;

/// The menu's rows: their height (the icon's square), the inset around them, the space between them and their rounding
const ROW_HEIGHT: f32 = 24.0;
const ROW_INSET: i8 = 3;
const ROW_SPACING: f32 = 2.0;
const ROW_RADIUS: u8 = 4;

/// The gap between a row's icon and its label, and the space after the longest label
const ROW_LABEL_GAP: f32 = 4.0;
const ROW_END_PADDING: f32 = 12.0;

/// The accent bar at the current variant's left edge
const CURRENT_BAR_WIDTH: f32 = 3.0;

/// A tool's shelf button: its current variant's icon and the arrow; screen readers get the tool's `name` with the variant as its description
pub(crate) fn button(ui: &mut Ui, variant: ToolVariant, name: &str, tooltip: &str, selected: bool, size: f32) -> Response {
    let tokens = Tokens::get(ui.ctx());
    let variant_name = tr(variant.label());
    let response = painted_icon_button(ui, name, selected, size, |painter, rect, color, fill| {
        // At rest the button is clear over the shelf card; the arrow stays quieter than the icon until the button lights up
        let (ground, arrow_color) = if fill == Color32::TRANSPARENT { (tokens.panel, tokens.text_dim) } else { (fill, color) };
        tool_variant_icon::paint(painter, rect, variant, color, ground);
        let arrow_centre = rect.right_bottom() - vec2(size, size) * ARROW_INSET_SCALE;
        painter.text(arrow_centre, Align2::CENTER_CENTER, egui_phosphor::regular::CARET_DOWN, FontId::proportional(size * ARROW_SCALE), arrow_color);
    });
    ui.ctx().accesskit_node_builder(response.id, |node| node.set_description(variant_name));
    response.on_hover_text(tooltip)
}

/// Opens, shows and closes `tool`'s variant menu beside its shelf `button`. True when this frame's click on the button was
/// the arrow's, so it does not also select the tool
pub(crate) fn menu(app: &mut TextureCraftApp, button: &Response, tool: Tool) -> bool {
    let ctx = button.ctx.clone();
    let popup_id = button.id.with("variants");

    // A click's press is forgotten by its release, so where it started is kept from the press
    let pressed_on_arrow_id = button.id.with("pressed_on_arrow");
    if button.hovered() && ctx.input(|input| input.pointer.primary_pressed()) {
        let corner = arrow_corner(button.rect);
        let on_arrow = ctx.input(|input| input.pointer.interact_pos()).is_some_and(|position| corner.contains(position));
        ctx.data_mut(|data| data.insert_temp(pressed_on_arrow_id, on_arrow));
    }
    let arrow_click = button.clicked() && ctx.data(|data| data.get_temp::<bool>(pressed_on_arrow_id)).unwrap_or_default();
    if !ctx.input(|input| input.pointer.primary_down()) {
        ctx.data_mut(|data| data.remove::<bool>(pressed_on_arrow_id));
    }

    // A second click on the arrow closes the menu: it started outside the open menu
    if arrow_click {
        egui::Popup::open_id(&ctx, popup_id);
    }
    let frame = egui::Frame::menu(&ctx.global_style()).inner_margin(ROW_INSET);
    let name = trf("{tool} variants", &[("tool", &tr(tool.label()))]);
    egui::Popup::from_response(button)
        .id(popup_id)
        .kind(egui::PopupKind::Menu)
        .open_memory(None)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClick)
        .align(egui::RectAlign::RIGHT_START)
        .gap(MENU_GAP)
        .frame(frame)
        .accessible_name(name)
        .show(|ui| rows(app, ui, tool));
    arrow_click
}

/// The square at the button's lower right that holds the arrow
fn arrow_corner(button: Rect) -> Rect {
    let side = button.height() * ARROW_CORNER_SCALE;
    Rect::from_min_max(button.right_bottom() - vec2(side, side), button.right_bottom())
}

/// One row per variant of `tool`, the current one lit with the accent bar; a click runs the row's command
fn rows(app: &mut TextureCraftApp, ui: &mut Ui, tool: Tool) {
    let tokens = Tokens::get(ui.ctx());
    let font = egui::TextStyle::Body.resolve(ui.style());
    let current = ToolVariant::current(&app.session.tool, tool);
    let variants = ToolVariant::of(tool);
    let label_width =
        variants.iter().map(|variant| ui.painter().layout_no_wrap(tr(variant.label()).to_owned(), font.clone(), tokens.text).size().x).fold(0.0, f32::max);
    let width = ROW_HEIGHT + ROW_LABEL_GAP + label_width + ROW_END_PADDING;
    ui.spacing_mut().item_spacing = vec2(0.0, ROW_SPACING);

    for &variant in variants {
        let label = tr(variant.label());
        let is_current = current == Some(variant);
        let (rect, response) = ui.allocate_exact_size(vec2(width, ROW_HEIGHT), Sense::click());
        response.widget_info(|| egui::WidgetInfo::selected(egui::Role::RadioButton, ui.is_enabled(), is_current, label));

        // The current variant is lit like the active tool and marked with the accent bar, so colour is not its only sign
        let lit = response.hovered() || response.has_focus();
        let fill = if is_current {
            tokens.tool_active
        } else if lit {
            tokens.row_hover
        } else {
            tokens.panel
        };
        let text_color = if is_current || lit { tokens.text_strong } else { tokens.text };
        let painter = ui.painter();
        painter.rect_filled(rect, ROW_RADIUS, fill);
        if is_current {
            let bar = Rect::from_min_size(rect.min, vec2(CURRENT_BAR_WIDTH, ROW_HEIGHT));
            painter.rect_filled(bar, CornerRadius { nw: ROW_RADIUS, sw: ROW_RADIUS, ne: 0, se: 0 }, tokens.accent);
        }
        let icon_rect = Rect::from_min_size(rect.min, vec2(ROW_HEIGHT, ROW_HEIGHT));
        tool_variant_icon::paint(painter, icon_rect, variant, text_color, fill);
        painter.text(icon_rect.right_center() + vec2(ROW_LABEL_GAP, 0.0), Align2::LEFT_CENTER, label, font.clone(), text_color);

        if response.clicked() {
            app.run(variant.command_id(), json!({}));
        }
    }
}
