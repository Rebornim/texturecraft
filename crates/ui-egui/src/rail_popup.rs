//! The rail's panels open in a popup beside their rail icon, one at a time; dragging its title docks or floats the panel

use egui::{Id, Pos2, Rect, RichText, Sense, Ui};
use egui_phosphor::regular as ic;

use crate::TextureCraftApp;
use crate::docking::Panel;
use crate::i18n::tr;
use crate::state::Mode;
use crate::theme::Tokens;
use crate::widgets::icon_button;

/// The rail's panels, top to bottom, with their icons and Window ▸ Views ids
pub(crate) const RAIL_PANELS: [(Panel, &str, &str); 5] = [
    (Panel::TextureSetSettings, ic::STACK, "ui.view.textureSetSettings"),
    (Panel::Display, ic::MONITOR, "ui.view.display"),
    (Panel::Shader, ic::SLIDERS_HORIZONTAL, "ui.view.shader"),
    (Panel::History, ic::CLOCK_COUNTER_CLOCKWISE, "ui.view.history"),
    (Panel::Log, ic::FILE_TEXT, "ui.view.log"),
];

/// The popup reaches this far into the viewport, so it covers the panels between the viewport and the rail
const VIEWPORT_OVERHANG: f32 = 48.0;

/// Narrowest the popup gets, when the viewport is not left of the rail's column
const POPUP_MIN_WIDTH: f32 = 320.0;

/// Settings panels' popups fit their contents up to this height, then scroll
const SETTINGS_MAX_HEIGHT: f32 = 640.0;

/// History and Log fill their popup, so it has one height
const LIST_HEIGHT: f32 = 400.0;

/// Gap between the popup and the rail, and from the window's bottom edge
const RAIL_GAP: f32 = 4.0;

/// Size of the close button in the popup's title row
const TITLE_BUTTON_SIZE: f32 = 22.0;

/// The popup's area, whose rect egui remembers between frames
fn area_id() -> Id {
    Id::unique("rail_popup")
}

/// Where the rail drew its strip and each icon on this frame
#[derive(Clone)]
struct RailRects {
    strip: Rect,
    icons: Vec<(Panel, Rect)>,
}

impl Default for RailRects {
    fn default() -> Self {
        RailRects { strip: Rect::NOTHING, icons: Vec::new() }
    }
}

fn rail_rects_id() -> Id {
    Id::unique("rail_rects")
}

/// A drag that left the popup's title: the panel now floats, and the dock takes the drag over once it has drawn it
#[derive(Clone, Copy)]
struct PendingDrag {
    panel: Panel,
    title: Rect,
}

fn pending_drag_id() -> Id {
    Id::unique("rail_popup_pending_drag")
}

fn viewport_id() -> Id {
    Id::unique("rail_popup_viewport")
}

/// Record where the dock drew the viewport, whose right edge the popup reaches past
pub(crate) fn remember_viewport(ctx: &egui::Context, viewport: Rect) {
    ctx.data_mut(|data| data.insert_temp(viewport_id(), viewport));
}

/// Whether `panel` opens from the rail rather than in the dock
pub(crate) fn is_rail_panel(panel: Panel) -> bool {
    RAIL_PANELS.iter().any(|(rail_panel, _, _)| *rail_panel == panel)
}

/// Record the rail's strip before its icons, so the popup can line up with them
pub(crate) fn begin_rail(ctx: &egui::Context, strip: Rect) {
    ctx.data_mut(|data| data.insert_temp(rail_rects_id(), RailRects { strip, icons: Vec::new() }));
}

/// Record where the rail drew `panel`'s icon
pub(crate) fn remember_icon(ctx: &egui::Context, panel: Panel, icon: Rect) {
    ctx.data_mut(|data| data.get_temp_mut_or_default::<RailRects>(rail_rects_id()).icons.push((panel, icon)));
}

/// A rail icon or its Window ▸ Views item: open `panel` in the popup in place of the one there, or close it
pub(crate) fn toggle(app: &mut TextureCraftApp, panel: Panel) {
    // Bake mode has no rail, and a panel in the dock (which then has no rail icon) is toggled there
    if app.ui.mode == Mode::Baking || app.ui.dock.is_open(panel) {
        app.ui.dock.toggle(panel);
        return;
    }

    app.ui.rail_panel = if app.ui.rail_panel == Some(panel) { None } else { Some(panel) };
}

/// Hand a drag that left the popup's title to the dock, once the dock has drawn the panel it floated
pub(crate) fn hand_drag_to_dock(ctx: &egui::Context, dock_area: Id) {
    let Some(pending) = ctx.data(|data| data.get_temp::<PendingDrag>(pending_drag_id())) else { return };

    ctx.data_mut(|data| data.remove::<PendingDrag>(pending_drag_id()));

    if ctx.input(|input| input.pointer.primary_down()) {
        craft_ui::docking::begin_drag(ctx, dock_area, pending.panel, pending.title);
    }
}

