use egui::{Align, Layout, RichText, Sense, Stroke, Ui, UiBuilder, vec2};
use egui_phosphor::regular as icon;
use serde_json::json;
use texturecraft_engine::core::{BakeSettings, BakeSyncGroup, BakedMap};

use super::set_picker::SetPicker;
use super::set_picker_purpose::SetPickerPurpose;
use crate::TextureCraftApp;
use crate::docking::Panel;
use crate::i18n::{baked_map_label, tr, trf};
use crate::theme::Tokens;
use crate::widgets::{icon_button, icon_dropdown_button};

/// Height of one baker row, in points
const ROW_HEIGHT: f32 = 24.0;
/// Inset of a row's checkbox and name from its left edge, in points
const ROW_INSET: f32 = 6.0;
/// Side of a row's map thumbnail, in points
const THUMBNAIL_SIZE: f32 = 18.0;
/// Side of a row's icon buttons and of the list menu button, in points
const ROW_BUTTON_SIZE: f32 = 20.0;

/// The Common Settings button (with the sync icon when there are several texture sets), Check All with the list menu, and one row per map (checkbox enables, click selects)
pub(super) fn show(app: &mut TextureCraftApp, ui: &mut Ui, settings: &mut BakeSettings) {
    let several_sets = app.session.doc.project.texture_sets.len() > 1;
    ui.horizontal(|ui| {
        if ui.button(format!("{} {}", icon::GEAR, tr("Common Settings"))).clicked() {
            app.ui.dock.focus(Panel::CommonSettings);
        }
        if several_sets {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let shares = settings.sync.shares(BakeSyncGroup::Common);
                if icon_button(ui, icon::LINK, tr("Synchronize Common Settings"), shares, ROW_BUTTON_SIZE).clicked() {
                    SetPicker::open(app, ui.ctx(), SetPickerPurpose::Sync(BakeSyncGroup::Common));
                }
            });
        }
    });

    SetPicker::show(app, &ui.ctx().clone());

    ui.add_space(4.0);
    ui.horizontal(|ui| {
        check_all(ui, settings);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| list_menu(app, ui, settings));
    });
    ui.separator();

    let selected = super::selected_map(app.ui.selected_baked_map, settings);
    for map in BakedMap::ALL {
        if ui.push_id(map, |ui| row(app, ui, map, settings, map == selected)).inner {
            app.ui.selected_baked_map = Some(map);
            app.ui.dock.focus(Panel::MeshMapSettings);
        }
    }
}

/// Enable every map, or disable them all when every map is already enabled
fn check_all(ui: &mut Ui, settings: &mut BakeSettings) {
    let enabled_count = BakedMap::ALL.into_iter().filter(|map| settings.maps.is_enabled(*map)).count();
    let mut all_enabled = enabled_count == BakedMap::ALL.len();
    let some_enabled = enabled_count > 0 && !all_enabled;

    if ui.add(egui::Checkbox::new(&mut all_enabled, tr("Check All")).indeterminate(some_enabled)).changed() {
        for map in BakedMap::ALL {
            settings.maps.set_enabled(map, all_enabled);
        }
    }
}

/// The ≡ menu beside Check All: check states, and copying them or every setting to other texture sets
fn list_menu(app: &mut TextureCraftApp, ui: &mut Ui, settings: &mut BakeSettings) {
    let button = icon_dropdown_button(ui, icon::LIST, tr("Mesh Map Bakers Menu"), false, ROW_BUTTON_SIZE);
    egui::Popup::menu(&button).show(|ui| {
        if ui.button(tr("Check All Mesh Maps")).clicked() {
            BakedMap::ALL.into_iter().for_each(|map| settings.maps.set_enabled(map, true));
        }
        if ui.button(tr("Uncheck All Mesh Maps")).clicked() {
            BakedMap::ALL.into_iter().for_each(|map| settings.maps.set_enabled(map, false));
        }
        if ui.button(tr("Invert Checked Mesh Maps")).clicked() {
            BakedMap::ALL.into_iter().for_each(|map| settings.maps.set_enabled(map, !settings.maps.is_enabled(map)));
        }
        copy_items(app, ui, "checked", None, [tr("Apply Checked to More Texture Sets..."), tr("Apply Checked to All Texture Sets")]);
        ui.separator();
        copy_items(app, ui, "settings", None, [tr("Sync All Settings to More Texture Sets..."), tr("Sync All Settings to All Texture Sets")]);
    });
}

