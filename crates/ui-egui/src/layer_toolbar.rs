//! The Layers panel's top row: the displayed channel, the Add effect and Add mask menus, the add-layer buttons and remove, and the
//! texture picker Add bitmap mask opens first

use egui::{Align2, Color32, CornerRadius, FontId, Frame, Id, Painter, Pos2, Rect, Response, Sense, Shadow, Stroke, StrokeKind, Ui, pos2, vec2};
use egui_phosphor::regular as ic;
use serde_json::{Value, json};
use texturecraft_engine::compose::ImageStore;
use texturecraft_engine::core::{Layer, LayerId};
use texturecraft_engine::fill_alpha::library_alpha_of;
use texturecraft_engine::paint::alphas;

use crate::TextureCraftApp;
use crate::i18n::tr;
use crate::picker_group::PickerGroup;
use crate::picker_tab::PickerTab;
use crate::resource_picker;
use crate::resource_picker_item::ResourcePickerItem;
use crate::theme::Tokens;
use crate::thumbs::Thumbs;
use crate::tile_picture::TilePicture;

/// The row's height and its controls' height
const BAR_HEIGHT: f32 = 40.0;
const CONTROL_HEIGHT: f32 = 26.0;
const CONTROL_ROUNDING: u8 = 5;
const SIDE: f32 = 8.0;
const CHANNEL_WIDTH: f32 = 88.0;
/// A menu button: its icon, then its caret
const MENU_BUTTON_WIDTH: f32 = 32.0;
const MENU_ICON_X: f32 = 11.0;
const MENU_CARET_INSET: f32 = 8.0;
const MENU_ICON_SIZE: f32 = 14.0;
const MASK_ICON_SIZE: f32 = 12.0;
const CARET_SIZE: f32 = 11.0;
const BUTTON_WIDTH: f32 = 24.0;
const ICON_SIZE: f32 = 14.0;
/// Air after the channel list, between the two menu buttons, before the add-layer buttons and between them
const CHANNEL_GAP: f32 = 6.0;
const MENU_GAP: f32 = 3.0;
const GROUP_GAP: f32 = 8.0;
const BUTTON_GAP: f32 = 2.0;
const CHANNEL_TEXT_X: f32 = 8.0;
const CHANNEL_CARET_INSET: f32 = 10.0;
const CHANNEL_CARET_SIZE: f32 = 12.0;
const TEXT_SIZE: f32 = 12.0;

/// A hanging menu: its caption, rows, icon and text columns and its smallest width
const MENU_PADDING: f32 = 4.0;
const MENU_CAPTION_HEIGHT: f32 = 22.0;
const MENU_CAPTION_X: f32 = 12.0;
const MENU_CAPTION_SIZE: f32 = 10.0;
const MENU_ROW: f32 = 24.0;
const MENU_ROW_INSET: f32 = 4.0;
const MENU_ROW_ROUNDING: u8 = 4;
const MENU_ITEM_ICON_X: f32 = 18.0;
const MENU_ITEM_ICON_SIZE: f32 = 13.0;
const MENU_ITEM_SWATCH: f32 = 12.0;
const MENU_TEXT_X: f32 = 34.0;
const MENU_RIGHT: f32 = 22.0;
const MENU_MIN_WIDTH: f32 = 120.0;
const MENU_ROUNDING: u8 = 6;
const SHADOW_OFFSET: [i8; 2] = [3, 4];
const SHADOW: Color32 = Color32::from_rgb(12, 14, 18);
/// The two swatches of Add mask's White and Black
const WHITE_SWATCH: u8 = 235;
const BLACK_SWATCH: u8 = 10;

/// The filter Add filter adds: at its defaults it changes nothing, and its slot opens the picker to swap it
pub(crate) const NEW_FILTER: &str = "none";

