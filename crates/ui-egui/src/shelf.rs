//! The Assets panel: built-in, saved and project assets in one grid by type, library and search, dragged onto the layer stack, the views or the brush

use egui::{Color32, RichText, Sense, Stroke, Ui, vec2};
use egui_phosphor::regular as ic;
use serde_json::json;
#[cfg(not(target_arch = "wasm32"))]
use texturecraft_engine::formats::mcmat::MCMAT_EXT;
use texturecraft_engine::geom::glam::Vec2;
use texturecraft_engine::orb_preview::MAX_PREVIEW_SIZE_PX;

use crate::TextureCraftApp;
use crate::decal_drop::DecalDrop;
use crate::i18n::{tr, trf};
use crate::shelf_entry::ShelfEntry;
use crate::shelf_preview::ShelfPreview;
use crate::theme::Tokens;
use crate::thumbs::Thumbs;
use crate::widgets::dnd_release;

/// A shelf tile's size and the gap between tiles in points: a 68 x 78 grid step, as in the reference
const TILE_WIDTH: f32 = 66.0;
const TILE_HEIGHT: f32 = 76.0;
const TILE_GAP: f32 = 2.0;
/// Corner radius of a tile's hover and selection highlights, in points
const TILE_ROUNDING: f32 = 3.0;
/// The last clicked or right-clicked tile's [`ShelfEntry::key`], kept in egui's memory under this id
const SELECTED_TILE: &str = "shelf_selected_tile";
/// The outline widths of the selected tile and of the one in use (the brush's alpha, the shown environment), in points
const SELECTED_OUTLINE_WIDTH: f32 = 1.5;
const IN_USE_OUTLINE_WIDTH: f32 = 1.0;
/// The randomness badge's dot radius, dot spacing and inset from the thumbnail's corner, in points
const BADGE_DOT_RADIUS: f32 = 1.3;
const BADGE_DOT_SPACING: f32 = 3.6;
const BADGE_INSET: f32 = 2.5;
/// The randomness badge's dot colour and its rim's width (points) and colour
const BADGE_DOT_FILL: Color32 = Color32::from_gray(205);
const BADGE_DOT_RIM: f32 = 0.8;
const BADGE_DOT_RIM_COLOR: Color32 = Color32::from_black_alpha(170);
/// The thumbnail's size in a tile and its center below the tile's top, in points
const THUMBNAIL_SIZE: f32 = 60.0;
const THUMBNAIL_CENTER_Y: f32 = 31.0;
/// Loading icon size relative to the thumbnail width, and its opacity over the dim text color
const LOADING_ICON_SCALE: f32 = 0.9;
const LOADING_ICON_OPACITY: f32 = 0.5;
/// A tile label's font size, its gap below the thumbnail and its total side margin, in points
const LABEL_FONT_SIZE: f32 = 9.5;
const LABEL_GAP: f32 = 3.0;
const LABEL_MARGIN: f32 = 4.0;
/// The hover preview's size and its gap above the label, in points
const BIG_PREVIEW_SIZE: f32 = 256.0;
const BIG_PREVIEW_LABEL_GAP: f32 = 4.0;
/// Dwell in seconds before rendering, shorter than the popup delay
const BIG_PREVIEW_DWELL: f32 = 0.1;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tab {
    Materials,
    Smart,
    Masks,
    Filters,
    Brushes,
    Alphas,
    /// Procedural patterns and the project's images
    Textures,
    Environments,
}

/// The shelf tabs: icon and (English, translated where shown) name
pub(crate) const TABS: [(Tab, &str, &str); 8] = [
    (Tab::Materials, ic::SPHERE, "Materials"),
    (Tab::Smart, ic::STACK, "Smart materials"),
    (Tab::Masks, ic::CIRCLE_HALF, "Smart masks"),
    (Tab::Filters, ic::FUNNEL, "Filters"),
    (Tab::Brushes, ic::PAINT_BRUSH_BROAD, "Brushes"),
    (Tab::Alphas, ic::SHAPES, "Alphas"),
    (Tab::Textures, ic::IMAGE, "Textures"),
    (Tab::Environments, ic::SUN_HORIZON, "Environments"),
];

