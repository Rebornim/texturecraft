//! Properties of a color selection effect: its ID Mask slot with the output value, hardness, tolerance, background, and the ID colors it selects

use egui::{Color32, CornerRadius, Painter, Rect, Stroke, StrokeKind, Ui};
use egui_phosphor::regular as ic;
use serde_json::json;
use texturecraft_engine::core::{BakedMap, ColorSelection, LayerId, SelectionBackground};

use crate::TextureCraftApp;
use crate::drawn_icon::DrawnIcon;
use crate::i18n::tr;
use crate::id_color_pick::IdColorPick;
use crate::picker_group::PickerGroup;
use crate::picker_tab::PickerTab;
use crate::property_sheet::PropertySheet;
use crate::resource_picker;
use crate::resource_picker_item::ResourcePickerItem;
use crate::slot_thumbnail::{Thumbnail, paint_thumbnail};
use crate::theme::Tokens;
use crate::thumbs::Thumbs;
use crate::tile_picture::TilePicture;

/// Squares on each side of the transparent background's checker
const CHECKER_SQUARES: usize = 4;

/// The checker's two grays
const CHECKER_GRAYS: [u8; 2] = [150, 90];

/// The ID Mask slot, the output value, hardness, tolerance and background, then the Colors band: Pick color and one row per ID color. Every edit goes through `effect.set` or `effect.colorSelection.pick`
pub fn color_selection_properties(app: &mut TextureCraftApp, ui: &mut Ui, effect: LayerId, selection: &ColorSelection) {
    let id = effect.0;
    let set_index = app.session.doc.project.locate(effect_owner(app, effect)).map_or(app.session.ts(), |(set_index, _, _)| set_index);
    let mesh_map = app.session.baked(set_index).filter(|baked| baked.maps.contains_key(&BakedMap::Id)).cloned();
    let ctx = ui.ctx().clone();
    let (picture, line) = match &selection.id_map {
        Some(name) => {
            let picture =
                app.session.images.get(name).cloned().map_or(Thumbnail::Glyph(ic::IMAGE), |image| Thumbnail::Texture(app.thumbs.image(&ctx, name, &image)));
            (Some(picture), format!("{name} · {}", tr("texture")))
        }
        None => match mesh_map.as_ref().and_then(|baked| app.thumbs.id_mesh_map(&ctx, baked)) {
            Some(texture) => (Some(Thumbnail::Texture(texture)), format!("{} · {}", tr("ID"), tr("mesh map"))),
            None => (None, tr("None").to_owned()),
        },
    };
    let has_id_map = picture.is_some() && selection.id_map.as_ref().is_none_or(|name| app.session.images.contains_key(name));
    let picking = app.picking_id_color.filter(|pick| pick.effect == effect);
    let tokens = Tokens::get(ui.ctx());
    let (mut output, mut hardness, mut tolerance, background) = (selection.output, selection.hardness, selection.tolerance, selection.background);

    PropertySheet::show(ui, ("color_selection_sheet", id), |sheet| {
        sheet.section(tr("Color selection"), false);
        let thumbnail = |painter: &Painter, rect: Rect| match &picture {
            Some(picture) => paint_thumbnail(painter, rect, picture, &tokens),
            None => empty_slot(painter, rect, &tokens),
        };
        let slot = sheet.slot(&thumbnail, tr("ID Mask"), &line, None, selection.id_map.is_some(), false);
        let popup = sheet.ui.make_persistent_id(("id_map_picker", id));
        if slot.pick.clicked() {
            resource_picker::toggle(sheet.ui.ctx(), popup);
        }
        let current = selection.id_map.clone();
        let tabs = |thumbs: &mut Thumbs| texture_tabs(thumbs, &ctx, &app.session.images);
        match resource_picker::show(&slot.pick, popup, &mut app.thumbs, Some(&current), current.is_some(), tabs) {
            Some(None) => crate::menus::invoke(app, &ctx, "resource.import"),
            Some(Some(name)) if current.as_ref() != Some(&name) => {
                app.run("effect.set", json!({"effect": id, "idMap": name}));
            }
            Some(Some(_)) | None => {}
        }
        if slot.clear {
            app.run("effect.set", json!({"effect": id, "idMap": null}));
        }
        if let Some(name) = crate::dock::image_drop(sheet.ui, &slot.pick) {
            app.run("effect.set", json!({"effect": id, "idMap": name}));
        }

        sheet.caption(tr("Output value"));
        let ramp = sheet.ramp(tr("Output value"), &mut output, 0.0..=1.0);
        if ramp.invert {
            app.run("effect.set", json!({"effect": id, "output": 1.0 - output}));
        } else if let Some(merge) = ramp.changed {
            app.run("effect.set", json!({"effect": id, "output": output, "merge": merge}));
        }
        sheet.close_card();

        if let Some(merge) = sheet.slider(tr("Hardness"), &mut hardness, 0.0..=1.0, 2) {
            app.run("effect.set", json!({"effect": id, "hardness": hardness, "merge": merge}));
        }
        if let Some(merge) = sheet.slider(tr("Tolerance"), &mut tolerance, 0.0..=1.0, 2) {
            app.run("effect.set", json!({"effect": id, "tolerance": tolerance, "merge": merge}));
        }
        let swatch = |painter: &Painter, rect: Rect, background: SelectionBackground| background_swatch(painter, rect, background, &tokens);
        let name = |background: SelectionBackground| tr(background.label()).to_owned();
        if let (Some(picked), _) = sheet.dropdown(tr("Background Color"), background, &SelectionBackground::ALL, name, Some(&swatch)) {
            app.run("effect.set", json!({"effect": id, "background": picked.id()}));
        }

        let count = selection.colors.len().to_string();
        if !sheet.band(tr("Colors"), true, None, Some(&count)).open {
            return;
        }
        let adding = picking.is_some_and(|pick| pick.index.is_none());
        if sheet.button_enabled(DrawnIcon::Pipette, tr("Pick color"), adding, has_id_map).clicked() {
            toggle_pick(app, effect, None, adding);
        }
        for (index, color) in selection.colors.iter().enumerate() {
            let repicking = picking.is_some_and(|pick| pick.index == Some(index));
            let row = sheet.id_color_row(display_bytes(*color), repicking, has_id_map);
            if row.repick {
                toggle_pick(app, effect, Some(index), repicking);
            }
            if row.remove {
                // Removing a row changes the indices of pending replacements
                IdColorPick::end(app);
                let remaining: Vec<[f32; 3]> = selection.colors.iter().enumerate().filter(|(kept, _)| *kept != index).map(|(_, kept)| *kept).collect();
                app.run("effect.set", json!({"effect": id, "colors": remaining}));
            }
        }
    });
}

