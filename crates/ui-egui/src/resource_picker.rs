//! A popup hanging under a slot that picks a resource (a filter, a channel's source) from tiles under bright group headers, behind edge tabs and a search field

use egui::{Align, Align2, Color32, CornerRadius, FontId, Frame, Id, Rect, Response, Sense, Shadow, Stroke, StrokeKind, Ui, pos2, vec2};
use egui_phosphor::regular as ic;

use crate::i18n::tr;
use crate::picker_tab::PickerTab;
use crate::resource_picker_item::ResourcePickerItem;
use crate::theme::{Tokens, semibold};
use crate::thumbs::Thumbs;
use crate::tile_picture::TilePicture;

/// Columns of tiles in the grid
const COLUMNS: usize = 4;
/// One row of tiles: the picture, two lines of name and air
const ROW_HEIGHT: f32 = 86.0;
/// The grid's height, fixed so the popup keeps its place and size while searching or switching tabs; less only where the window ends sooner
const GRID_HEIGHT: f32 = 4.0 * ROW_HEIGHT;
/// Room kept between the popup's shadow and the window's bottom edge
const WINDOW_MARGIN: f32 = 8.0;
const TILE_GAP: f32 = 4.0;
const PICTURE_SIZE: f32 = 52.0;
const PICTURE_TOP: f32 = 6.0;
const PICTURE_ROUNDING: u8 = 3;
const TILE_ROUNDING: u8 = 5;
const NAME_SIZE: f32 = 10.0;
/// From the picture's top to the first name line's centre, and between the two lines
const NAME_CENTER: f32 = PICTURE_SIZE + 14.0;
const NAME_LINE: f32 = 12.0;
const NAME_MARGIN: f32 = 4.0;
const IMPORT_GLYPH_SIZE: f32 = 16.0;
const LOADING_GLYPH_SIZE: f32 = 24.0;
const GLYPH_SIZE: f32 = 22.0;

/// The bright group header row and the dim sub-caption row
const HEADER_HEIGHT: f32 = 24.0;
const HEADER_SIZE: f32 = 12.0;
const CAPTION_HEIGHT: f32 = 18.0;
const CAPTION_SIZE: f32 = 11.0;
/// Header and caption text from the grid's left edge
const HEADER_INSET: f32 = 4.0;

/// The edge tab strip and its tabs
const STRIP_HEIGHT: f32 = 28.0;
const STRIP_ROUNDING: u8 = 6;
const TAB_ROUNDING: u8 = 5;
/// Gap between the strip's top and an open tab's box
const TAB_TOP: f32 = 4.0;
/// A tab's width past its name: icon, gaps and padding
const TAB_PADDING: f32 = 38.0;
const TAB_GAP: f32 = 2.0;
const TAB_ICON_X: f32 = 14.0;
const TAB_TEXT_X: f32 = 26.0;
const TAB_ICON_SIZE: f32 = 13.0;
const TAB_TEXT_SIZE: f32 = 11.0;
const COUNT_SIZE: f32 = 10.0;
const COUNT_GAP: f32 = 4.0;

/// The search row and its field
const SEARCH_ROW: f32 = 36.0;
const FIELD_HEIGHT: f32 = 24.0;
const FIELD_ROUNDING: u8 = 5;
const SEARCH_TEXT_SIZE: f32 = 12.0;
const SEARCH_ICON_X: f32 = 13.0;
const SEARCH_TEXT_X: f32 = 25.0;
const CLEAR_X: f32 = 13.0;
const CLEAR_SIZE: f32 = 12.0;
const SEARCH_ICON_SIZE: f32 = 13.0;

/// Air from the popup's border to the search field and the grid, the grid's right room for the scroll bar, air under the grid
const SIDE: f32 = 5.0;
const SCROLL_ROOM: f32 = 10.0;
const BOTTOM: f32 = 6.0;
const POPUP_ROUNDING: u8 = 7;
const BORDER: f32 = 1.0;
const SHADOW_OFFSET: [i8; 2] = [2, 4];
const SHADOW: Color32 = Color32::from_rgb(12, 14, 18);

/// Opens the picker `popup_id` with an empty, focused search field, or closes it
pub(crate) fn toggle(ctx: &egui::Context, popup_id: Id) {
    if egui::Popup::is_id_open(ctx, popup_id) {
        egui::Popup::close_id(ctx, popup_id);
        return;
    }

    egui::Popup::open_id(ctx, popup_id);
    ctx.data_mut(|data| {
        data.insert_temp(search_id(popup_id), String::new());
        data.insert_temp(opened_id(popup_id), true);
    });
}

