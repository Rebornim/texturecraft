//! The rail's panels open attached to the rail, one at a time; undocking one makes it an ordinary floating panel

use egui::{Id, Margin, Order, Rect, RichText, Stroke, Ui};
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

/// Width of the panel attached to the rail
const FLYOUT_WIDTH: f32 = 320.0;

/// Size of the undock and close buttons in its title row
const TITLE_BUTTON_SIZE: f32 = 22.0;

/// Whether `panel` opens on the rail rather than in the dock
pub(crate) fn is_rail_panel(panel: Panel) -> bool {
    RAIL_PANELS.iter().any(|(rail_panel, _, _)| *rail_panel == panel)
}

/// A rail icon or its Window ▸ Views item: open `panel` on the rail in place of the one there, or close it
pub(crate) fn toggle(app: &mut TextureCraftApp, panel: Panel) {
    // Bake mode has no rail, and a panel in the dock (which then has no rail icon) is toggled there
    if app.ui.mode == Mode::Baking || app.ui.dock.is_open(panel) {
        app.ui.dock.toggle(panel);
        return;
    }

    app.ui.rail_panel = if app.ui.rail_panel == Some(panel) { None } else { Some(panel) };
}

/// Draw the rail's panel against the right edge of `area`, the dock area beside the rail
pub(crate) fn show(app: &mut TextureCraftApp, ctx: &egui::Context, area: Rect) {
    let Some(panel) = app.ui.rail_panel else { return };

    if app.ui.mode == Mode::Baking || !panel.available_in(app.ui.mode) || app.ui.dock.is_open(panel) {
        app.ui.rail_panel = None;
        return;
    }

    let tokens = Tokens::get(ctx);
    let rect = Rect::from_min_max(egui::pos2((area.right() - FLYOUT_WIDTH).max(area.left()), area.top()), area.max);
    let frame = egui::Frame::new().fill(tokens.panel).stroke(Stroke::new(1.0, tokens.accent)).inner_margin(Margin::same(1));
    let inner_size = rect.size() - frame.total_margin().sum();

    let mut action = None;
    egui::Area::new(Id::unique("rail_panel")).order(Order::Middle).fixed_pos(rect.min).show(ctx, |ui| {
        frame.show(ui, |ui| {
            ui.set_min_size(inner_size);
            ui.set_max_size(inner_size);
            action = title_row(ui, &tokens, panel);
            ui.separator();
            crate::docking::panel_body(app, ui, panel);
        });
    });

    match action {
        Some(TitleAction::Undock) => {
            app.ui.rail_panel = None;
            app.ui.dock.float_at(panel, [rect.left(), rect.top(), rect.width(), rect.height()]);
        }
        Some(TitleAction::Close) => app.ui.rail_panel = None,
        None => {}
    }
}

/// What the title row's buttons asked for
enum TitleAction {
    Undock,
    Close,
}

/// The panel's name, then its undock and close buttons at the right
fn title_row(ui: &mut Ui, tokens: &Tokens, panel: Panel) -> Option<TitleAction> {
    let mut action = None;
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.label(RichText::new(tr(panel.name())).strong().color(tokens.text_strong));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if icon_button(ui, ic::X, tr("Close"), false, TITLE_BUTTON_SIZE).clicked() {
                action = Some(TitleAction::Close);
            }
            if icon_button(ui, ic::ARROW_SQUARE_OUT, tr("Undock"), false, TITLE_BUTTON_SIZE).clicked() {
                action = Some(TitleAction::Undock);
            }
        });
    });
    action
}