/// What an Add menu item shows at its left
#[derive(Clone, Copy)]
enum ItemIcon {
    Glyph(&'static str),
    /// A gray swatch (Add mask's White and Black)
    Swatch(u8),
}

/// One item of the Add effect or Add mask menu
struct AddItem {
    icon: ItemIcon,
    /// The toolbar menu's name (the button says "Add effect" / "Add mask") and the layer context menu's
    short: &'static str,
    long: &'static str,
    command: &'static str,
    params: Value,
    enabled: bool,
}

/// What a click in one of the row's menus asks for, done once the menu has closed
enum Pick {
    Run { command: &'static str, params: Value },
    Bitmap,
}

/// The Layers panel's top row and what its menus open
pub(crate) fn show(app: &mut TextureCraftApp, ui: &mut Ui) {
    let tokens = Tokens::get(ui.ctx());
    let (bar, _) = ui.allocate_exact_size(vec2(ui.available_width(), BAR_HEIGHT), Sense::hover());
    let anchor = ui.interact(bar, ui.scope_id().with("layer_toolbar"), Sense::hover());
    let top = bar.center().y - CONTROL_HEIGHT * 0.5;
    let mut x = bar.left() + SIDE;
    let control = |x: f32, width: f32| Rect::from_min_size(pos2(x, top), vec2(width, CONTROL_HEIGHT));

    channel_list(app, ui, control(x, CHANNEL_WIDTH), &tokens);
    x += CHANNEL_WIDTH + CHANNEL_GAP;

    let selected = app.session.selected_layer().and_then(|id| app.session.doc.project.layers.get(&id)).cloned();
    let effect_items = effect_items(app, selected.as_ref());
    let mut pick = menu_button(ui, control(x, MENU_BUTTON_WIDTH), tr("Add effect"), MenuIcon::Glyph(ic::MAGIC_WAND), tr("Add effect"), &effect_items, &tokens);
    x += MENU_BUTTON_WIDTH + MENU_GAP;
    let mask_items = mask_items(selected.as_ref());
    pick = pick.or(menu_button(ui, control(x, MENU_BUTTON_WIDTH), tr("Add mask"), MenuIcon::Mask, tr("Add mask"), &mask_items, &tokens));
    x += MENU_BUTTON_WIDTH + MENU_GAP + GROUP_GAP - MENU_GAP;

    for (glyph, label, command) in [
        (ic::PAINT_BRUSH, tr("Add paint layer"), "layer.add.paint"),
        (ic::PAINT_BUCKET, tr("Add fill layer"), "layer.add.fill"),
        (ic::FOLDER_SIMPLE, tr("Add folder"), "layer.add.group"),
    ] {
        if plain_button(ui, control(x, BUTTON_WIDTH), glyph, label, &tokens).clicked() {
            app.run(command, Value::Null);
        }
        x += BUTTON_WIDTH + BUTTON_GAP;
    }
    let remove = control((bar.right() - SIDE - BUTTON_WIDTH).max(x), BUTTON_WIDTH);
    if plain_button(ui, remove, ic::TRASH, &crate::keymap::tip(&app.ui.keymap, tr("Remove layer"), "layer.remove"), &tokens).clicked() {
        app.run("layer.remove", Value::Null);
    }

    let popup = ui.scope_id().with("bitmap_mask_picker");
    match (pick, selected.as_ref()) {
        (Some(Pick::Run { command, params }), Some(layer)) => run_on(app, ui, layer, command, params),
        (Some(Pick::Bitmap), Some(_)) => resource_picker::toggle(ui.ctx(), popup),
        _ => {}
    }
    if let Some(layer) = selected {
        bitmap_picker(app, &anchor, popup, layer.id);
    }
}

/// The displayed channel: a list hanging from its button
fn channel_list(app: &mut TextureCraftApp, ui: &mut Ui, rect: Rect, tokens: &Tokens) {
    let response = ui.interact(rect, ui.scope_id().with("layer_channel"), Sense::click());
    let current = tr(app.ui.layer_channel.label());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, current));
    let painter = ui.painter();
    let fill = if response.hovered() { tokens.button.lerp_to_gamma(tokens.text, 0.06) } else { tokens.button };
    painter.rect(rect, CornerRadius::same(CONTROL_ROUNDING), fill, Stroke::new(1.0, tokens.button_border), StrokeKind::Inside);
    let caret_x = rect.right() - CHANNEL_CARET_INSET;
    let text =
        crate::widgets::elided(painter, current.to_owned(), TEXT_SIZE, tokens.text, caret_x - CHANNEL_CARET_SIZE * 0.5 - rect.left() - CHANNEL_TEXT_X - 2.0);
    painter.galley(pos2(rect.left() + CHANNEL_TEXT_X, rect.center().y - text.size().y * 0.5), text, tokens.text);
    painter.text(pos2(caret_x, rect.center().y), Align2::CENTER_CENTER, ic::CARET_DOWN, FontId::proportional(CHANNEL_CARET_SIZE), tokens.text);