/// Draws picker `popup_id` under `slot`, at its width, while open and returns the id clicked; `tabs` runs only then.
/// It opens on the tab holding `current`, scrolled to it when `reveal_current`, else at the top
pub(crate) fn show<K: Clone + PartialEq>(
    slot: &Response,
    popup_id: Id,
    thumbs: &mut Thumbs,
    current: Option<&K>,
    reveal_current: bool,
    tabs: impl FnOnce(&mut Thumbs) -> Vec<PickerTab<K>>,
) -> Option<K> {
    let ctx = slot.ctx.clone();
    if !egui::Popup::is_id_open(&ctx, popup_id) {
        return None;
    }

    let tk = Tokens::get(&ctx);
    let tabs = tabs(thumbs);
    let width = slot.rect.width() - 2.0 * BORDER;
    let frame = Frame::NONE.fill(tk.panel).stroke(Stroke::new(BORDER, tk.field_border)).corner_radius(CornerRadius::same(POPUP_ROUNDING)).shadow(Shadow {
        offset: SHADOW_OFFSET,
        blur: 0,
        spread: 0,
        color: SHADOW,
    });
    // The picker always hangs under its slot, never over the row it edits: a short window gets a shorter grid
    let head = if tabs.len() > 1 { STRIP_HEIGHT } else { 0.0 } + SEARCH_ROW;
    let room = ctx.content_rect().bottom() - slot.rect.bottom() - head - BOTTOM - 2.0 * BORDER - WINDOW_MARGIN;
    let grid_height = room.clamp(ROW_HEIGHT, GRID_HEIGHT);
    let mut picked = None;
    egui::Popup::from_response(slot)
        .id(popup_id)
        .open_memory(None)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .align(egui::RectAlign::BOTTOM_START)
        .align_alternatives(&[])
        .gap(1.0)
        .width(width)
        .frame(frame)
        .show(|ui| {
            ui.set_width(width);
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            let just_opened = !ui.is_sizing_pass() && ui.data_mut(|data| data.remove_temp::<bool>(opened_id(popup_id))).unwrap_or(false);
            let current_tab = current.and_then(|current| tabs.iter().position(|tab| tab.items().any(|item| &item.id == current)));
            let stored_tab = ui.data(|data| data.get_temp::<usize>(tab_id(popup_id)));
            let mut open_tab = if just_opened { current_tab.or(stored_tab) } else { stored_tab.or(current_tab) }.unwrap_or(0).min(tabs.len().saturating_sub(1));

            let query = ui.data(|data| data.get_temp::<String>(search_id(popup_id))).unwrap_or_default().trim().to_lowercase();
            if tabs.len() > 1
                && let Some(clicked) = tab_strip(ui, &tabs, open_tab, &query)
            {
                open_tab = clicked;
            }
            ui.data_mut(|data| data.insert_temp(tab_id(popup_id), open_tab));
            let query = search_field(ui, popup_id, just_opened);

            if let Some(tab) = tabs.get(open_tab) {
                let reveal = just_opened && reveal_current;
                let mut scroll = egui::ScrollArea::vertical().id_salt((popup_id, open_tab)).max_height(grid_height);
                // A new opening with nothing to reveal starts at the top, not where the last one was left
                if just_opened && !reveal {
                    scroll = scroll.vertical_scroll_offset(0.0);
                }
                scroll.show(ui, |ui| {
                    // A search that leaves few tiles must not shrink the popup, or egui moves it
                    ui.set_min_height(grid_height);
                    picked = grid(ui, tab, &query, current, reveal.then_some(grid_height));
                });
            }
            ui.add_space(BOTTOM);
        });

    if picked.is_some() {
        egui::Popup::close_id(&ctx, popup_id);
    }
    picked
}

