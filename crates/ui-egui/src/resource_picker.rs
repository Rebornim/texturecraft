//! A popup under a slot that picks a resource, such as a filter, from grouped shelf tiles searched by name

use egui::{Align2, Color32, FontId, Id, Response, Stroke, Ui};
use egui_phosphor::regular as ic;

use crate::i18n::tr;
use crate::picker_look::PickerLook;
use crate::resource_picker_item::ResourcePickerItem;
use crate::shelf;
use crate::shelf_preview::ShelfPreview;
use crate::theme::Tokens;
use crate::thumbs::Thumbs;

/// Four shelf tiles and the scrollbar
const PICKER_WIDTH: f32 = 320.0;
/// The grid's height, fixed so the popup keeps its place while searching
const PICKER_HEIGHT: f32 = 340.0;
const CURRENT_OUTLINE_WIDTH: f32 = 1.5;
const TILE_ROUNDING: f32 = 3.0;
/// The randomness badge: a see-through dark disc at the thumbnail's bottom-right corner holding a die, in points
const BADGE_RADIUS: f32 = 8.5;
const BADGE_ICON_SIZE: f32 = 12.0;
const BADGE_DISC: Color32 = Color32::from_black_alpha(140);
const BADGE_ICON: Color32 = Color32::from_gray(228);

/// Opens the picker `popup_id` with an empty, focused search field, or closes it
pub(crate) fn toggle(ctx: &egui::Context, popup_id: Id) {
    if egui::Popup::is_id_open(ctx, popup_id) {
        egui::Popup::close_id(ctx, popup_id);
        return;
    }

    egui::Popup::open_id(ctx, popup_id);
    ctx.data_mut(|data| {
        data.insert_temp(search_id(popup_id), String::new());
        data.insert_temp(focus_id(popup_id), true);
    });
}

/// Draws picker `popup_id` at `slot` while open and returns the id clicked (`items` runs only then)
pub(crate) fn show(
    slot: &Response,
    popup_id: Id,
    thumbs: &mut Thumbs,
    current: Option<&str>,
    items: impl FnOnce(&mut Thumbs) -> Vec<ResourcePickerItem>,
    preview: impl Fn(&'static str) -> ShelfPreview,
) -> Option<&'static str> {
    show_with_look(slot, popup_id, thumbs, (current, items), preview, &PickerLook::SHELF)
}

/// [`show`] with tiles drawn as `look` says
pub(crate) fn show_with_look(
    slot: &Response,
    popup_id: Id,
    thumbs: &mut Thumbs,
    (current, items): (Option<&str>, impl FnOnce(&mut Thumbs) -> Vec<ResourcePickerItem>),
    preview: impl Fn(&'static str) -> ShelfPreview,
    look: &PickerLook,
) -> Option<&'static str> {
    let ctx = slot.ctx.clone();
    if !egui::Popup::is_id_open(&ctx, popup_id) {
        return None;
    }

    let items = items(thumbs);
    let mut picked = None;
    egui::Popup::from_response(slot)
        .id(popup_id)
        .open_memory(None)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .align(egui::RectAlign::BOTTOM_END)
        .width(PICKER_WIDTH)
        .show(|ui| {
            ui.set_width(PICKER_WIDTH);
            let just_opened = ui.data_mut(|data| data.remove_temp::<bool>(focus_id(popup_id))).unwrap_or(false);
            let query = search_field(ui, popup_id, just_opened);
            egui::ScrollArea::vertical().max_height(PICKER_HEIGHT).show(ui, |ui| {
                // A search that leaves few tiles must not shrink the popup, or egui moves it
                ui.set_min_height(PICKER_HEIGHT);
                picked = grid(ui, thumbs, (&items, &query), (current, just_opened), &preview, look);
            });
        });

    if picked.is_some() {
        egui::Popup::close_id(&ctx, popup_id);
    }
    picked
}

/// The search field, focused the frame the picker opens; returns the lowercase query
fn search_field(ui: &mut Ui, popup_id: Id, just_opened: bool) -> String {
    let mut search: String = ui.data(|data| data.get_temp(search_id(popup_id))).unwrap_or_default();
    let field = ui.add(egui::TextEdit::singleline(&mut search).hint_text(tr("Search")).desired_width(f32::INFINITY));
    if just_opened {
        field.request_focus();
    }

    let query = search.trim().to_lowercase();
    ui.data_mut(|data| data.insert_temp(search_id(popup_id), search));
    query
}

/// The items matching `query` as tiles under group headers, `current` outlined; returns the one clicked
fn grid(
    ui: &mut Ui,
    thumbs: &mut Thumbs,
    (items, query): (&[ResourcePickerItem], &str),
    (current, reveal_current): (Option<&str>, bool),
    preview: &impl Fn(&'static str) -> ShelfPreview,
    look: &PickerLook,
) -> Option<&'static str> {
    let tk = Tokens::get(ui.ctx());
    let shown: Vec<&ResourcePickerItem> = items.iter().filter(|item| shelf::matches(query, &[item.label, item.id])).collect();
    let mut picked = None;
    for group in shelf::categories(&shown, |item| item.group) {
        if look.headers {
            shelf::header(ui, group);
        }
        shelf::tiles(ui, |ui| {
            for item in shown.iter().filter(|item| item.group == group) {
                let response = shelf::tile_with_look(ui, thumbs, (item.thumbnail.as_ref(), || preview(item.id)), item.label, None, look.tile);
                if look.random.contains(&item.id) {
                    randomness_badge(ui.painter(), look.tile.thumbnail_rect(response.rect));
                }
                if current == Some(item.id) {
                    if reveal_current {
                        response.scroll_to_me(Some(egui::Align::Center));
                    }
                    if look.outline_current {
                        ui.painter().rect_stroke(response.rect, TILE_ROUNDING, Stroke::new(CURRENT_OUTLINE_WIDTH, tk.accent), egui::StrokeKind::Inside);
                    }
                }
                if response.clicked() {
                    picked = Some(item.id);
                }
            }
        });
    }
    picked
}

/// A die on a see-through dark disc at `thumbnail`'s bottom-right corner: the item's result depends on a random seed
fn randomness_badge(painter: &egui::Painter, thumbnail: egui::Rect) {
    let center = thumbnail.right_bottom() - egui::vec2(BADGE_RADIUS, BADGE_RADIUS);
    painter.circle_filled(center, BADGE_RADIUS, BADGE_DISC);
    painter.text(center, Align2::CENTER_CENTER, ic::DICE_FIVE, FontId::proportional(BADGE_ICON_SIZE), BADGE_ICON);
}

/// Where the picker keeps its search text
fn search_id(popup_id: Id) -> Id {
    popup_id.with("search")
}

/// Set while the picker waits for its first frame open
fn focus_id(popup_id: Id) -> Id {
    popup_id.with("focus")
}