    let channels = app.session.doc.project.texture_set(app.session.ts()).map(|set| set.channel_list()).unwrap_or_default();
    egui::Popup::menu(&response).id(ui.scope_id().with("layer_channel_list")).width(rect.width()).gap(1.0).show(|ui| {
        ui.set_min_width(rect.width());
        for channel in channels {
            ui.selectable_value(&mut app.ui.layer_channel, channel, tr(channel.label()));
        }
    });
}

/// The icon of a menu button
#[derive(Clone, Copy)]
enum MenuIcon {
    Glyph(&'static str),
    /// Add mask's square, half of it filled corner to corner
    Mask,
}

/// A button with its icon and a caret whose menu hangs from it (caption, then `items`); the item picked
fn menu_button(ui: &mut Ui, rect: Rect, label: &str, icon: MenuIcon, caption: &str, items: &[AddItem], tokens: &Tokens) -> Option<Pick> {
    let response = ui.interact(rect, ui.scope_id().with(("layer_toolbar_menu", label)), Sense::click()).on_hover_text(label);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
    let popup = ui.scope_id().with(("layer_toolbar_menu_popup", label));
    let open = egui::Popup::is_id_open(ui.ctx(), popup);
    let painter = ui.painter();
    if response.hovered() && !open {
        painter.rect_filled(rect, CornerRadius::same(CONTROL_ROUNDING), tokens.button);
    }
    button_face(painter, rect, icon, tokens);

    let width = menu_width(ui, items);
    let frame = Frame::NONE
        .fill(tokens.chrome)
        .stroke(Stroke::new(1.0, tokens.field_border))
        .corner_radius(CornerRadius { nw: 0, ..CornerRadius::same(MENU_ROUNDING) })
        .shadow(Shadow { offset: SHADOW_OFFSET, blur: 0, spread: 0, color: SHADOW });
    let mut picked = None;
    let shown = egui::Popup::menu(&response).id(popup).align(egui::RectAlign::BOTTOM_START).gap(-1.0).frame(frame).width(width).show(|ui| {
        ui.set_width(width);
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        ui.add_space(MENU_PADDING);
        let (head, _) = ui.allocate_exact_size(vec2(width, MENU_CAPTION_HEIGHT - MENU_PADDING), Sense::hover());
        let caption = egui::RichText::new(caption.to_uppercase()).size(MENU_CAPTION_SIZE).strong().color(tokens.text_dim);
        let galley = egui::WidgetText::from(caption).into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, egui::TextStyle::Body);
        ui.painter().galley(pos2(head.left() + MENU_CAPTION_X, head.center().y - galley.size().y * 0.5), galley, tokens.text_dim);
        for item in items {
            if menu_row(ui, width, item, tokens).clicked() {
                picked = Some(if item.command == BITMAP { Pick::Bitmap } else { Pick::Run { command: item.command, params: item.params.clone() } });
                ui.close();
            }
        }
        ui.add_space(MENU_PADDING);
    });
    // The open button becomes the menu's tab: same fill and border, no line between them
    if let Some(shown) = shown {
        let painter = ui.ctx().layer_painter(shown.response.layer_id);
        let tab = Rect::from_min_max(rect.min, pos2(rect.right(), shown.response.rect.top() + 1.5));
        let top = CornerRadius { nw: CONTROL_ROUNDING, ne: CONTROL_ROUNDING, sw: 0, se: 0 };
        painter.rect(tab, top, tokens.chrome, Stroke::new(1.0, tokens.field_border), StrokeKind::Inside);
        painter.rect_filled(
            Rect::from_min_max(pos2(tab.left() + 1.0, tab.bottom() - 2.5), pos2(tab.right() - 1.0, tab.bottom())),
            CornerRadius::ZERO,
            tokens.chrome,
        );
        painter.vline(tab.left() + 0.5, tab.bottom() - 2.5..=tab.bottom(), Stroke::new(1.0, tokens.field_border));
        button_face(&painter, rect, icon, tokens);
    }
    picked
}