/// A shelf asset: built in (by id), a saved smart material (file path) or an image resource.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Asset {
    Material(&'static str),
    SmartMask(&'static str),
    Filter(&'static str),
    Alpha(&'static str),
    Library(String),
    Texture(String),
}

impl Asset {
    /// Assets that add layers (built-in and saved smart materials).
    pub fn is_material(&self) -> bool {
        matches!(self, Asset::Material(_) | Asset::Library(_))
    }

    /// Assets a 3D view drop puts on the texture set under the pointer
    pub fn goes_onto_set(&self) -> bool {
        self.is_material() || matches!(self, Asset::Texture(_))
    }
}

/// The drag-and-drop payload of a shelf tile.
#[derive(Clone, Debug)]
pub struct ShelfDrag {
    pub asset: Asset,
    pub label: String,
}

/// Apply `asset` the way a shelf click does: a material above the selection, a smart mask on
/// the selected layer, a filter on the selection (its mask when the mask is selected), an alpha
/// on the brush, a texture as an image fill.
pub fn apply(app: &mut TextureCraftApp, asset: &Asset) {
    let (cmd, params) = match asset {
        Asset::Material(id) => ("material.apply", json!({"material": id})),
        Asset::SmartMask(id) => ("mask.smart.apply", json!({"name": id})),
        Asset::Filter(id) => {
            let content = crate::dock::targets_content(app);
            if app.run("effect.add.filter", json!({"filter": id, "target": if content { "content" } else { "mask" }})).is_some()
                && content
                && let Some(name) = unpainted_paint_layer(app)
            {
                // Otherwise the filter seems to do nothing: it has nothing to change yet.
                app.toast(trf("\"{layer}\" has no paint yet: the filter changes what you paint on it.", &[("layer", &name)]), false);
            }
            return;
        }
        Asset::Alpha(id) => ("tool.brush", json!({"alpha": {"library": id}})),
        Asset::Library(path) => ("material.load", json!({"path": path})),
        Asset::Texture(name) => ("layer.add.image", json!({"name": name})),
    };
    app.run(cmd, params);
}

/// The selected layer's name when it is a paint layer with nothing painted (in itself or in its
/// content paint effects).
fn unpainted_paint_layer(app: &TextureCraftApp) -> Option<String> {
    use texturecraft_engine::core::{EffectKind, LayerKind};
    let s = &app.session;
    let layer = s.selected_layer().and_then(|id| s.doc.project.content_layer(id))?;
    if !matches!(layer.kind, LayerKind::Paint) {
        return None;
    }
    let ids: Vec<_> = std::iter::once(layer.id).chain(layer.content_effects.iter().filter(|e| matches!(e.kind, EffectKind::Paint)).map(|e| e.id)).collect();
    let painted = s.doc.pixels.layers.iter().any(|((id, _), img)| ids.contains(id) && img.allocated_count() > 0);
    (!painted).then(|| layer.name.clone())
}

/// What a view accepts from the shelf.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DropZone {
    /// The brush properties: alphas.
    Brush,
    /// The 2D view: alphas and materials.
    View2d,
    /// The 3D view: alphas, materials (on the texture set they drop on) and textures (decals where they drop, on that set)
    View3d,
}

/// The texture set under the pointer while a shelf material or image is dragged over the 3D view `response`: the set a drop there goes to
pub fn drop_set(app: &TextureCraftApp, ui: &Ui, response: &egui::Response) -> Option<usize> {
    if app.ui.mode != crate::state::Mode::Painting {
        return None;
    }

    response.dnd_hover_payload::<ShelfDrag>().filter(|drag| drag.asset.goes_onto_set())?;
    set_under_pointer(app, ui, response)
}

/// The texture set of the surface under the pointer in the 3D view `response`
fn set_under_pointer(app: &TextureCraftApp, ui: &Ui, response: &egui::Response) -> Option<usize> {
    let local_position = ui.input(|input| input.pointer.interact_pos())? - response.rect.min;
    app.session.texture_set_at(Vec2::new(local_position.x, local_position.y), app.session.viewport)
}

/// Highlight accepted shelf assets on hover and apply them on drop. In the 3D view a material or image goes to the texture set under the pointer, highlighted by the view itself ([`drop_set`])
pub fn drop_target(app: &mut TextureCraftApp, ui: &Ui, response: &egui::Response, zone: DropZone) {
    if app.ui.mode != crate::state::Mode::Painting {
        return;
    }

    if zone == DropZone::View3d {
        DecalDrop::show_menu(app, ui, response);
    }

    let tokens = Tokens::get(ui.ctx());
    let accepts = |drag: &ShelfDrag| match drag.asset {
        Asset::Alpha(_) => true,
        Asset::Material(_) | Asset::Library(_) => zone != DropZone::Brush,
        Asset::Texture(_) => zone == DropZone::View3d,
        Asset::SmartMask(_) | Asset::Filter(_) => false,
    };
    let onto_set = |asset: &Asset| zone == DropZone::View3d && asset.goes_onto_set();
    if response.dnd_hover_payload::<ShelfDrag>().is_some_and(|drag| accepts(&drag) && !onto_set(&drag.asset)) {
        ui.painter().rect_stroke(response.rect.shrink(1.0), 3.0, Stroke::new(2.0, tokens.accent), egui::StrokeKind::Inside);
    }

    let pointer = ui.input(|input| input.pointer.interact_pos());
    let Some(drag) = dnd_release::<ShelfDrag>(response).filter(|drag| accepts(drag)) else { return };

    match (&drag.asset, pointer) {
        // The background takes nothing
        (Asset::Texture(name), Some(position)) => {
            let Some(texture_set) = set_under_pointer(app, ui, response) else { return };

            let drop = DecalDrop {
                image: name.clone(),
                texture_set,
                view_position: position - response.rect.min,
                view_size: response.rect.size(),
                camera: app.session.camera,
                screen_position: position,
            };
            drop.open(ui);
        }
        (asset, _) if onto_set(asset) => {
            let Some(ts) = set_under_pointer(app, ui, response) else { return };

            if app.run("textureSet.select", json!({"ts": ts})).is_some() {
                apply(app, asset);
            }
        }
        (asset, _) => apply(app, asset),
    }
}

/// The Assets panel body (a dock tab)
pub fn body(app: &mut TextureCraftApp, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let filter = app.ui.shelf_filter.to_lowercase();
    scan_brushes(app);
    let matching: Vec<ShelfEntry> = ShelfEntry::all(app).into_iter().filter(|entry| entry.matches(&filter)).collect();
    let counts: Vec<usize> = TABS.iter().map(|(tab, _, _)| matching.iter().filter(|entry| entry.tab() == *tab).count()).collect();
    search_field(app, ui);
    crate::shelf_tabs::row(app, ui, &counts, |_, _, _| {});
    ui.add_space(4.0);

    let tab = app.ui.shelf_tab;
    let shown_tab = TABS.get(tab).map(|(shown, _, _)| *shown);
    let mut entries: Vec<ShelfEntry> = matching.into_iter().filter(|entry| crate::shelf_tabs::shows(tab, entry.tab())).collect();
    entries.sort_by_cached_key(|entry| entry.label().to_lowercase());

    egui::ScrollArea::vertical().id_salt("shelf_scroll").auto_shrink([false, false]).show(ui, |ui| {
        if filter.is_empty() && shown_tab == Some(Tab::Textures) {
            import_images(app, ui, &ctx, entries.is_empty());
        }
        tiles(ui, |ui| {
            for entry in &entries {
                entry_tile(app, ui, &ctx, entry);
            }
        });

        if shown_tab.is_none_or(|shown| shown == Tab::Smart) {
            library(app, ui, &filter);
        }
    });
    drag_preview(&ctx);
}

/// The search field: a live filter on names (and categories) with a clear button while it has text
fn search_field(app: &mut TextureCraftApp, ui: &mut Ui) {
    let tk = Tokens::get(ui.ctx());
    ui.horizontal(|ui| {
        ui.label(RichText::new(ic::MAGNIFYING_GLASS).color(tk.text_dim));
        let clear_width = if app.ui.shelf_filter.is_empty() { 0.0 } else { ui.spacing().interact_size.y + ui.spacing().item_spacing.x };
        let field = egui::TextEdit::singleline(&mut app.ui.shelf_filter).hint_text(tr("Search")).desired_width(ui.available_width() - clear_width);
        ui.add(field);
        if !app.ui.shelf_filter.is_empty() && ui.add(egui::Button::new(ic::X).frame(false)).on_hover_text(tr("Clear")).clicked() {
            app.ui.shelf_filter.clear();
        }
    });
}

/// The dragged asset's name next to the pointer.
fn drag_preview(ctx: &egui::Context) {
    let tk = Tokens::get(ctx);
    let (Some(d), Some(pos)) = (egui::DragAndDrop::payload::<ShelfDrag>(ctx), ctx.input(|i| i.pointer.hover_pos())) else { return };
    let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::unique("shelf_drag")));
    let galley = p.layout_no_wrap(d.label.clone(), egui::FontId::proportional(11.0), tk.text_strong);
    let rect = egui::Rect::from_min_size(pos + vec2(14.0, 12.0), galley.size() + vec2(12.0, 6.0));
    p.rect_filled(rect, 3.0, tk.row_selected);
    p.rect_stroke(rect, 3.0, Stroke::new(1.0, tk.accent), egui::StrokeKind::Inside);
    p.galley(rect.min + vec2(6.0, 3.0), galley, tk.text_strong);
}

