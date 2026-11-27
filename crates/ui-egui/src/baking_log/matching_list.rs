//! The Baking Log's Matching by Name tab: which low poly part the latest bake paired with which high poly part

use egui::{Align2, CornerRadius, FontId, Rect, Role, Sense, Stroke, Ui, WidgetInfo, pos2, vec2};
use egui_phosphor::regular as ic;
use texturecraft_engine::bake::{BakeLogMatching, NamePair};

use super::{GLYPH_SIZE, HOVER_INSET, NAME_SIZE, ROW_HEIGHT};
use crate::i18n::{tr, trf};
use crate::property_sheet::SIDE_PADDING_POINTS;
use crate::theme::{Tokens, semibold};

/// Size of the column captions
const CAPTION_SIZE: f32 = 11.0;

/// The two columns' captions and a row per pair (only the parts with no partner, if `issues_only`), each ruled off below
pub(super) fn matching_list(ui: &mut Ui, matching: &BakeLogMatching, issues_only: bool) {
    let tokens = Tokens::get(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_HEIGHT), Sense::hover());
    let captions = [caption(tr("Low poly mesh ({suffix})"), &matching.low_suffix), caption(tr("High poly mesh ({suffix})"), &matching.high_suffix)];
    for (left, caption) in columns(rect).into_iter().zip(captions) {
        ui.painter().text(pos2(left, rect.center().y), Align2::LEFT_CENTER, caption, FontId::proportional(CAPTION_SIZE), tokens.text_dim);
    }
    rule(ui, rect);

    for pair in matching.pairs.iter().filter(|pair| !issues_only || pair.unpaired()) {
        pair_row(ui, pair);
    }
}

/// A column caption in capitals, with the set's `suffix` kept as typed
fn caption(template: &str, suffix: &str) -> String {
    template.split("{suffix}").map(str::to_uppercase).collect::<Vec<_>>().join(suffix)
}

/// The left edges of the low and high poly columns of `row`
fn columns(row: Rect) -> [f32; 2] {
    [row.left() + SIDE_PADDING_POINTS, row.center().x]
}

/// The hairline under `row`, inside the side padding
fn rule(ui: &Ui, row: Rect) {
    let tokens = Tokens::get(ui.ctx());
    let x = row.left() + SIDE_PADDING_POINTS..=row.right() - SIDE_PADDING_POINTS;
    ui.painter().hline(x, row.bottom() - 0.5, Stroke::new(1.0, tokens.separator));
}

/// A pair's row: the low poly part, then the high poly part; a side with no partner is a warning glyph that tells why on hover
fn pair_row(ui: &mut Ui, pair: &NamePair) {
    let tokens = Tokens::get(ui.ctx());
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_HEIGHT), Sense::hover());
    let no_match = tr("No match");
    let name = format!("{}, {}", pair.low.as_deref().unwrap_or(no_match), pair.high.as_deref().unwrap_or(no_match));
    let reason = match (&pair.low, &pair.high) {
        (Some(low), None) => Some(trf("No high poly part pairs with {part} by name, so it bakes from none.", &[("part", low)])),
        (None, Some(high)) => Some(trf("No low poly part pairs with {part} by name, so no texture set bakes from it.", &[("part", high)])),
        _ => None,
    };
    response.widget_info(|| WidgetInfo::labeled(Role::Label, true, &name));
    super::describe(ui, &response, reason.as_deref());

    let painter = ui.painter();
    if response.hovered() {
        painter.rect_filled(rect.shrink2(vec2(HOVER_INSET, 0.0)), CornerRadius::ZERO, tokens.row_hover);
    }
    let center_y = rect.center().y;
    for (left, part) in columns(rect).into_iter().zip([&pair.low, &pair.high]) {
        match part {
            Some(part) => painter.text(pos2(left, center_y), Align2::LEFT_CENTER, part, FontId::proportional(NAME_SIZE), tokens.text),
            None => painter.text(pos2(left, center_y), Align2::LEFT_CENTER, ic::WARNING, FontId::proportional(GLYPH_SIZE), tokens.warning),
        };
    }
    rule(ui, rect);

    if let Some(reason) = reason {
        response.on_hover_ui(|ui| {
            ui.label(egui::RichText::new(no_match).font(semibold(NAME_SIZE)).color(tokens.text));
            ui.label(egui::RichText::new(reason).size(NAME_SIZE).color(tokens.text_dim));
        });
    }
}
