//! Symmetry: the toolbar toggle and settings popup, and what the 3D view draws for it

mod settings_popup;
mod symmetry_display;
mod symmetry_drag;
mod symmetry_intersection;
mod symmetry_manipulator;
mod symmetry_manipulator_arrow;
mod view_overlay;

use egui::Ui;
use egui_phosphor::regular as ic;
use texturecraft_engine::Tool;
use texturecraft_engine::paint::Symmetry;

pub use symmetry_display::SymmetryDisplay;
pub(crate) use view_overlay::{manipulator_hovered, manipulator_input, overlay, scene_overlay};

use crate::TextureCraftApp;
use crate::i18n::tr;
use crate::widgets::{icon_button, icon_dropdown_button};

/// Radial copies a brush gets when it switches to radial symmetry
const DEFAULT_RADIAL_COUNT: u8 = 6;

/// Whether `tool` paints symmetry copies
pub(crate) fn applies_to(tool: Tool) -> bool {
    matches!(tool, Tool::Paint | Tool::Eraser | Tool::Projection | Tool::Clone | Tool::Path)
}

/// Whether the 3D view shows symmetry now
pub(crate) fn shown(app: &TextureCraftApp) -> bool {
    app.ui.mode == crate::state::Mode::Painting && applies_to(app.session.tool.tool) && app.session.tool.brush.symmetry.any()
}

/// The axis the popup shows: the radial axis, or the first mirror axis
fn current_axis(symmetry: &Symmetry) -> usize {
    if symmetry.radial > 1 {
        return usize::from(symmetry.axis.min(2));
    }
    [symmetry.x, symmetry.y, symmetry.z].iter().position(|on| *on).unwrap_or(usize::from(symmetry.axis.min(2)))
}

/// The world axes the plane (mirror) or the axis (radial) can move along
pub(crate) fn movable_axes(symmetry: &Symmetry) -> [bool; 3] {
    if symmetry.radial_on() {
        let axis = usize::from(symmetry.axis.min(2));
        return [0, 1, 2].map(|k| k != axis);
    }
    [symmetry.x, symmetry.y, symmetry.z]
}

/// Mirror on one axis, or radial copies around it
fn set_mode(symmetry: &mut Symmetry, radial: bool, axis: usize) {
    symmetry.axis = axis as u8;
    if radial {
        (symmetry.x, symmetry.y, symmetry.z) = (false, false, false);
        if symmetry.radial <= 1 {
            symmetry.radial = DEFAULT_RADIAL_COUNT;
        }
    } else {
        (symmetry.x, symmetry.y, symmetry.z) = (axis == 0, axis == 1, axis == 2);
        symmetry.radial = 0;
    }
}

/// The toolbar's Symmetry toggle: turning it on with nothing set mirrors on X
fn toggle(symmetry: &mut Symmetry) {
    if symmetry.any() {
        symmetry.enabled = false;
        return;
    }

    symmetry.enabled = true;
    if !symmetry.x && !symmetry.y && !symmetry.z && symmetry.radial <= 1 {
        set_mode(symmetry, false, 0);
    }
}

/// The Symmetry toggle and the settings button with its popup
pub(crate) fn toolbar_controls(app: &mut TextureCraftApp, ui: &mut Ui) {
    let button_size = ui.spacing().interact_size.y;
    if icon_button(ui, ic::FLIP_HORIZONTAL, tr("Symmetry"), app.session.tool.brush.symmetry.any(), button_size).clicked() {
        toggle(&mut app.session.tool.brush.symmetry);
    }

    // No tooltip on the settings button; the label still names it for screen readers
    let popup_id = ui.make_persistent_id("symmetry_settings");
    let open = egui::Popup::is_id_open(ui.ctx(), popup_id);
    let response = icon_dropdown_button(ui, ic::GEAR_SIX, tr("Symmetry settings"), open, button_size);
    if response.clicked() {
        egui::Popup::toggle_id(ui.ctx(), popup_id);
    }
    egui::Popup::from_response(&response)
        .id(popup_id)
        .open_memory(None)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| settings_popup::show(app, ui));
}
