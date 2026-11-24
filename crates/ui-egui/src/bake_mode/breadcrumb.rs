use egui::{Align2, FontId, Rect, Sense, Stroke, Ui, WidgetInfo, pos2, vec2};
use egui_phosphor::regular as icon;
use serde_json::json;
use texturecraft_engine::core::BakedMap;

use crate::TextureCraftApp;
use crate::i18n::{baked_map_label, tr};
use crate::theme::{Tokens, semibold};

/// The breadcrumb's line and the air under its hairline
const LINE_HEIGHT: f32 = 32.0;
const AIR_BELOW: f32 = 10.0;

/// A caret's click square, and the air between a caret and its name
const CARET_BOX: f32 = 18.0;
const CARET_TO_NAME: f32 = 4.0;

/// Air between the set's name and the next caret
const NAME_TO_CARET: f32 = 8.0;

/// Size of the carets and of the reset glyph
const CARET_SIZE: f32 = 12.0;
const RESET_SIZE: f32 = 14.0;
const RESET_BOX: f32 = 20.0;

/// Size of the names
const NAME_SIZE: f32 = 12.0;

/// › texture set › map above Mesh Map Settings, each caret a menu to switch it, with ⟲ (Restore Defaults) at the right when the map has parameters; returns whether ⟲ was clicked
pub(super) fn show(app: &mut TextureCraftApp, ui: &mut Ui, selected: BakedMap, restorable: bool) -> bool {
    let tokens = Tokens::get(ui.ctx());
    let active_set = app.session.ts();
    let set_names: Vec<String> = app.session.doc.project.texture_sets.iter().map(|texture_set| texture_set.name.clone()).collect();
    let (line, _) = ui.allocate_exact_size(vec2(ui.available_width(), LINE_HEIGHT), Sense::hover());
    let center_y = line.center().y;
    let painter = ui.painter().clone();

    let mut x = line.left();
    let set_caret = caret(ui, &painter, &tokens, x, center_y, "texture_set_caret", tr("Switch Texture Set"));
    x += CARET_BOX + CARET_TO_NAME;
    let set_name = set_names.get(active_set).map_or("", String::as_str);
    let set_galley = painter.layout_no_wrap(set_name.to_owned(), FontId::proportional(NAME_SIZE), tokens.text);
    let set_width = set_galley.size().x;
    painter.galley(pos2(x, center_y - set_galley.size().y / 2.0), set_galley, tokens.text);
    x += set_width + NAME_TO_CARET;
    let map_caret = caret(ui, &painter, &tokens, x, center_y, "map_caret", tr("Switch Mesh Map"));
    x += CARET_BOX + CARET_TO_NAME;
    painter.text(pos2(x, center_y), Align2::LEFT_CENTER, tr(baked_map_label(selected)), semibold(NAME_SIZE), tokens.text_strong);

    egui::Popup::menu(&set_caret).show(|ui| {
        for (set_index, name) in set_names.iter().enumerate() {
            if ui.selectable_label(set_index == active_set, name).clicked() {
                app.run("textureSet.select", json!({ "ts": set_index }));
            }
        }
    });
    egui::Popup::menu(&map_caret).show(|ui| {
        for map in BakedMap::ALL {
            if ui.selectable_label(map == selected, tr(baked_map_label(map))).clicked() {
                app.ui.selected_baked_map = Some(map);
            }
        }
    });

    let mut restore = false;
    if restorable {
        let rect = Rect::from_center_size(pos2(line.right() - RESET_BOX / 2.0, center_y), vec2(RESET_BOX, RESET_BOX));
        let response = ui.interact(rect, ui.scope_id().with("restore_defaults"), Sense::click());
        response.widget_info(|| WidgetInfo::labeled(egui::Role::Button, true, tr("Restore Defaults")));
        let response = response.on_hover_text(tr("Restore Defaults"));
        let color = if response.hovered() { tokens.text_strong } else { tokens.text_dim };
        painter.text(rect.center(), Align2::CENTER_CENTER, icon::CLOCK_COUNTER_CLOCKWISE, FontId::proportional(RESET_SIZE), color);
        restore = response.clicked();
    }

    painter.line_segment([pos2(line.left(), line.bottom()), pos2(line.right(), line.bottom())], Stroke::new(1.0, tokens.separator));
    ui.add_space(AIR_BELOW);
    restore
}

/// A caret at `left` that opens a menu, answering as `label`
fn caret(ui: &mut Ui, painter: &egui::Painter, tokens: &Tokens, left: f32, center_y: f32, salt: &str, label: &str) -> egui::Response {
    let rect = Rect::from_min_size(pos2(left, center_y - CARET_BOX / 2.0), vec2(CARET_BOX, CARET_BOX));
    let response = ui.interact(rect, ui.scope_id().with(salt), Sense::click());
    response.widget_info(|| WidgetInfo::labeled(egui::Role::Button, true, label));
    let color = if response.hovered() { tokens.text_strong } else { tokens.text_dim };
    painter.text(rect.center(), Align2::CENTER_CENTER, icon::CARET_RIGHT, FontId::proportional(CARET_SIZE), color);
    response
}