pub(crate) fn matches(filter: &str, texts: &[&str]) -> bool {
    filter.is_empty() || texts.iter().any(|t| t.to_lowercase().contains(filter))
}

pub(crate) fn header(ui: &mut Ui, text: &str) {
    let tk = Tokens::get(ui.ctx());
    ui.add_space(2.0);
    ui.label(RichText::new(text).size(11.0).color(tk.text_dim));
}

pub(crate) fn tiles(ui: &mut Ui, body: impl FnOnce(&mut Ui)) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(TILE_GAP, TILE_GAP);
        body(ui);
    });
    ui.add_space(6.0);
}

/// The distinct categories of `items`, in order of first appearance.
pub(crate) fn categories<T>(items: &[T], category: impl Fn(&T) -> &'static str) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for c in items.iter().map(category) {
        if !out.contains(&c) {
            out.push(c);
        }
    }
    out
}

/// The user's saved smart materials (desktop library folder).
#[cfg(not(target_arch = "wasm32"))]
fn library(app: &mut TextureCraftApp, ui: &mut Ui, filter: &str) {
    let tk = Tokens::get(ui.ctx());
    if app.library.is_none() {
        let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(crate::library_dir())
            .map(|d| d.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "tcmat" || x == MCMAT_EXT)).collect())
            .unwrap_or_default();
        files.sort();
        app.library = Some(files);
    }
    let files = app.library.clone().unwrap_or_default();
    let name = |f: &std::path::PathBuf| f.file_stem().and_then(|s| s.to_str()).unwrap_or("material").to_string();
    let shown: Vec<_> = files.iter().filter(|f| matches(filter, &[&name(f)])).collect();
    if !filter.is_empty() && shown.is_empty() {
        return;
    }
    ui.horizontal(|ui| {
        header(ui, tr("My smart materials"));
        if ui.small_button(ic::ARROW_CLOCKWISE).on_hover_text(tr("Rescan the library folder")).clicked() {
            app.library = None;
        }
    });
    if files.is_empty() {
        ui.label(RichText::new(tr("Right-click a layer > Create smart material")).size(10.0).color(tk.text_dim));
    }
    for f in shown {
        let (label, asset) = (name(f), Asset::Library(f.display().to_string()));
        // MaterialCraft materials (sent from MaterialCraft) show a graph.
        let glyph = if f.extension().is_some_and(|x| x == MCMAT_EXT) { ic::GRAPH } else { ic::SPHERE };
        let r = ui.add(egui::Button::selectable(false, format!("{glyph}  {label}")).sense(Sense::click_and_drag())).on_hover_ui(|ui| {
            ui.label(crate::i18n::trf(
                "{file}: add above the selected layer (or drag it into the layer stack or onto the viewport)",
                &[("file", &f.display())],
            ));
        });
        if r.drag_started() {
            r.dnd_set_drag_payload(ShelfDrag { asset: asset.clone(), label });
        }
        if r.clicked() {
            apply(app, &asset);
        }
    }
    ui.add_space(6.0);
}

