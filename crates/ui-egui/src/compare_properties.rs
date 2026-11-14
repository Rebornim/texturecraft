//! Properties of a compare mask: its channel, the two sources with the operator between them, the constant (a side reads it), the tolerance
//! (within tolerance), the hardness, and the two sources' histograms in their colours

use std::sync::Arc;

use egui::{Align2, Color32, FontId, Mesh, Rect, Sense, Shape, Stroke, Ui, pos2, vec2};
use serde_json::json;
use texturecraft_engine::core::{Channel, CompareOp, CompareSource, Layer, LayerId};

use crate::TextureCraftApp;
use crate::i18n::tr;
use crate::property_sheet::PropertySheet;
use crate::theme::{AXIS_COLORS, Tokens};

/// Buckets of each source's histogram, over 0..1
const HISTOGRAM_BUCKETS: usize = 64;

/// Height of the histogram well
const HISTOGRAM_HEIGHT: f32 = 70.0;

/// Air the tallest bucket keeps under the well's top
const HISTOGRAM_HEADROOM: f32 = 4.0;

/// From the well's bottom to its value labels' centres, and the room they take
const AXIS_LABEL_OFFSET: f32 = 10.0;
const AXIS_LABEL_ROOM: f32 = 22.0;

/// Size of the value labels under the well
const AXIS_LABEL_SIZE: f32 = 10.0;

/// How much of a source's colour shows over the well (the other source shows through)
const HISTOGRAM_TINT: f32 = 0.5;

/// Width of the constant's line in the well
const CONSTANT_LINE_WIDTH: f32 = 2.0;

/// The colours that tag the left and the right source
const SOURCE_TAGS: [Color32; 2] = [AXIS_COLORS[1], AXIS_COLORS[2]];

/// The operator switch's words, in [`CompareOp::ALL`] order
const OPERATOR_WORDS: [&str; 3] = ["<", "≈", ">"];

/// What compare mask `effect_id` holds
#[derive(Clone, Copy)]
pub(crate) struct CompareSettings {
    pub(crate) op: CompareOp,
    pub(crate) constant: f32,
    pub(crate) tolerance: f32,
    pub(crate) left: CompareSource,
    pub(crate) right: CompareSource,
    pub(crate) channel: Channel,
    pub(crate) hardness: f32,
}

/// The properties of compare mask `effect_id` on `layer`; every edit goes through `effect.set`
pub(crate) fn compare_properties(app: &mut TextureCraftApp, ui: &mut Ui, layer: &Layer, effect_id: LayerId, settings: CompareSettings) {
    let project = &app.session.doc.project;
    let channels = project.locate(layer.id).and_then(|(set, _, _)| project.texture_set(set).ok()).map(|set| set.channel_list()).unwrap_or_default();
    let histograms = histogram_counts(app, ui, effect_id);
    let id = effect_id.0;
    let CompareSettings { op, mut constant, mut tolerance, left, right, channel, mut hardness } = settings;

    PropertySheet::show(ui, ("compare_sheet", id), |sheet| {
        sheet.section(tr("Compare mask"), false);
        if let (Some(picked), _) = sheet.dropdown(tr("Channel"), channel, &channels, |option| tr(option.label()).to_string(), None) {
            app.run("effect.set", json!({"effect": id, "compareChannel": picked.id()}));
        }

        let operator = CompareOp::ALL.iter().position(|candidate| *candidate == op).unwrap_or_default();
        let picked = sheet.compare_rows((left, right), &CompareSource::ALL, source_name, SOURCE_TAGS, (&OPERATOR_WORDS, operator));
        if let Some(source) = picked.left {
            app.run("effect.set", json!({"effect": id, "left": source.id()}));
        }
        if let Some(source) = picked.right {
            app.run("effect.set", json!({"effect": id, "right": source.id()}));
        }
        if let Some(chosen) = picked.operator.and_then(|index| CompareOp::ALL.get(index)) {
            app.run("effect.set", json!({"effect": id, "op": chosen.id()}));
        }

        if (left == CompareSource::Constant || right == CompareSource::Constant)
            && let Some(merge) = sheet.slider(tr("Constant"), &mut constant, 0.0..=1.0, 2)
        {
            app.run("effect.set", json!({"effect": id, "value": constant, "merge": merge}));
        }
        if op == CompareOp::WithinTolerance
            && let Some(merge) = sheet.slider(tr("Tolerance"), &mut tolerance, 0.0..=1.0, 2)
        {
            app.run("effect.set", json!({"effect": id, "tolerance": tolerance, "merge": merge}));
        }
        if let Some(merge) = sheet.slider(tr("Hardness"), &mut hardness, 0.0..=1.0, 2) {
            app.run("effect.set", json!({"effect": id, "hardness": hardness, "merge": merge}));
        }

        if sheet.band(tr("Source histograms"), true, None, None).open {
            // A constant has no spread: it shows as a line at its value in its side's colour
            let constants = [left, right].map(|side| (side == CompareSource::Constant).then_some(constant));
            sheet.embed(|ui| source_histograms(ui, histograms.as_deref(), constants));
        }
    });
}