/// A menu button's icon and caret
fn button_face(painter: &Painter, rect: Rect, icon: MenuIcon, tokens: &Tokens) {
    let icon_at = pos2(rect.left() + MENU_ICON_X, rect.center().y);
    match icon {
        MenuIcon::Glyph(glyph) => {
            painter.text(icon_at, Align2::CENTER_CENTER, glyph, FontId::proportional(MENU_ICON_SIZE), tokens.text);
        }
        MenuIcon::Mask => mask_icon(painter, icon_at, MASK_ICON_SIZE, tokens.text),
    }
    painter.text(pos2(rect.right() - MENU_CARET_INSET, rect.center().y), Align2::CENTER_CENTER, ic::CARET_DOWN, FontId::proportional(CARET_SIZE), tokens.text);
}

/// The menu's width: its longest item, with room for the icons and the right edge
fn menu_width(ui: &Ui, items: &[AddItem]) -> f32 {
    let widest = items
        .iter()
        .map(|item| ui.fonts_mut(|fonts| fonts.layout_no_wrap(tr(item.short).to_owned(), FontId::proportional(TEXT_SIZE), Color32::WHITE).size().x))
        .fold(0.0, f32::max);
    (widest + MENU_TEXT_X + MENU_RIGHT).max(MENU_MIN_WIDTH)
}

/// One menu row: hover face, icon, name; dim and unclickable when it can't go where the selection is
fn menu_row(ui: &mut Ui, width: f32, item: &AddItem, tokens: &Tokens) -> Response {
    let sense = if item.enabled { Sense::click() } else { Sense::hover() };
    let (rect, response) = ui.allocate_exact_size(vec2(width, MENU_ROW), sense);
    let label = tr(item.short);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, item.enabled, label));
    let painter = ui.painter();
    if item.enabled && response.hovered() {
        painter.rect_filled(rect.shrink2(vec2(MENU_ROW_INSET, 0.0)), CornerRadius::same(MENU_ROW_ROUNDING), tokens.button);
    }
    let color = if item.enabled { tokens.text } else { tokens.text_dim };
    let icon_at = pos2(rect.left() + MENU_ITEM_ICON_X, rect.center().y);
    match item.icon {
        ItemIcon::Glyph(glyph) => {
            painter.text(icon_at, Align2::CENTER_CENTER, glyph, FontId::proportional(MENU_ITEM_ICON_SIZE), color);
        }
        ItemIcon::Swatch(gray) => {
            let swatch = Rect::from_center_size(icon_at, vec2(MENU_ITEM_SWATCH, MENU_ITEM_SWATCH));
            painter.rect(swatch, CornerRadius::same(2), Color32::from_gray(gray), Stroke::new(1.0, tokens.field_border), StrokeKind::Inside);
        }
    }
    painter.text(pos2(rect.left() + MENU_TEXT_X, rect.center().y), Align2::LEFT_CENTER, label, FontId::proportional(TEXT_SIZE), color);
    response
}

/// A plain icon button of the row: a button face while hovered
fn plain_button(ui: &mut Ui, rect: Rect, glyph: &str, label: &str, tokens: &Tokens) -> Response {
    let response = ui.interact(rect, ui.scope_id().with(("layer_toolbar_button", glyph)), Sense::click()).on_hover_text(label);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
    if response.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(CONTROL_ROUNDING), tokens.button);
    }
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, glyph, FontId::proportional(ICON_SIZE), tokens.text);
    response
}