#[cfg(target_arch = "wasm32")]
fn library(_: &mut TextureCraftApp, _: &mut Ui, _: &str) {}

/// Read the user's saved brush presets (desktop brushes folder) unless they are already listed
#[cfg(not(target_arch = "wasm32"))]
fn scan_brushes(app: &mut TextureCraftApp) {
    use texturecraft_engine::brush_preset_cmds::BRUSH_EXT;

    if app.brushes.is_some() {
        return;
    }

    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(crate::user_dir("brushes"))
        .map(|dir| dir.flatten().map(|entry| entry.path()).filter(|path| path.extension().is_some_and(|extension| extension == BRUSH_EXT)).collect())
        .unwrap_or_default();
    files.sort();
    app.brushes = Some(files);
}

#[cfg(target_arch = "wasm32")]
fn scan_brushes(_: &mut TextureCraftApp) {}

/// The Textures tab's import button, and a hint while the project has no images
fn import_images(app: &mut TextureCraftApp, ui: &mut Ui, ctx: &egui::Context, empty: bool) {
    let tk = Tokens::get(ctx);
    if ui.small_button(tr("Import...")).clicked() {
        crate::menus::invoke(app, ctx, "resource.import");
    }
    if empty {
        ui.label(RichText::new(tr("Import images to use them in fills and masks")).size(10.0).color(tk.text_dim));
    }
}