/// A copy's two menu items: "...more Texture Sets..." opens the set picker, "...all Texture Sets" copies to every set
fn copy_items(app: &mut TextureCraftApp, ui: &mut Ui, what: &'static str, map: Option<BakedMap>, [more, all]: [&str; 2]) {
    if ui.button(more).clicked() {
        SetPicker::open(app, ui.ctx(), SetPickerPurpose::Copy { what, map });
    }
    if ui.button(all).clicked() {
        let from = app.session.ts();
        app.run("bake.copy", json!({ "what": what, "map": map.map(BakedMap::id), "from": from, "ts": "all" }));
    }
}

/// One baker row; returns whether the row (not its checkbox or icons) was clicked
fn row(app: &mut TextureCraftApp, ui: &mut Ui, map: BakedMap, settings: &mut BakeSettings, is_selected: bool) -> bool {
    let several_sets = app.session.doc.project.texture_sets.len() > 1;
    let tokens = Tokens::get(ui.ctx());
    let name = tr(baked_map_label(map));
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_HEIGHT), Sense::click());
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, is_selected, name));

    let painter = ui.painter();
    if is_selected {
        painter.rect_filled(rect, tokens.cr(), tokens.row_selected);
        painter.rect_stroke(rect, tokens.cr(), Stroke::new(1.0, tokens.accent), egui::StrokeKind::Inside);
    } else if response.hovered() {
        painter.rect_filled(rect, tokens.cr(), tokens.row_hover);
    }
    painter.hline(rect.x_range(), rect.bottom(), Stroke::new(1.0, tokens.separator));

    // Added after the row's own response, so the checkbox and icons take the clicks over it
    let content_rect = rect.shrink2(vec2(ROW_INSET, 0.0));
    let mut content = ui.new_child(UiBuilder::new().max_rect(content_rect).layout(Layout::left_to_right(Align::Center)));
    let mut is_enabled = settings.maps.is_enabled(map);
    let checkbox = content.checkbox(&mut is_enabled, "");
    thumbnail(&mut content);
    let show_icons = is_selected || content.rect_contains_pointer(rect);
    content.with_layout(Layout::right_to_left(Align::Center), |ui| {
        let shares = settings.sync.shares(BakeSyncGroup::Map(map));
        if several_sets && show_icons && icon_button(ui, icon::LINK, &trf("Synchronize {map} Settings", &[("map", &name)]), shares, ROW_BUTTON_SIZE).clicked() {
            SetPicker::open(app, ui.ctx(), SetPickerPurpose::Sync(BakeSyncGroup::Map(map)));
        }
        if show_icons {
            let _ = icon_button(ui, icon::LIGHTNING, &trf("Bake {map} Only", &[("map", &name)]), false, ROW_BUTTON_SIZE);
            let _ = icon_button(ui, icon::EYE, &trf("Show {map} in the Viewport", &[("map", &name)]), false, ROW_BUTTON_SIZE);
        }
        let text = if is_selected { RichText::new(name).strong() } else { RichText::new(name) };
        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
            let label = ui.add(egui::Label::new(text).selectable(false).truncate());
            if checkbox.clone().labelled_by(label.id).changed() {
                settings.maps.set_enabled(map, is_enabled);
            }
        });
    });

    response.context_menu(|ui| {
        let labels = |more: &str, all: &str| [trf(more, &[("map", &name)]), trf(all, &[("map", &name)])];
        let [check_more, check_all] = labels("Check {map} for More Texture Sets...", "Check {map} for All Texture Sets");
        copy_items(app, ui, "check", Some(map), [&check_more, &check_all]);
        ui.separator();
        let [apply_more, apply_all] = labels("Apply {map} Settings to More Texture Sets...", "Apply {map} Settings to All Texture Sets");
        copy_items(app, ui, "settings", Some(map), [&apply_more, &apply_all]);
        ui.separator();
        if ui.add_enabled(several_sets, egui::Button::new(tr("Sync Settings..."))).clicked() {
            SetPicker::open(app, ui.ctx(), SetPickerPurpose::Sync(BakeSyncGroup::Map(map)));
        }
    });
    response.clicked()
}

/// The map's thumbnail; before it is baked, an empty square crossed by a diagonal
fn thumbnail(ui: &mut Ui) {
    let tokens = Tokens::get(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(vec2(THUMBNAIL_SIZE, THUMBNAIL_SIZE), Sense::hover());
    let stroke = Stroke::new(1.0, tokens.text_dim);
    ui.painter().rect_stroke(rect, 0.0, stroke, egui::StrokeKind::Inside);
    ui.painter().line_segment([rect.left_top(), rect.right_bottom()], stroke);
}