/// The layer whose mask holds `effect`
fn effect_owner(app: &TextureCraftApp, effect: LayerId) -> LayerId {
    app.session.doc.project.effect(effect).map_or(effect, |(owner, _)| owner)
}

fn toggle_pick(app: &mut TextureCraftApp, effect: LayerId, index: Option<usize>, running: bool) {
    if running {
        IdColorPick::end(app);
    } else {
        IdColorPick::start(app, effect, index);
    }
}

/// An ID color as the ID map view draws it: the stored value, unconverted, in 0–255
fn display_bytes(color: [f32; 3]) -> [u8; 3] {
    color.map(|value| (value.clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// The ID Mask picker: the project's textures, Import... first; the id is the texture's name, none for Import...
fn texture_tabs(thumbs: &mut Thumbs, ctx: &egui::Context, images: &texturecraft_engine::compose::ImageStore) -> Vec<PickerTab<Option<String>>> {
    let mut textures = vec![ResourcePickerItem { id: None, label: tr("Import...").to_owned(), caption: None, picture: TilePicture::Import }];
    let mut names: Vec<&String> = images.keys().filter(|name| texturecraft_engine::fill_alpha::library_alpha_of(name).is_none()).collect();
    names.sort();
    textures.extend(names.into_iter().filter_map(|name| images.get(name).map(|image| (name, image))).map(|(name, image)| ResourcePickerItem {
        id: Some(name.clone()),
        label: name.clone(),
        caption: None,
        picture: TilePicture::Texture(Some(thumbs.image(ctx, name, image))),
    }));
    vec![PickerTab { icon: ic::SQUARES_FOUR, name: tr("Resources"), groups: vec![PickerGroup { header: Some(tr("Textures")), items: textures }] }]
}

/// The slot's picture while there is no ID map: an empty outlined square
fn empty_slot(painter: &Painter, rect: Rect, tokens: &Tokens) {
    painter.rect_stroke(rect.shrink(1.0), CornerRadius::same(3), Stroke::new(1.0, tokens.text_dim), StrokeKind::Inside);
}

/// The swatch of a background choice: its gray, or a checker for transparent
fn background_swatch(painter: &Painter, rect: Rect, background: SelectionBackground, tokens: &Tokens) {
    match background.value() {
        Some(gray) => {
            painter.rect_filled(rect, CornerRadius::ZERO, Color32::from_gray((gray * 255.0).round() as u8));
        }
        None => {
            let square = rect.width() / CHECKER_SQUARES as f32;
            for row in 0..CHECKER_SQUARES {
                for column in 0..CHECKER_SQUARES {
                    let min = rect.min + egui::vec2(column as f32 * square, row as f32 * square);
                    let gray = CHECKER_GRAYS[(row + column) % 2];
                    painter.rect_filled(Rect::from_min_size(min, egui::vec2(square, square)), CornerRadius::ZERO, Color32::from_gray(gray));
                }
            }
        }
    }
    painter.rect_stroke(rect, CornerRadius::ZERO, Stroke::new(1.0, tokens.field_border), StrokeKind::Inside);
}