/// Add mask's icon: a square, half of it filled corner to corner
pub(crate) fn mask_icon(painter: &Painter, center: Pos2, size: f32, color: Color32) {
    let half = size * 0.5;
    let filled = vec![pos2(center.x + half, center.y - half), pos2(center.x + half, center.y + half), pos2(center.x - half, center.y + half)];
    painter.add(egui::Shape::convex_polygon(filled, color, Stroke::NONE));
    painter.rect_stroke(Rect::from_center_size(center, vec2(size, size)), CornerRadius::same(2), Stroke::new(1.5, color), StrokeKind::Inside);
}

/// Selects `layer` when it isn't, runs `command` on it, and shows a filter Add filter added in Properties with its picker open
fn run_on(app: &mut TextureCraftApp, ui: &Ui, layer: &Layer, command: &str, params: Value) {
    if app.session.selected_layer() != Some(layer.id) {
        app.run("layer.select", json!({"layer": layer.id.0}));
    }
    let mut params = params;
    if let Some(object) = params.as_object_mut() {
        object.insert("layer".into(), json!(layer.id.0));
    }
    let added = app.run(command, params);
    if command == "effect.add.filter"
        && let Some(effect) = added.as_ref().and_then(|result| result["effect"].as_u64())
    {
        app.ui.dock.focus(crate::docking::Panel::Properties);
        crate::filter_slot::open_picker_when_shown(ui.ctx(), effect);
    }
}

/// The Add effect items for `layer`: on its content when the content is the selected target, else in its mask; an item that can't go
/// there is greyed, not sent elsewhere (all of them without a layer)
fn effect_items(app: &TextureCraftApp, layer: Option<&Layer>) -> Vec<AddItem> {
    let content = layer.is_some_and(|layer| !layer.is_group() && app.session.selected_layer() == Some(layer.id) && crate::dock::targets_content(app));
    let target = if content { "content" } else { "mask" };
    // Generators, compare masks and colour selections go in masks only
    let anywhere = layer.is_some();
    let mask_only = anywhere && !content;
    let item = |icon, short, long, command, params: Value, enabled| AddItem { icon: ItemIcon::Glyph(icon), short, long, command, params, enabled };
    vec![
        item(ic::SPARKLE, "Generator", "Add generator", "effect.add.generator", json!({}), mask_only),
        item(ic::PAINT_BRUSH, "Paint", "Add paint", "effect.add.paint", json!({"target": target}), anywhere),
        item(ic::PAINT_BUCKET, "Fill", "Add fill", "effect.add.fill", json!({"target": target}), anywhere),
        item(ic::CHART_BAR, "Levels", "Add levels", "effect.add.levels", json!({"target": target}), anywhere),
        item(ic::SCALES, "Compare mask", "Add compare mask", "effect.add.compare", json!({}), mask_only),
        item(ic::FUNNEL, "Filter", "Add filter", "effect.add.filter", json!({"target": target, "filter": NEW_FILTER}), anywhere),
        item(ic::SELECTION, "Color selection", "Add color selection", "effect.add.colorSelection", json!({}), mask_only),
        item(ic::ANCHOR, "Anchor point", "Add anchor point", "effect.add.anchor", json!({"target": target}), anywhere),
    ]
}

/// The command of Add mask's Bitmap: the texture picker opens first and adds the mask with the picked texture
const BITMAP: &str = "mask.add.bitmap";

/// The Add mask items: a layer that has a mask of its own adds to it through Add effect instead
fn mask_items(layer: Option<&Layer>) -> Vec<AddItem> {
    let enabled = layer.is_some_and(|layer| layer.mask.is_none());
    let item = |icon, short, long, command| AddItem { icon, short, long, command, params: json!({}), enabled };
    vec![
        item(ItemIcon::Swatch(WHITE_SWATCH), "White", "Add white mask", "mask.add.white"),
        item(ItemIcon::Swatch(BLACK_SWATCH), "Black", "Add black mask", "mask.add.black"),
        item(ItemIcon::Glyph(ic::IMAGE), "Bitmap", "Add bitmap mask", BITMAP),
        item(ItemIcon::Glyph(ic::SELECTION), "Color selection", "Add mask with color selection", "mask.add.colorSelection"),
        item(ItemIcon::Glyph(ic::SCALES), "Height combination", "Add mask with height combination", "mask.add.heightCombination"),
    ]
}