/// The edge tabs on the popup's top: a chrome strip, the open tab a panel-coloured box joined to the body; while searching each counts its matches
fn tab_strip<K>(ui: &mut Ui, tabs: &[PickerTab<K>], open: usize, query: &str) -> Option<usize> {
    let tk = Tokens::get(ui.ctx());
    let (strip, _) = ui.allocate_exact_size(vec2(ui.available_width(), STRIP_HEIGHT), Sense::hover());
    let top_only = CornerRadius { nw: STRIP_ROUNDING, ne: STRIP_ROUNDING, sw: 0, se: 0 };
    ui.painter().rect_filled(strip, top_only, tk.chrome);

    let mut clicked = None;
    let mut left = strip.left() + SIDE;
    for (index, tab) in tabs.iter().enumerate() {
        let is_open = index == open;
        let empty = !tab.items().next().is_some();
        let color = if is_open {
            tk.text
        } else if empty {
            tk.button_border
        } else {
            tk.text_dim
        };
        let name_font = if is_open { semibold(TAB_TEXT_SIZE) } else { FontId::proportional(TAB_TEXT_SIZE) };
        let name = ui.painter().layout_no_wrap(tab.name.to_owned(), name_font, color);
        let count = (!query.is_empty()).then(|| tab.items().filter(|item| item.matches(query)).count());
        let count_galley = count.map(|count| {
            let count_color = if count > 0 { tk.accent } else { tk.button_border };
            ui.painter().layout_no_wrap(count.to_string(), FontId::proportional(COUNT_SIZE), count_color)
        });
        let width = name.size().x + TAB_PADDING + count_galley.as_ref().map_or(0.0, |galley| COUNT_GAP + galley.size().x);

        let rect = Rect::from_min_max(pos2(left, strip.top() + TAB_TOP), pos2(left + width, strip.bottom()));
        let response = ui.interact(rect, ui.scope_id().with(("picker_tab", index)), Sense::click());
        response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Tab, true, is_open, tab.name));
        if is_open {
            ui.painter().rect_filled(rect, CornerRadius { nw: TAB_ROUNDING, ne: TAB_ROUNDING, sw: 0, se: 0 }, tk.panel);
        }
        let text_color = if response.hovered() && !is_open { tk.text } else { color };
        let center_y = strip.center().y + TAB_TOP / 2.0;
        ui.painter().text(pos2(left + TAB_ICON_X, center_y), Align2::CENTER_CENTER, tab.icon, FontId::proportional(TAB_ICON_SIZE), text_color);
        let name_left = left + TAB_TEXT_X;
        ui.painter().galley(pos2(name_left, center_y - name.size().y / 2.0), name.clone(), text_color);
        if let Some(galley) = count_galley {
            ui.painter().galley(pos2(name_left + name.size().x + COUNT_GAP, center_y - galley.size().y / 2.0), galley, Color32::PLACEHOLDER);
        }
        if response.clicked() {
            clicked = Some(index);
        }
        left += width + TAB_GAP;
    }
    clicked
}

/// The neutral search field (no accent outline, focused or not), focused the frame the picker opens, × clearing it; returns the lowercase query
fn search_field(ui: &mut Ui, popup_id: Id, just_opened: bool) -> String {
    let tk = Tokens::get(ui.ctx());
    let mut search: String = ui.data(|data| data.get_temp(search_id(popup_id))).unwrap_or_default();
    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), SEARCH_ROW), Sense::hover());
    let field = Rect::from_center_size(row.center(), vec2(row.width() - 2.0 * SIDE, FIELD_HEIGHT));
    let painter = ui.painter();
    painter.rect(field, CornerRadius::same(FIELD_ROUNDING), tk.field, Stroke::new(BORDER, tk.field_border), StrokeKind::Inside);
    painter.text(
        pos2(field.left() + SEARCH_ICON_X, field.center().y),
        Align2::CENTER_CENTER,
        ic::MAGNIFYING_GLASS,
        FontId::proportional(SEARCH_ICON_SIZE),
        tk.text_dim,
    );

    let text_rect = Rect::from_min_max(pos2(field.left() + SEARCH_TEXT_X, field.top()), pos2(field.right() - 2.0 * CLEAR_X, field.bottom()));
    let hint = egui::RichText::new(tr("Search")).color(tk.text_dim).size(SEARCH_TEXT_SIZE);
    let edit = egui::TextEdit::singleline(&mut search)
        .frame(Frame::NONE)
        .hint_text(hint)
        .font(semibold(SEARCH_TEXT_SIZE))
        .text_color(tk.text)
        .align(Align2::LEFT_CENTER)
        .desired_width(text_rect.width());
    let response = ui.put(text_rect, edit);
    if just_opened {
        response.request_focus();
    }

    if !search.is_empty() {
        let clear = Rect::from_center_size(pos2(field.right() - CLEAR_X, field.center().y), vec2(FIELD_HEIGHT, FIELD_HEIGHT));
        let clear = ui.interact(clear, popup_id.with("clear"), Sense::click());
        clear.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, tr("Clear search")));
        let color = if clear.hovered() { tk.text_strong } else { tk.text };
        ui.painter().text(clear.rect.center(), Align2::CENTER_CENTER, ic::X, FontId::proportional(CLEAR_SIZE), color);
        if clear.clicked() {
            search.clear();
            response.request_focus();
        }
    }

    let query = search.trim().to_lowercase();
    ui.data_mut(|data| data.insert_temp(search_id(popup_id), search));
    query
}