/// Draw the open popup beside its icon; a press outside it closes it, as for a menu (Escape does too, in `menus.rs`)
pub(crate) fn show(app: &mut TextureCraftApp, ctx: &egui::Context) {
    let Some(panel) = app.ui.rail_panel else { return };
    let rail = ctx.data(|data| data.get_temp::<RailRects>(rail_rects_id())).unwrap_or_default();
    let icon = rail.icons.iter().find(|(icon_panel, _)| *icon_panel == panel).map(|(_, rect)| *rect);
    let Some(icon) = icon.filter(|_| app.ui.mode != Mode::Baking && !app.ui.dock.is_open(panel)) else {
        app.ui.rail_panel = None;
        return;
    };

    let shown = ctx.memory(|memory| memory.area_rect(area_id()));
    if closed_by_input(ctx, shown, rail.strip) {
        app.ui.rail_panel = None;
        return;
    }

    // Level with the icon, raised when the popup's last size would run past the window's bottom
    let height = shown.map_or(0.0, |rect| rect.height());
    let bottom = ctx.content_rect().bottom() - RAIL_GAP;
    let top = icon.top().min(bottom - height).max(ctx.content_rect().top());
    let tokens = Tokens::get(ctx);
    let right = rail.strip.left() - RAIL_GAP;
    let viewport_right = ctx.data(|data| data.get_temp::<Rect>(viewport_id())).map(|viewport| viewport.right());
    let width = viewport_right.map_or(POPUP_MIN_WIDTH, |viewport_right| right - (viewport_right - VIEWPORT_OVERHANG)).max(POPUP_MIN_WIDTH);
    let frame = egui::Frame::popup(&ctx.global_style()).fill(tokens.panel).stroke(egui::Stroke::new(1.0, tokens.accent));
    let content_width = width - frame.total_margin().sum().x;

    let mut action = None;
    egui::Area::new(area_id()).order(egui::Order::Foreground).pivot(egui::Align2::RIGHT_TOP).fixed_pos(Pos2::new(right, top)).show(ctx, |ui| {
        frame.show(ui, |ui| {
            ui.set_width(content_width);
            action = title_row(ui, &tokens, panel);
            ui.separator();
            body(app, ui, panel);
        });
    });

    match action {
        Some(TitleAction::Close) => app.ui.rail_panel = None,
        Some(TitleAction::Drag(title)) => {
            app.ui.rail_panel = None;
            let rect = shown.unwrap_or(title);
            app.ui.dock.float_at(panel, [rect.left(), rect.top(), rect.width(), rect.height()]);
            ctx.data_mut(|data| data.insert_temp(pending_drag_id(), PendingDrag { panel, title }));
        }
        None => {}
    }
}

/// A press outside the popup and the rail, unless in one of the popup's own dropdowns
fn closed_by_input(ctx: &egui::Context, shown: Option<Rect>, strip: Rect) -> bool {
    let pressed_at = ctx.input(|input| input.pointer.press_origin().filter(|_| input.pointer.any_pressed()));
    // A press on the rail is a click on an icon, which switches or closes the popup itself
    let outside = pressed_at.is_some_and(|at| !strip.contains(at) && shown.is_some_and(|rect| !rect.contains(at)));
    outside && !egui::Popup::is_any_open(ctx)
}

/// The panel's contents: History and Log at one height, the settings panels as tall as their contents up to a limit
fn body(app: &mut TextureCraftApp, ui: &mut Ui, panel: Panel) {
    // A child Ui, so the height limits start below the title row instead of moving the cursor back up to it
    ui.scope(|ui| {
        if matches!(panel, Panel::History | Panel::Log) {
            ui.set_height(LIST_HEIGHT);
        } else {
            // The area's remembered size would otherwise cap the height
            ui.set_max_height(SETTINGS_MAX_HEIGHT);
        }
        crate::docking::panel_body(app, ui, panel);
    });
}

/// What the title row asked for
enum TitleAction {
    Close,
    /// The title was dragged from this rect
    Drag(Rect),
}

/// The panel's name, which drags the panel into the dock, then its close button
fn title_row(ui: &mut Ui, tokens: &Tokens, panel: Panel) -> Option<TitleAction> {
    let mut action = None;
    let mut close = Rect::NOTHING;
    let row = ui.horizontal(|ui| {
        ui.label(RichText::new(tr(panel.name())).strong().color(tokens.text_strong));
        ui.add_space((ui.available_width() - TITLE_BUTTON_SIZE).max(0.0));
        let response = icon_button(ui, ic::X, tr("Close"), false, TITLE_BUTTON_SIZE);
        close = response.rect;
        if response.clicked() {
            action = Some(TitleAction::Close);
        }
    });

    // The row left of the close button drags, as a tab does
    let title = Rect::from_min_max(row.response.rect.min, egui::pos2(close.left().min(row.response.rect.right()), row.response.rect.bottom()));
    let drag = ui.interact(title, ui.scope_id().with("rail_popup_title"), Sense::drag()).on_hover_cursor(egui::CursorIcon::Grab);
    if drag.drag_started() {
        action = Some(TitleAction::Drag(title));
    }
    action
}