/// The layer context menu's Add effect items (its own buttons, the long names)
pub(crate) fn effect_menu_items(app: &mut TextureCraftApp, ui: &mut Ui, layer: &Layer) {
    for item in effect_items(app, Some(layer)) {
        let text = format!("{}  {}", icon_of(item.icon), tr(item.long));
        if ui.add_enabled(item.enabled, egui::Button::new(text)).clicked() {
            run_on(app, ui, layer, item.command, item.params);
            ui.close();
        }
    }
}

/// The Add mask items of a layer row's empty mask slot (the long names); Bitmap opens the picker under the layer row
pub(crate) fn mask_slot_menu(app: &mut TextureCraftApp, ui: &mut Ui, layer: &Layer, picker: Id) {
    for item in mask_items(Some(layer)) {
        if ui.add_enabled(item.enabled, egui::Button::new(tr(item.long))).clicked() {
            if item.command == BITMAP {
                resource_picker::toggle(ui.ctx(), picker);
            } else {
                run_on(app, ui, layer, item.command, item.params);
            }
            ui.close();
        }
    }
}

fn icon_of(icon: ItemIcon) -> &'static str {
    match icon {
        ItemIcon::Glyph(glyph) => glyph,
        ItemIcon::Swatch(_) => ic::SQUARE,
    }
}

/// What the bitmap picker offers: Import..., an image resource, a library alpha
#[derive(Clone, PartialEq)]
enum BitmapChoice {
    Import,
    Image(String),
    Alpha(&'static str),
}

/// The texture picker of Add bitmap mask, hanging under `anchor` at its width while open: a pick adds the mask to `layer`, Escape or a
/// click elsewhere adds nothing
pub(crate) fn bitmap_picker(app: &mut TextureCraftApp, anchor: &Response, popup: Id, layer: LayerId) {
    let ctx = anchor.ctx.clone();
    let images = app.session.images.clone();
    let tabs = |thumbs: &mut Thumbs| bitmap_tabs(thumbs, &ctx, &images);
    match resource_picker::show(anchor, popup, &mut app.thumbs, None, false, tabs) {
        Some(BitmapChoice::Import) => crate::menus::invoke(app, &ctx, "resource.import"),
        Some(BitmapChoice::Image(name)) => {
            app.run("layer.select", json!({"layer": layer.0}));
            app.run(BITMAP, json!({"layer": layer.0, "image": name}));
        }
        Some(BitmapChoice::Alpha(alpha)) => {
            app.run("layer.select", json!({"layer": layer.0}));
            app.run(BITMAP, json!({"layer": layer.0, "alpha": alpha}));
        }
        None => {}
    }
}

/// The bitmap picker's one tab: Textures (Import... first) and the Alphas in their shelf groups
fn bitmap_tabs(thumbs: &mut Thumbs, ctx: &egui::Context, images: &ImageStore) -> Vec<PickerTab<BitmapChoice>> {
    let tile = |id, label: &str, caption, picture| ResourcePickerItem { id, label: label.to_owned(), caption, picture };
    let mut textures = vec![tile(BitmapChoice::Import, tr("Import..."), None, TilePicture::Import)];
    let mut names: Vec<&String> = images.keys().filter(|name| library_alpha_of(name).is_none()).collect();
    names.sort();
    textures.extend(
        names
            .into_iter()
            .filter_map(|name| images.get(name).map(|image| (name, image)))
            .map(|(name, image)| tile(BitmapChoice::Image(name.clone()), name, None, TilePicture::Texture(Some(thumbs.image(ctx, name, image))))),
    );
    let alphas = alphas::library()
        .iter()
        .map(|alpha| tile(BitmapChoice::Alpha(alpha.id.as_str()), tr(&alpha.label), Some(tr(alpha.category)), TilePicture::Texture(thumbs.alpha(ctx, alpha))))
        .collect();
    let groups = vec![PickerGroup { header: Some(tr("Textures")), items: textures }, PickerGroup { header: Some(tr("Alphas")), items: alphas }];
    vec![PickerTab { icon: ic::SQUARES_FOUR, name: tr("Resources"), groups }]
}
