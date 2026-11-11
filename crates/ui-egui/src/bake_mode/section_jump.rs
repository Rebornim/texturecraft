//! The icons above Common Settings that scroll to its sections, highlighting the one in view

use egui::{Context, Id, Ui};
use egui_phosphor::regular as icon;

use super::settings_section::SettingsSection;
use crate::i18n::tr;
use crate::widgets::icon_button;

/// Side of a jump icon, in points
const JUMP_BUTTON_SIZE: f32 = 26.0;

/// The section a jump icon asked for, until the panel scrolls to it
fn request_id() -> Id {
    Id::unique("bake_settings_jump_request")
}

/// The section whose header was last scrolled past
fn in_view_id() -> Id {
    Id::unique("bake_settings_in_view")
}

/// One icon per section; clicking one asks the panel to scroll to it
pub(super) fn bar(ui: &mut Ui) {
    let in_view = ui.data(|data| data.get_temp::<SettingsSection>(in_view_id())).unwrap_or(SettingsSection::Common);
    ui.horizontal(|ui| {
        for section in SettingsSection::ALL {
            let (glyph, tooltip) = match section {
                SettingsSection::Common => (icon::GEAR, tr("Common Settings")),
                SettingsSection::Cage => (icon::CUBE, tr("Cage")),
                SettingsSection::Skew => (icon::ARROWS_OUT_LINE_VERTICAL, tr("Skew Correction")),
            };
            if icon_button(ui, glyph, tooltip, section == in_view, JUMP_BUTTON_SIZE).clicked() {
                ui.data_mut(|data| {
                    data.insert_temp(request_id(), section);
                    data.insert_temp(in_view_id(), section);
                });
            }
        }
    });
}

/// The section a jump icon asked for this frame, if any
pub(super) fn take(ctx: &Context) -> Option<SettingsSection> {
    ctx.data_mut(|data| {
        let requested = data.get_temp::<SettingsSection>(request_id());
        data.remove::<SettingsSection>(request_id());
        requested
    })
}

/// Mark where `section` starts: scroll there when it was asked for, and once the user scrolls, note the last section whose header passed the top as in view
pub(super) fn anchor(ui: &mut Ui, section: SettingsSection, requested: Option<SettingsSection>) {
    if requested == Some(section) {
        ui.scroll_to_cursor(Some(egui::Align::TOP));
        return;
    }

    // The first header's position changes only when the panel scrolls; a jump keeps its icon lit until then
    let offset_id = Id::unique("bake_settings_scroll_offset");
    let top = ui.cursor().top() - ui.clip_rect().top();
    if section == SettingsSection::Common {
        let scrolled = ui.data(|data| data.get_temp::<f32>(offset_id)).is_some_and(|previous| previous != top);
        ui.data_mut(|data| {
            data.insert_temp(offset_id, top);
            data.insert_temp(scrolled_id(), scrolled);
        });
    }

    let scrolled = ui.data(|data| data.get_temp::<bool>(scrolled_id())).unwrap_or(false);
    if requested.is_none() && scrolled && (section == SettingsSection::Common || top <= JUMP_BUTTON_SIZE) {
        ui.data_mut(|data| data.insert_temp(in_view_id(), section));
    }
}

/// Whether the panel scrolled since the previous frame
fn scrolled_id() -> Id {
    Id::unique("bake_settings_scrolled")
}
