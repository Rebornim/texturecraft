//! Properties of an anchor point: its name, typed in place, and the References band with what reads it (a click selects that)

use egui::Ui;
use egui_phosphor::regular as ic;
use serde_json::json;
use texturecraft_engine::core::{EffectKind, LayerId, LayerKind, Project};

use crate::TextureCraftApp;
use crate::i18n::tr;
use crate::property_sheet::PropertySheet;

/// One reader of an anchor point: the layer, its name, what reads (kind name) and the effect (None: the fill layer itself)
struct Reference {
    layer: LayerId,
    name: String,
    kind: &'static str,
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
            if sheet.reference_row(ic::PAINT_BUCKET, &reference.name, &tr(reference.kind)) {
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
            let kind = match effect {
                None if matches!(layer.kind, LayerKind::Fill(_)) => "Fill",
                None => return None,
                Some(effect) if layer.content_effects.iter().any(|e| e.id == effect) => "Fill",
                Some(effect) => match project.effect(effect).map(|(_, found)| &found.kind) {
                    Some(EffectKind::Fill { .. }) => "Mask fill",
                    Some(other) => other.name(),
                    None => return None,
                },
            };
            Some(Reference { layer: layer_id, name: layer.name.clone(), kind, effect })
        })
        .collect()
}