/// The tile of `entry`: a click applies it, an image resource has a context menu
fn entry_tile(app: &mut TextureCraftApp, ui: &mut Ui, ctx: &egui::Context, entry: &ShelfEntry) {
    // An image removed this frame has nothing to show
    let (Some(preview), texture) = (entry.preview(app), entry.thumbnail(app, ctx)) else { return };

    let asset = entry.asset();
    let response = tile(ui, &mut app.thumbs, texture.as_ref(), || preview, entry.label(), asset.as_ref());
    let selected_id = egui::Id::unique(SELECTED_TILE);
    let key = entry.key();
    let selected = ctx.data(|data| data.get_temp::<String>(selected_id)).is_some_and(|selected| selected == key);
    if selected || entry.in_use(app) {
        let tk = Tokens::get(ctx);
        let width = if selected { SELECTED_OUTLINE_WIDTH } else { IN_USE_OUTLINE_WIDTH };
        ui.painter().rect_stroke(response.rect, TILE_ROUNDING, Stroke::new(width, tk.accent), egui::StrokeKind::Inside);
    }

    if entry.includes_randomness() {
        randomness_badge(ui.painter(), response.rect);
    }

    if response.clicked() || response.secondary_clicked() {
        ctx.data_mut(|data| data.insert_temp(selected_id, key));
    }
    if response.clicked() {
        entry.activate(app);
    }

    if let ShelfEntry::Image(name) = entry {
        response.context_menu(|ui| {
            if ui.button(tr("Add as decal (view center)")).clicked() {
                app.run("layer.add.decal", json!({"image": name}));
                ui.close();
            }
            if ui.button(tr("Remove resource")).clicked() {
                app.run("resource.remove", json!({"name": name}));
                ui.close();
            }
        });
    }
}

/// Asset tile with optional dragging and a larger hover preview
pub(crate) fn tile(
    ui: &mut Ui,
    thumbs: &mut Thumbs,
    tex: Option<&egui::TextureHandle>,
    preview: impl FnOnce() -> ShelfPreview,
    label: &str,
    drag: Option<&Asset>,
) -> egui::Response {
    let tk = Tokens::get(ui.ctx());
    let sense = if drag.is_some() { Sense::click_and_drag() } else { Sense::click() };
    let (rect, resp) = ui.allocate_exact_size(vec2(TILE_WIDTH, TILE_HEIGHT), sense);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
    if let Some(asset) = drag
        && resp.drag_started()
    {
        resp.dnd_set_drag_payload(ShelfDrag { asset: asset.clone(), label: label.to_string() });
    }

    let painter = ui.painter();
    if resp.hovered() || resp.dragged() {
        painter.rect_filled(rect, TILE_ROUNDING, tk.row_hover);
    }
    let image_rect = thumbnail_rect(rect);
    thumbnail(painter, tex, image_rect, &tk);
    let galley = painter.layout_job(one_line(label, rect.width() - LABEL_MARGIN, tk.text));
    painter.galley(egui::pos2(rect.center().x - galley.size().x * 0.5, image_rect.bottom() + LABEL_GAP), galley, tk.text);

    let big = if resp.hovered() { big_preview(ui, thumbs, preview) } else { None };
    resp.on_hover_ui(|ui| hover_preview(ui, tex, big.as_ref(), label))
}

