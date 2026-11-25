//! The Baking Log panel: the latest bake under three tabs, read from the session's bake record

use egui::{Align2, CornerRadius, FontId, Id, Rect, Role, Sense, Ui, WidgetInfo, pos2, vec2};
use egui_phosphor::regular as ic;
use texturecraft_engine::bake::{BakeLogMap, BakeMapState};

use crate::TextureCraftApp;
use crate::i18n::{baked_map_label, tr};
use crate::property_sheet::{SIDE_PADDING_POINTS, TAB_GAP, TAB_ICON_CENTER, TAB_ICON_SIZE, TAB_NAME_LEFT, TAB_PADDING, TAB_TEXT_SIZE, TAB_TOP, shown_tab_box};
use crate::theme::{Tokens, semibold};

mod baking_log_tab;

pub use baking_log_tab::BakingLogTab;

/// Height of the tab strip, as the Properties section tabs
const STRIP_HEIGHT: f32 = 30.0;

/// How far the first tab reaches left of the rows' side padding, as the Properties section tabs
const FIRST_TAB_OUTDENT: f32 = 4.0;

/// Height of one row of a list
const ROW_HEIGHT: f32 = 24.0;

/// Air above the first row
const LIST_TOP: f32 = 4.0;

/// How far a hovered row's fill stays inside the panel's sides
const HOVER_INSET: f32 = 3.0;

/// From the panel's left edge to a row's glyph centre, and to its name
const GLYPH_CENTER: f32 = SIDE_PADDING_POINTS + 6.0;
const NAME_LEFT: f32 = SIDE_PADDING_POINTS + 22.0;

/// Size of a row's glyph and of its name
const GLYPH_SIZE: f32 = 13.0;
const NAME_SIZE: f32 = 12.0;

pub(crate) fn body(app: &mut TextureCraftApp, ui: &mut Ui) {
    let running = app.session.bake_log.is_running();
    if let Some(tab) = tab_strip(ui, app.ui.baking_log_tab, running) {
        app.ui.baking_log_tab = tab;
    }

    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.add_space(LIST_TOP);
        if app.ui.baking_log_tab == BakingLogTab::BakingProcess {
            baking_process(ui, &app.session.bake_log.maps);
        }
    });
}

/// The tabs on the panel's top edge, as the Properties section tabs; a tab with work in progress shows dots for its icon
fn tab_strip(ui: &mut Ui, shown: BakingLogTab, running: bool) -> Option<BakingLogTab> {
    let tokens = Tokens::get(ui.ctx());
    let (strip, _) = ui.allocate_exact_size(vec2(ui.available_width(), STRIP_HEIGHT), Sense::hover());
    ui.painter().rect_filled(strip, CornerRadius::ZERO, tokens.chrome);

    let mut picked = None;
    let mut left = strip.left() + SIDE_PADDING_POINTS - FIRST_TAB_OUTDENT;
    for tab in BakingLogTab::ALL {
        let name = tr(tab.name());
        let on = tab == shown;
        let width = ui.painter().layout_no_wrap(name.to_owned(), semibold(TAB_TEXT_SIZE), tokens.text).size().x + TAB_PADDING;
        let rect = Rect::from_min_max(pos2(left, strip.top() + TAB_TOP), pos2(left + width, strip.bottom()));
        let response = ui.interact(rect, ui.scope_id().with(("baking_log_tab", tab)), Sense::click());
        response.widget_info(|| WidgetInfo::selected(Role::Tab, true, on, name));
        if response.clicked() {
            picked = Some(tab);
        }

        let painter = ui.painter();
        if on {
            shown_tab_box(painter, &tokens, rect);
        }
        let color = if on || response.hovered() { tokens.text } else { tokens.text_dim };
        let in_progress = running && tab == BakingLogTab::BakingProcess;
        let (icon, icon_color) = if in_progress { (ic::DOTS_THREE, tokens.accent) } else { (tab.icon(), color) };
        let center_y = strip.center().y + TAB_TOP / 2.0;
        painter.text(pos2(left + TAB_ICON_CENTER, center_y), Align2::CENTER_CENTER, icon, FontId::proportional(TAB_ICON_SIZE), icon_color);
        let font = if on { semibold(TAB_TEXT_SIZE) } else { FontId::proportional(TAB_TEXT_SIZE) };
        painter.text(pos2(left + TAB_NAME_LEFT, center_y), Align2::LEFT_CENTER, name, font, color);
        if response.has_focus() {
            crate::property_sheet::paint_focus_outline(painter, response.rect, tokens.text_strong);
        }

        left += width + TAB_GAP;
    }
    picked
}

