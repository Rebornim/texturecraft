//! A fill's Symmetry section: mirrored or radial copies of its 3D projection

use std::ops::RangeInclusive;

use egui::{Painter, Rect};
use serde_json::{Value, json};
use texturecraft_engine::core::{FillSymmetry, Projection, Sampling, SymmetryKind};

use crate::TextureCraftApp;
use crate::dock::with;
use crate::fill_pictures::ring_picture;
use crate::i18n::tr;
use crate::property_sheet::{PropertySheet, StackSlider};
use crate::theme::{AXIS_COLORS, Tokens};

/// The axes' names, as the Axis switch and the axis position's letters show them
const AXIS_NAMES: [&str; 3] = ["X", "Y", "Z"];

/// What the Axis position track spans, in mesh radii from the mesh's centre
const POSITION_RANGE: RangeInclusive<f32> = -FillSymmetry::REACH..=FillSymmetry::REACH;

/// What the Count track spans; a typed count may reach the engine's most
const COUNT_TRACK: RangeInclusive<f32> = 2.0..=16.0;

/// What the Angle span track spans, in degrees
const SPAN_RANGE: RangeInclusive<f32> = 1.0..=360.0;

/// The Symmetry section under the Fill section: its switch on the caption, then Type, Axis, the radial copies beside their picture, Flip copy and Axis position
pub(crate) fn symmetry_section(app: &mut TextureCraftApp, sheet: &mut PropertySheet<'_>, projection: &Projection, sampling: Sampling, base: &Value) {
    let symmetry = sampling.symmetry;
    let run = |app: &mut TextureCraftApp, fields: Value| app.run("layer.fill.projection", with(base, fields));
    if !projection.takes_symmetry() {
        let reason = match projection {
            Projection::UvTile => tr("Not available with Fill (Match per UV Tile)"),
            _ => tr("Not available with UV projection"),
        };
        sheet.section_unavailable(tr("Symmetry"), reason);
        return;
    }
    let switch = sheet.section_switch(tr("Symmetry"), symmetry.on);
    if switch.clicked() {
        run(app, json!({"symmetry": !symmetry.on}));
    }
    if !symmetry.on {
        return;
    }

    let (picked, _) = sheet.dropdown(tr("Type"), symmetry.kind, &SymmetryKind::ALL, |kind| tr(kind_label(kind)).to_owned(), None);
    if let Some(kind) = picked {
        run(app, json!({"symmetryType": kind.id()}));
    }
    let axis = usize::from(symmetry.axis.min(2));
    if let Some(picked) = sheet.switch_tinted(tr("Axis"), &AXIS_NAMES, axis, &AXIS_COLORS) {
        run(app, json!({"symmetryAxis": picked}));
    }
    if symmetry.kind == SymmetryKind::Radial {
        radial_rows(app, sheet, &symmetry, base);
    }

    let mut flip = symmetry.flip;
    if sheet.toggle_pair(tr("Flip copy"), ["U", "V"], &mut flip).is_some() {
        run(app, json!({"flipCopy": flip}));
    }

    // A mirror moves its plane along its axis; radial copies turn round an axis that moves across it
    let moving: Vec<usize> = match symmetry.kind {
        SymmetryKind::Mirror => vec![axis],
        SymmetryKind::Radial => (0..3).filter(|other| *other != axis).collect(),
    };
    let mut parts: Vec<(&str, egui::Color32, f32)> = moving.iter().map(|&a| (AXIS_NAMES[a], AXIS_COLORS[a], symmetry.position[a])).collect();
    if let Some((index, merge)) = sheet.lettered_sliders(tr("Axis position"), &mut parts, POSITION_RANGE, 2)
        && let (Some(&moved), Some((_, _, value))) = (moving.get(index), parts.get(index))
    {
        let mut position = symmetry.position;
        position[moved] = *value;
        run(app, json!({"axisPosition": position, "merge": merge}));
    }
}

/// Count and Angle span beside the ring of copies, seen along the axis
fn radial_rows(app: &mut TextureCraftApp, sheet: &mut PropertySheet<'_>, symmetry: &FillSymmetry, base: &Value) {
    let tokens = Tokens::get(sheet.ui.ctx());
    let axis_color = AXIS_COLORS[usize::from(symmetry.axis.min(2))];
    let (count, span) = (symmetry.copies(), symmetry.span);
    let picture = |painter: &Painter, rect: Rect| ring_picture(painter, rect, count, span, axis_color, &tokens);
    let (mut count_value, mut span_value) = (count as f32, span);
    let mut sliders = [
        StackSlider { label: tr("Count"), value: &mut count_value, range: COUNT_TRACK, decimals: 0, suffix: "" },
        StackSlider { label: tr("Angle span"), value: &mut span_value, range: SPAN_RANGE, decimals: 0, suffix: "°" },
    ];
    let fields = match sheet.picture_sliders(&picture, &mut sliders) {
        Some((0, merge)) => json!({"symmetryCount": count_value.round(), "merge": merge}),
        Some((_, merge)) => json!({"angleSpan": span_value, "merge": merge}),
        None => return,
    };
    app.run("layer.fill.projection", with(base, fields));
}

/// The Type dropdown's name for `kind`
fn kind_label(kind: SymmetryKind) -> &'static str {
    match kind {
        SymmetryKind::Mirror => "Mirror symmetry",
        SymmetryKind::Radial => "Radial symmetry",
    }
}