/// The open tab's items matching `query` as tiles under their headers and captions, `current` outlined and, given the grid's height in `reveal_in`, scrolled to; returns the one clicked
fn grid<K: Clone + PartialEq>(ui: &mut Ui, tab: &PickerTab<K>, query: &str, current: Option<&K>, reveal_in: Option<f32>) -> Option<K> {
    let tk = Tokens::get(ui.ctx());
    let left = ui.max_rect().left() + SIDE;
    let cell = (ui.available_width() - SIDE - SCROLL_ROOM) / COLUMNS as f32;
    let mut picked = None;
    let mut place = 0;
    for group in &tab.groups {
        let shown: Vec<&ResourcePickerItem<K>> = group.items.iter().filter(|item| item.matches(query)).collect();
        if shown.is_empty() {
            continue;
        }
        let group_top = ui.cursor().top();
        if let Some(header) = group.header {
            caption_row(ui, left, HEADER_HEIGHT, header, semibold(HEADER_SIZE), tk.text);
        }
        let mut caption_top = group_top;
        let mut row: Vec<(usize, &ResourcePickerItem<K>)> = Vec::new();
        let mut last_caption = None;
        for (index, item) in shown.iter().enumerate() {
            if item.caption != last_caption {
                flush_row(ui, &mut row, left, cell, current, &mut picked);
                if let Some(caption) = item.caption {
                    caption_top = ui.cursor().top();
                    caption_row(ui, left, CAPTION_HEIGHT, caption, FontId::proportional(CAPTION_SIZE), tk.text_dim);
                }
                last_caption = item.caption;
            }
            row.push((place, item));
            place += 1;
            if let Some(grid_height) = reveal_in
                && current == Some(&item.id)
            {
                // The current's header or caption shows above it when both fit, so the artist sees where it sits
                let row_top = ui.cursor().top();
                let label_top = if item.caption.is_some() { caption_top } else { group_top };
                let from = if row_top + ROW_HEIGHT - label_top <= grid_height { label_top } else { row_top };
                ui.scroll_to_rect(Rect::from_min_max(pos2(left, from), pos2(left + cell, row_top + ROW_HEIGHT)), Some(Align::TOP));
            }
            if row.len() == COLUMNS || index + 1 == shown.len() {
                flush_row(ui, &mut row, left, cell, current, &mut picked);
            }
        }
    }
    picked
}

/// A header or caption line of `height`, its text `font` in `color`
fn caption_row(ui: &mut Ui, left: f32, height: f32, text: &str, font: FontId, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    ui.painter().text(pos2(left + HEADER_INSET, rect.center().y), Align2::LEFT_CENTER, text, font, color);
}

/// Lays out the tiles in `row` as one grid row and empties it, recording a click in `picked`
fn flush_row<K: Clone + PartialEq>(
    ui: &mut Ui,
    row: &mut Vec<(usize, &ResourcePickerItem<K>)>,
    left: f32,
    cell: f32,
    current: Option<&K>,
    picked: &mut Option<K>,
) {
    if row.is_empty() {
        return;
    }
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_HEIGHT), Sense::hover());
    for (column, (place, item)) in row.drain(..).enumerate() {
        let x = left + column as f32 * cell;
        let tile = Rect::from_min_max(pos2(x + 1.0, rect.top()), pos2(x + cell - 1.0, rect.bottom() - TILE_GAP));
        if tile_ui(ui, tile, (place, item), current == Some(&item.id)).clicked() {
            *picked = Some(item.id.clone());
        }
    }
}

