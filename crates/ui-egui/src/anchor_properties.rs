//! Properties of an anchor point: its name, typed in place, and the References band with what reads it (a click selects that)

use egui::Ui;
use egui_phosphor::regular as ic;
use serde_json::json;
use texturecraft_engine::core::{EffectKind, LayerId, LayerKind, Project};

use crate::TextureCraftApp;
use crate::i18n::{n_, tr};
use crate::property_sheet::PropertySheet;
use crate::reference::Reference;

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
                None if matches!(layer.kind, LayerKind::Fill(_)) => (n_("Fill"), ic::PAINT_BUCKET),
                None => return None,
                Some(effect) => {
                    let in_content = layer.content_effects.iter().any(|content_effect| content_effect.id == effect);
                    match project.effect(effect).map(|(_, found)| &found.kind) {
                        Some(EffectKind::Fill { .. } | EffectKind::Material(_)) if in_content => (n_("Fill"), ic::PAINT_BUCKET),
                        Some(EffectKind::Fill { .. }) => (n_("Mask fill"), ic::PAINT_BUCKET),
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