/// A source's name in its list
fn source_name(source: CompareSource) -> String {
    tr(match source {
        CompareSource::LayersBelow => "Layers below",
        CompareSource::ThisLayer => "This layer",
        CompareSource::ThisMask => "This mask",
        CompareSource::Constant => "Constant",
    })
    .to_string()
}

/// The well with the two sources' histograms (`counts`, left then right; a side reading a constant has `constants` set instead), quarter lines and value labels
fn source_histograms(ui: &mut Ui, counts: Option<&[Vec<u32>; 2]>, constants: [Option<f32>; 2]) {
    let tokens = Tokens::get(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), HISTOGRAM_HEIGHT + AXIS_LABEL_ROOM), Sense::hover());
    let well = Rect::from_min_size(rect.min, vec2(rect.width(), HISTOGRAM_HEIGHT));
    let painter = ui.painter_at(rect.expand2(vec2(AXIS_LABEL_SIZE, 0.0)));
    painter.rect_filled(well, 0.0, tokens.field);
    let at = |share: f32| well.left() + well.width() * share;
    for quarter in [0.25, 0.5, 0.75] {
        painter.line_segment([pos2(at(quarter), well.top()), pos2(at(quarter), well.bottom())], Stroke::new(1.0, tokens.separator));
    }

    // The right source first, the left over it, each mixed half with what is under it; a constant side draws no spread
    let shown = |side: usize| counts.and_then(|both| both.get(side)).filter(|bins| !bins.is_empty() && constants.get(side).copied().flatten().is_none());
    let tallest = [0, 1].into_iter().filter_map(shown).flatten().copied().max().unwrap_or(0).max(1) as f32;
    for side in [1, 0] {
        let (Some(tag), Some(bins)) = (SOURCE_TAGS.get(side), shown(side)) else { continue };

        // One mesh from bucket centre to bucket centre (separate strips would leave seams between them)
        let step = well.width() / bins.len() as f32;
        let tops = bins.iter().enumerate().map(|(index, count)| {
            (well.left() + (index as f32 + 0.5) * step, well.bottom() - (*count as f32 / tallest) * (HISTOGRAM_HEIGHT - HISTOGRAM_HEADROOM))
        });
        let outline: Vec<(f32, f32)> =
            std::iter::once((well.left(), well.bottom())).chain(tops).chain(std::iter::once((well.right(), well.bottom()))).collect();
        let mut mesh = Mesh::default();
        let tint = tag.gamma_multiply(HISTOGRAM_TINT);
        for (index, (x, top)) in outline.iter().enumerate() {
            mesh.colored_vertex(pos2(*x, *top), tint);
            mesh.colored_vertex(pos2(*x, well.bottom()), tint);
            if let Ok(next) = u32::try_from(index * 2)
                && index > 0
            {
                mesh.add_triangle(next - 2, next - 1, next);
                mesh.add_triangle(next - 1, next + 1, next);
            }
        }
        painter.add(Shape::mesh(mesh));
    }
    for (side, constant) in constants.iter().enumerate() {
        if let (Some(value), Some(tag)) = (constant, SOURCE_TAGS.get(side)) {
            let x = at(value.clamp(0.0, 1.0));
            painter.line_segment([pos2(x, well.top() + 2.0), pos2(x, well.bottom())], Stroke::new(CONSTANT_LINE_WIDTH, *tag));
        }
    }

    let labels = ["0", "0.25", "0.5", "0.75", "1"];
    for (index, label) in labels.iter().enumerate() {
        let x = at(index as f32 / (labels.len() - 1) as f32);
        painter.text(pos2(x, well.bottom() + AXIS_LABEL_OFFSET), Align2::CENTER_CENTER, *label, FontId::proportional(AXIS_LABEL_SIZE), tokens.text_dim);
    }
}

/// What the two sides of compare mask `effect_id` read, recomputed after a change once no pointer button is held
fn histogram_counts(app: &TextureCraftApp, ui: &Ui, effect_id: LayerId) -> Option<Arc<[Vec<u32>; 2]>> {
    let key = egui::Id::unique(("compare_histograms", effect_id.0));
    let revision = app.session.revision;
    match ui.data(|data| data.get_temp::<(u64, Arc<[Vec<u32>; 2]>)>(key)) {
        Some((cached, counts)) if cached == revision || ui.input(|input| input.pointer.any_down()) => Some(counts),
        _ => app.session.compare_histograms(effect_id, HISTOGRAM_BUCKETS).map(|counts| {
            let counts = Arc::new(counts);
            ui.data_mut(|data| data.insert_temp(key, (revision, counts.clone())));
            counts
        }),
    }
}
