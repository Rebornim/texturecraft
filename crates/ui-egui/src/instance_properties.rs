//! Properties of an instance: INSTANCE OF, the card of the layer it draws (a click goes to it), or a missing layer where the source is gone

use egui::epaint::RectShape;
use egui::{Color32, CornerRadius, Painter, Rect, Stroke, StrokeKind, TextureId, Ui, pos2};
use serde_json::json;
use texturecraft_engine::compose::ThumbTarget;
use texturecraft_engine::core::{Channel, Layer, LayerId, Project};

use crate::TextureCraftApp;
use crate::i18n::tr;
use crate::property_sheet::PropertySheet;
use crate::theme::Tokens;

/// Corner radius of the source's thumbnail
const THUMB_RADIUS: u8 = 3;

/// How far a missing source's outline sits inside the thumbnail's square
const MISSING_INSET: f32 = 1.0;

/// The properties of instance `layer`: where it draws from
pub(crate) fn instance_properties(app: &mut TextureCraftApp, ui: &mut Ui, layer: &Layer) {
    let Some(source) = layer.instance_source() else { return };
    let project = &app.session.doc.project;
    let missing = !project.layers.contains_key(&source);
    let name = project.layers.get(&source).map_or_else(|| tr("Missing layer").to_string(), |l| l.name.clone());

    // A missing source has no texture set left; the instance's own names where it was
    let set = set_name(project, if missing { layer.id } else { source });
    let picture =
        if missing { None } else { app.layer_panel.texture(ui.ctx(), &app.session, ThumbTarget::Content { layer: source, channel: Channel::BaseColor }) };
    let tokens = Tokens::get(ui.ctx());
    let thumbnail = |painter: &Painter, rect: Rect| paint_thumbnail(painter, rect, picture, missing, &tokens);

    let clicked = PropertySheet::show(ui, ("instance_sheet", layer.id.0), |sheet| {
        sheet.section(tr("Instance of"), false);
        sheet.source_card(&thumbnail, &name, &set, missing)
    });
    if clicked {
        app.run("layer.select", json!({"layer": source.0, "target": "content"}));
    }
}

/// The name of the texture set holding `layer`, empty when none does
fn set_name(project: &Project, layer: LayerId) -> String {
    project.locate(layer).and_then(|(ts, _, _)| project.texture_sets.get(ts)).map(|t| t.name.clone()).unwrap_or_default()
}

/// The source's thumbnail with rounded corners (the field colour until it is rendered), or an empty outlined square for a missing source
fn paint_thumbnail(painter: &Painter, rect: Rect, picture: Option<TextureId>, missing: bool, tokens: &Tokens) {
    let radius = CornerRadius::same(THUMB_RADIUS);
    if missing {
        painter.rect_stroke(rect.shrink(MISSING_INSET), radius, Stroke::new(1.0, tokens.text_dim), StrokeKind::Inside);
        return;
    }

    match picture {
        Some(texture) => {
            let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
            painter.add(RectShape::filled(rect, radius, Color32::WHITE).with_texture(texture, uv));
        }
        None => {
            painter.rect_filled(rect, radius, tokens.field);
        }
    }
}