/// Where a tile's thumbnail goes in the tile's `rect`
fn thumbnail_rect(rect: egui::Rect) -> egui::Rect {
    egui::Rect::from_center_size(rect.center_top() + vec2(0.0, THUMBNAIL_CENTER_Y), vec2(THUMBNAIL_SIZE, THUMBNAIL_SIZE))
}

/// Three light, dark-rimmed dots at the thumbnail's bottom-right corner: the asset includes randomness
fn randomness_badge(painter: &egui::Painter, tile: egui::Rect) {
    let corner = thumbnail_rect(tile).right_bottom() - vec2(BADGE_INSET, BADGE_INSET);
    for offset in [vec2(0.0, 0.0), vec2(-BADGE_DOT_SPACING, 0.0), vec2(-BADGE_DOT_SPACING * 0.5, -BADGE_DOT_SPACING)] {
        painter.circle(corner + offset, BADGE_DOT_RADIUS, BADGE_DOT_FILL, Stroke::new(BADGE_DOT_RIM, BADGE_DOT_RIM_COLOR));
    }
}

/// A tile label on one line, cut with "…" past `width` points (the hover popup shows the whole name)
fn one_line(label: &str, width: f32, color: Color32) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::single_section(label.to_string(), egui::TextFormat::simple(egui::FontId::proportional(LABEL_FONT_SIZE), color));
    job.wrap = egui::text::TextWrapping::truncate_at_width(width);
    job
}

/// Start rendering during the popup delay once the pointer rests
fn big_preview(ui: &Ui, thumbs: &mut Thumbs, preview: impl FnOnce() -> ShelfPreview) -> Option<egui::TextureHandle> {
    let ctx = ui.ctx();
    let preview = preview();
    let requested_size_px = (BIG_PREVIEW_SIZE * ctx.pixels_per_point()).round() as usize;

    // High display scales must respect the orb camera's allocation limit
    let on_the_orb = matches!(&preview, ShelfPreview::SmartMask(_) | ShelfPreview::SmartMaterial(_));
    let size_px = if on_the_orb { requested_size_px.clamp(1, MAX_PREVIEW_SIZE_PX) } else { requested_size_px };

    // Scrolling moves tiles beneath a stationary pointer
    let idle_seconds = ui.input(|input| if input.is_scrolling() { 0.0 } else { input.pointer.time_since_last_movement().min(input.time_since_last_scroll()) });
    if idle_seconds >= BIG_PREVIEW_DWELL {
        return thumbs.big(ctx, preview, size_px);
    }

    ctx.request_repaint_after_secs(BIG_PREVIEW_DWELL - idle_seconds);
    thumbs.rendered_big(ctx, &preview, size_px)
}

/// `texture` drawn into `rect`, or a dim loading icon while it renders
fn thumbnail(painter: &egui::Painter, texture: Option<&egui::TextureHandle>, rect: egui::Rect, tk: &Tokens) {
    match texture {
        Some(texture) => {
            painter.image(texture.id(), rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
        }
        None => {
            let font = egui::FontId::proportional(rect.width() * LOADING_ICON_SCALE);
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                egui_phosphor::regular::ARROWS_CLOCKWISE,
                font,
                tk.text_dim.gamma_multiply(LOADING_ICON_OPACITY),
            );
        }
    }
}

/// Use the thumbnail as a placeholder until the hover preview arrives
fn hover_preview(ui: &mut Ui, small: Option<&egui::TextureHandle>, big: Option<&egui::TextureHandle>, label: &str) {
    let tk = Tokens::get(ui.ctx());
    ui.set_max_width(BIG_PREVIEW_SIZE);
    ui.vertical_centered(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(BIG_PREVIEW_SIZE, BIG_PREVIEW_SIZE), Sense::hover());
        thumbnail(ui.painter(), big.or(small), rect, &tk);
        ui.add_space(BIG_PREVIEW_LABEL_GAP);
        ui.label(label);
    });
}
