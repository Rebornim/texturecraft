//! The layers under the pointer while R is held over the 3D view: a label names the top one live, and a click lists every layer there to pick from

use egui::{Color32, Key, Pos2, Rect, Sense, Stroke, Ui, pos2, vec2};
use serde_json::json;
use texturecraft_engine::compose::ThumbTarget;
use texturecraft_engine::core::{Channel, LayerId};
use texturecraft_engine::geom::glam::Vec2;
use texturecraft_engine::session::ViewMode;

use crate::TextureCraftApp;
use crate::i18n::{tr, trf};
use crate::state::Mode;
use crate::theme::Tokens;

/// The list's row height, thumbnail side and the room around a row's contents (points)
const ROW_HEIGHT: f32 = 36.0;
const THUMBNAIL_SIZE: f32 = 28.0;
const ROW_PADDING: f32 = 6.0;

/// The narrowest the list gets, so short names still give a comfortable click target (points)
const MIN_LIST_WIDTH: f32 = 160.0;

/// How far the list sits right of and below the click, clear of the pointer (points)
const LIST_OFFSET: f32 = 8.0;

/// Gap the list keeps from the view's edges
const VIEW_MARGIN: f32 = 4.0;

/// Room between the list's frame and its rows (points)
const LIST_MARGIN: i8 = 2;

/// Text sizes of a row's name and of the channels line under it
const NAME_FONT_SIZE: f32 = 12.0;
const CHANNELS_FONT_SIZE: f32 = 11.0;

/// Width of the accent edge around the focused row (points)
const FOCUS_EDGE_WIDTH: f32 = 1.0;

/// The list of layers laying something down where R + click landed
#[derive(Clone, Debug, PartialEq)]
pub struct LayerPickList {
    /// Where the click landed on screen, and in viewport pixels for `layer.pickAt`
    at: Pos2,
    point: Vec2,
    /// The view the list stays inside
    bounds: Rect,
    /// The layers there, top first, each with the channels it changes there
    rows: Vec<(LayerId, Vec<Channel>)>,
    /// The list's first frame: its top row takes the keyboard focus
    opening: bool,
    /// Closed by a press outside it: until every button is up, that press neither paints nor reopens it
    closing: bool,
}

/// The cached label for the texel under the pointer: (texture set, texel, document revision, view) and its text
type HoverLabel = ((usize, (u32, u32), u64, ViewMode), String);

/// R + click at `at` (`point` in viewport pixels) in the view `bounds`: one layer there is selected straight away, several open the list
pub(crate) fn click(app: &mut TextureCraftApp, at: Pos2, point: Vec2, bounds: Rect) {
    let viewport = app.session.viewport;
    match app.session.layers_under(point, viewport) {
        Ok((_, rows)) if rows.len() > 1 => app.layer_pick_list = Some(LayerPickList { at, point, bounds, rows, opening: true, closing: false }),
        _ => {
            app.run("layer.pickAt", json!({"x": point.x, "y": point.y, "viewport": [viewport.x, viewport.y]}));
        }
    }
}

/// Whether the list takes the views' clicks: open, or closing until the press that closed it ends
pub(crate) fn blocks_view_input(app: &TextureCraftApp) -> bool {
    app.layer_pick_list.is_some()
}

/// What the label at the pointer says over viewport pixel `point`: the top layer there and how many more, None off the mesh; recomputed only when the texel, document or view changes
pub(crate) fn hover_label(app: &TextureCraftApp, ui: &Ui, point: Vec2) -> Option<String> {
    let session = &app.session;
    let (ts, texel) = session.texel_under(point, session.viewport)?;
    let key = (ts, texel, session.revision, session.view);
    let id = egui::Id::unique("layer_pick_hover_label");
    if let Some((cached, label)) = ui.data(|data| data.get_temp::<HoverLabel>(id))
        && cached == key
    {
        return Some(label);
    }

    let layers = session.layers_at_texel(ts, texel);
    let name = |id: &LayerId| session.doc.project.layers.get(id).map(|layer| layer.name.clone()).unwrap_or_default();
    let label = match layers.as_slice() {
        [] => tr("No layer here").to_owned(),
        [(top, _)] => name(top),
        [(top, _), more @ ..] => trf("{name}  +{n} more", &[("name", &name(top)), ("n", &more.len())]),
    };
    ui.data_mut(|data| data.insert_temp::<HoverLabel>(id, (key, label.clone())));
    Some(label)
}

/// The list's area, whose rect egui remembers between frames
fn area_id() -> egui::Id {
    egui::Id::unique("layer_pick_list")
}