/// One row per texture set and map, in bake order; the map being baked stays in view
fn baking_process(ui: &mut Ui, maps: &[BakeLogMap]) {
    let followed = maps.iter().position(|row| row.state == BakeMapState::Baking);
    let follow_id = Id::unique("baking_log_followed_map");
    let newly_followed = followed.is_some() && ui.data(|data| data.get_temp::<Option<usize>>(follow_id)) != Some(followed);
    ui.data_mut(|data| data.insert_temp(follow_id, followed));

    for (index, row) in maps.iter().enumerate() {
        let rect = map_row(ui, row);
        if newly_followed && followed == Some(index) {
            ui.scroll_to_rect(rect, None);
        }
    }
}

/// A map's row: its state's glyph, then "<texture set> <map>"; a map that was not baked tells why on hover
fn map_row(ui: &mut Ui, row: &BakeLogMap) -> Rect {
    let tokens = Tokens::get(ui.ctx());
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_HEIGHT), Sense::hover());
    let name = format!("{} {}", row.set_name, tr(baked_map_label(row.map)));
    let (glyph, glyph_color, state) = match &row.state {
        BakeMapState::Waiting => (ic::CIRCLE_DASHED, tokens.text_dim, tr("Waiting")),
        BakeMapState::Baking => (ic::DOTS_THREE, tokens.accent, tr("Baking")),
        BakeMapState::Baked => (ic::CHECK, tokens.text, tr("Baked")),
        BakeMapState::Failed(_) => (ic::X_CIRCLE, tokens.danger, tr("Not baked")),
        BakeMapState::Canceled => (ic::WARNING, tokens.warning, tr("Canceled")),
    };
    response.widget_info(|| WidgetInfo::labeled(Role::Label, true, format!("{name}, {state}")));

    let painter = ui.painter();
    if response.hovered() {
        painter.rect_filled(rect.shrink2(vec2(HOVER_INSET, 0.0)), CornerRadius::ZERO, tokens.row_hover);
    }
    let center_y = rect.center().y;
    painter.text(pos2(rect.left() + GLYPH_CENTER, center_y), Align2::CENTER_CENTER, glyph, FontId::proportional(GLYPH_SIZE), glyph_color);
    let (font, color) = match row.state {
        BakeMapState::Baking => (semibold(NAME_SIZE), tokens.text),
        BakeMapState::Waiting => (FontId::proportional(NAME_SIZE), tokens.text_dim),
        _ => (FontId::proportional(NAME_SIZE), tokens.text),
    };
    painter.text(pos2(rect.left() + NAME_LEFT, center_y), Align2::LEFT_CENTER, name, font, color);

    let reason = match &row.state {
        BakeMapState::Failed(reason) => Some(reason.as_str()),
        BakeMapState::Canceled => Some(tr("The bake was canceled before this map was kept.")),
        _ => None,
    };
    if let Some(reason) = reason {
        response.on_hover_ui(|ui| {
            ui.label(egui::RichText::new(state).font(semibold(NAME_SIZE)).color(tokens.text));
            ui.label(egui::RichText::new(reason).size(NAME_SIZE).color(tokens.text_dim));
        });
    }
    rect
}
