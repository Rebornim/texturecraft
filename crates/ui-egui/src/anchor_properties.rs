//! Properties of an anchor point: its name, typed in place, and the References band with what reads it (a click selects that)

use egui::Ui;
use egui_phosphor::regular as ic;
use serde_json::json;
use texturecraft_engine::core::{EffectKind, LayerId, LayerKind, Project};

use crate::TextureCraftApp;
use crate::i18n::tr;
use crate::property_sheet::PropertySheet;

/// One reader of an anchor point: the layer, its name, what reads (kind name and its icon) and the effect (None: the fill layer itself)
struct Reference {
    layer: LayerId,
    name: String,
    kind: &'static str,
    glyph: &'static str,
    effect: Option<LayerId>,
}

/// The properties of anchor point `anchor` named `name`; renaming goes through `effect.set`, a reference's click through `layer.select`
pub(crate) fn anchor_properties(app: &mut TextureCraftApp, ui: &mut Ui, anchor: LayerId, name: &str) {
    let references = references(&app.session.doc.project, anchor);
    PropertySheet::show(ui, ("anchor_sheet", anchor.0), |sheet| {
        sheet.section(tr("Anchor point"), false);
        if let Some(renamed) = sheet.name_field(tr("Anchor point name"), name) {
            app.run("effect.set", json!({"effect": anchor.0, "name": renamed}));
        }
        // The count says when nothing reads it
        let count = references.len().to_string();
        if !sheet.band(tr("References"), true, None, Some(&count)).open {
            return;
        }
        for reference in &references {
            if sheet.reference_row(reference.glyph, &reference.name, tr(reference.kind)) {
                let target = reference.effect.map_or_else(|| json!("content"), |effect| json!(effect.0));
                app.run("layer.select", json!({"layer": reference.layer.0, "target": target}));
            }
        }
    });
}

/// What reads `anchor`, bottom to top
fn references(project: &Project, anchor: LayerId) -> Vec<Reference> {
    project
        .anchor_readers(anchor)
        .into_iter()
        .filter_map(|(layer_id, effect)| {
            let layer = project.layers.get(&layer_id)?;
            let (kind, glyph) = match effect {
                None if matches!(layer.kind, LayerKind::Fill(_)) => ("Fill", ic::PAINT_BUCKET),
                None => return None,
                Some(effect) => {
                    let in_content = layer.content_effects.iter().any(|e| e.id == effect);
                    match project.effect(effect).map(|(_, found)| &found.kind) {
                        Some(EffectKind::Fill { .. } | EffectKind::Material(_)) if in_content => ("Fill", ic::PAINT_BUCKET),
                        Some(EffectKind::Fill { .. }) => ("Mask fill", ic::PAINT_BUCKET),
                        Some(other) => (other.name(), reader_glyph(other)),
                        None => return None,
                    }
                }
            };
            Some(Reference { layer: layer_id, name: layer.name.clone(), kind, glyph, effect })
        })
        .collect()
}

/// The icon of an effect that reads an anchor point, the one its Add effect item shows
fn reader_glyph(kind: &EffectKind) -> &'static str {
    match kind {
        EffectKind::Compare { .. } => ic::SCALES,
        EffectKind::Levels(_) => ic::CHART_BAR,
        EffectKind::Filter(_) => ic::FUNNEL,
        EffectKind::ColorSelection(_) => ic::SELECTION,
        EffectKind::Generator(_) => ic::SPARKLE,
        EffectKind::Paint | EffectKind::Path(_) => ic::PAINT_BRUSH,
        EffectKind::Anchor => ic::ANCHOR,
        EffectKind::Fill { .. } | EffectKind::Material(_) => ic::PAINT_BUCKET,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};
    use texturecraft_engine::core::LayerId;
    use texturecraft_engine::execute;

    use super::*;

    #[test]
    fn a_reference_shows_the_icon_of_what_reads_the_anchor() {
        let mut session = texturecraft_engine::sample_session(256, 64).unwrap();
        execute(&mut session, "layer.add.fill", &json!({"roughness": 0.5})).unwrap();
        let anchor = execute(&mut session, "effect.add.anchor", &Value::Null).unwrap()["effect"].as_u64().unwrap();
        execute(&mut session, "layer.add.fill", &Value::Null).unwrap();
        let filter = execute(&mut session, "effect.add.filter", &json!({"target": "content", "filter": "bevelSmooth"})).unwrap()["effect"].clone();
        execute(&mut session, "layer.fill.set", &json!({"effect": filter, "input": "distanceMap", "anchor": anchor})).unwrap();

        let glyphs: Vec<&str> = references(&session.doc.project, LayerId(anchor)).iter().map(|reference| reference.glyph).collect();
        assert_eq!(glyphs, [ic::FUNNEL], "a filter reading the anchor shows the filter icon, not a fill's");
    }
}