/// Draws the open list; a row click selects its layer, Esc or a press outside closes it
pub(crate) fn show(app: &mut TextureCraftApp, ctx: &egui::Context) {
    let (any_down, pressed_at, escape) =
        ctx.input(|input| (input.pointer.any_down(), input.pointer.press_origin().filter(|_| input.pointer.any_pressed()), input.key_pressed(Key::Escape)));
    let shown = ctx.memory(|memory| memory.area_rect(area_id()));
    match &mut app.layer_pick_list {
        Some(_) if app.ui.mode != Mode::Painting || escape => app.layer_pick_list = None,
        Some(list) if list.closing => {
            if !any_down {
                app.layer_pick_list = None;
            }
        }
        Some(list) if pressed_at.is_some_and(|at| shown.is_some_and(|rect| !rect.contains(at))) => list.closing = true,
        _ => {}
    }

    let Some(list) = app.layer_pick_list.clone().filter(|list| !list.closing) else { return };
    let tokens = Tokens::get(ctx);
    let mut picked = None;
    let width = list_width(app, ctx, &list.rows);

    // Right of and below the click; past the view's right edge it opens left of the click, so it never covers the spot
    let bounds = list.bounds.shrink(VIEW_MARGIN);
    let frame_width = width + 2.0 * f32::from(LIST_MARGIN);
    let left = if list.at.x + LIST_OFFSET + frame_width > bounds.right() { list.at.x - LIST_OFFSET - frame_width } else { list.at.x + LIST_OFFSET };
    let at = pos2(left, list.at.y + LIST_OFFSET);
    egui::Area::new(area_id()).order(egui::Order::Foreground).fixed_pos(at).constrain_to(bounds).show(ctx, |ui| {
        egui::Frame::popup(ui.style()).fill(tokens.panel).inner_margin(egui::Margin::same(LIST_MARGIN)).show(ui, |ui| {
            // A deep stack scrolls inside the view
            egui::ScrollArea::vertical().max_height(bounds.height() - 2.0 * f32::from(LIST_MARGIN)).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                // Up and Down move the keyboard focus along the rows (egui's arrow navigation), Enter picks the focused one
                for (index, (layer, channels)) in list.rows.iter().enumerate() {
                    let response = row(app, ui, *layer, channels, width);
                    if list.opening && index == 0 {
                        response.request_focus();
                    }
                    if response.clicked() {
                        picked = Some(*layer);
                    }
                }
            });
        });
    });

    if let Some(list) = &mut app.layer_pick_list {
        list.opening = false;
    }
    if let Some(layer) = picked {
        let viewport = app.session.viewport;
        app.run("layer.pickAt", json!({"x": list.point.x, "y": list.point.y, "viewport": [viewport.x, viewport.y], "layer": layer.0}));
        app.layer_pick_list = None;
    }
}

/// The rows' width: the widest name or channels line beside the thumbnail
fn list_width(app: &TextureCraftApp, ctx: &egui::Context, rows: &[(LayerId, Vec<Channel>)]) -> f32 {
    let text_width = |text: String, font: egui::FontId| ctx.fonts_mut(|fonts| fonts.layout_no_wrap(text, font, Color32::WHITE).size().x);
    let widest = rows
        .iter()
        .map(|(layer, channels)| {
            let name = text_width(layer_name(app, *layer), crate::theme::semibold(NAME_FONT_SIZE));
            name.max(text_width(channels_line(channels), egui::FontId::proportional(CHANNELS_FONT_SIZE)))
        })
        .fold(0.0, f32::max);
    (THUMBNAIL_SIZE + widest + ROW_PADDING * 3.0).max(MIN_LIST_WIDTH)
}

/// One row: the layer's content thumbnail, its name and the channels it changes at the spot
fn row(app: &mut TextureCraftApp, ui: &mut Ui, layer: LayerId, channels: &[Channel], width: f32) -> egui::Response {
    let tokens = Tokens::get(ui.ctx());
    let name = layer_name(app, layer);
    let channels = channels_line(channels);
    let (rect, _) = ui.allocate_exact_size(vec2(width, ROW_HEIGHT), Sense::hover());
    let response = ui.interact(rect, egui::Id::unique("layer_pick_row").with(layer.0), Sense::click());
    let selected = app.session.selected_layer() == Some(layer);
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::ListItem, true, selected, &name));

    // The pointed or focused row takes the selected layer's tint; focus adds an accent edge
    let painter = ui.painter();
    if response.hovered() || response.has_focus() {
        painter.rect_filled(rect, tokens.radius, crate::dock::selected_layer_fill(&tokens));
    }
    if response.has_focus() {
        painter.rect_stroke(rect, tokens.radius, Stroke::new(FOCUS_EDGE_WIDTH, tokens.accent), egui::StrokeKind::Inside);
    }

    let thumbnail = Rect::from_min_size(pos2(rect.left() + ROW_PADDING, rect.center().y - THUMBNAIL_SIZE * 0.5), vec2(THUMBNAIL_SIZE, THUMBNAIL_SIZE));
    painter.rect_filled(thumbnail, crate::dock::THUMBNAIL_ROUNDING, tokens.field);
    let target = ThumbTarget::Content { layer, channel: app.ui.layer_channel };
    if let Some(texture) = app.layer_panel.texture_outside(ui.ctx(), &app.session, target) {
        painter.image(texture, thumbnail, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
    }

    let text_left = thumbnail.right() + ROW_PADDING;
    painter.text(pos2(text_left, rect.center().y), egui::Align2::LEFT_BOTTOM, name, crate::theme::semibold(NAME_FONT_SIZE), tokens.text_strong);
    painter.text(pos2(text_left, rect.center().y), egui::Align2::LEFT_TOP, channels, egui::FontId::proportional(CHANNELS_FONT_SIZE), tokens.text_dim);
    response
}

fn layer_name(app: &TextureCraftApp, layer: LayerId) -> String {
    app.session.doc.project.layers.get(&layer).map(|layer| layer.name.clone()).unwrap_or_default()
}

/// The channels a layer changes at the spot, by their names
fn channels_line(channels: &[Channel]) -> String {
    channels.iter().map(|channel| tr(channel.label())).collect::<Vec<_>>().join(", ")
}