/// One tile, `place` its order in the tab: its picture, its name in up to two lines; hovered = button fill, current = accent outline and a bold name
fn tile_ui<K>(ui: &mut Ui, rect: Rect, (place, item): (usize, &ResourcePickerItem<K>), chosen: bool) -> Response {
    let tk = Tokens::get(ui.ctx());
    let response = ui.interact(rect, ui.scope_id().with(("picker_tile", place)), Sense::click());
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::ListBoxOption, true, chosen, &item.label));
    if !ui.is_rect_visible(rect) {
        return response;
    }

    let hovered = response.hovered();
    let painter = ui.painter();
    if hovered {
        painter.rect_filled(rect, CornerRadius::same(TILE_ROUNDING), tk.button);
    }
    if chosen {
        painter.rect_stroke(rect, CornerRadius::same(TILE_ROUNDING), Stroke::new(BORDER, tk.accent), StrokeKind::Inside);
    }

    let picture = Rect::from_min_size(pos2(rect.center().x - PICTURE_SIZE / 2.0, rect.top() + PICTURE_TOP), vec2(PICTURE_SIZE, PICTURE_SIZE));
    let rounding = CornerRadius::same(PICTURE_ROUNDING);
    match &item.picture {
        TilePicture::Texture(Some(texture)) => {
            egui::Image::from_texture((texture.id(), picture.size())).corner_radius(rounding).paint_at(ui, picture);
        }
        TilePicture::Texture(None) => {
            painter.text(
                picture.center(),
                Align2::CENTER_CENTER,
                ic::ARROWS_CLOCKWISE,
                FontId::proportional(LOADING_GLYPH_SIZE),
                tk.text_dim.gamma_multiply(0.5),
            );
        }
        TilePicture::Swatch(color) => {
            painter.rect_filled(picture, rounding, *color);
        }
        TilePicture::Glyph(glyph) => {
            painter.rect(picture, rounding, tk.field, Stroke::new(BORDER, tk.field_border), StrokeKind::Inside);
            painter.text(picture.center(), Align2::CENTER_CENTER, *glyph, FontId::proportional(GLYPH_SIZE), tk.text_dim);
        }
        TilePicture::Import => {
            painter.rect(picture, rounding, tk.field, Stroke::new(BORDER, tk.field_border), StrokeKind::Inside);
            let color = if hovered { tk.text } else { tk.text_dim };
            painter.text(picture.center(), Align2::CENTER_CENTER, ic::PLUS, FontId::proportional(IMPORT_GLYPH_SIZE), color);
        }
    }
    name_lines(ui, rect, &item.label, hovered || chosen, chosen);

    response
}

/// The tile's name under its picture: whole words on the first line, the rest on a second, each cut with "…" when too long
fn name_lines(ui: &Ui, rect: Rect, label: &str, bright: bool, bold: bool) {
    let tk = Tokens::get(ui.ctx());
    let painter = ui.painter();
    let color = if bright { tk.text } else { tk.text_dim };
    let font = if bold { semibold(NAME_SIZE) } else { FontId::proportional(NAME_SIZE) };
    let room = rect.width() - NAME_MARGIN;
    let fits = |text: &str| painter.layout_no_wrap(text.to_owned(), font.clone(), color).size().x <= room;

    let words: Vec<&str> = label.split_whitespace().collect();
    let mut taken = 0;
    while taken < words.len() && fits(&words[..=taken].join(" ")) {
        taken += 1;
    }
    let lines = if taken == 0 || taken == words.len() { vec![label.to_owned()] } else { vec![words[..taken].join(" "), words[taken..].join(" ")] };
    for (index, line) in lines.iter().enumerate() {
        let mut job = egui::text::LayoutJob::simple_singleline(line.clone(), font.clone(), color);
        job.wrap = egui::text::TextWrapping::truncate_at_width(room);
        let galley = painter.layout_job(job);
        let center = pos2(rect.center().x, rect.top() + PICTURE_TOP + NAME_CENTER + index as f32 * NAME_LINE);
        painter.galley(center - galley.size() / 2.0, galley, color);
    }
}

/// Where the picker keeps its search text
fn search_id(popup_id: Id) -> Id {
    popup_id.with("search")
}

/// Set while the picker waits for its first frame open
fn opened_id(popup_id: Id) -> Id {
    popup_id.with("opened")
}

/// The tab the picker shows
fn tab_id(popup_id: Id) -> Id {
    popup_id.with("tab")
}
