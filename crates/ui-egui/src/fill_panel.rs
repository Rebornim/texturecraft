//! A fill's Properties: its projection, size, placement and edges, then its material

use std::ops::RangeInclusive;

use egui::{Margin, Painter, Rect};
use egui_phosphor::regular as ic;
use serde_json::{Value, json};
use texturecraft_engine::compose::tile_length::{MeshSize, tile_length, tiling_for_length};
use texturecraft_engine::core::{Channel, Fill, Filtering, LayerId, Projection, ScaleMode, UvWrap};

use crate::TextureCraftApp;
use crate::dock::{anchors_below, decal_editor, fill_channel, with};
use crate::fill_pictures::{filtering_picture, projection_picture, window_preview, wrap_picture};
use crate::fill_placement::Placement;
use crate::i18n::{filtering_label, tr, wrap_label};
use crate::projection_kind::ProjectionKind;
use crate::property_sheet::{NumberField, PAD, PreviewLower, PreviewSize, PropertySheet};
use crate::theme::{AXIS_COLORS, Tokens};

/// Centimetres in one mesh unit: meshes are read in metres
const CM_PER_UNIT: f32 = 100.0;

/// What the Tiling track spans, and what a typed tiling may be (the command's own limits)
const TILING_TRACK: RangeInclusive<f32> = 0.01..=5.0;
const TILING_TYPED: RangeInclusive<f32> = 0.001..=1000.0;

/// What the Size (cm) track spans, and what a typed size may be
const SIZE_CM_TRACK: RangeInclusive<f32> = 1.0..=500.0;
const SIZE_CM_TYPED: RangeInclusive<f32> = 0.1..=100_000.0;

/// What an Offset track spans (tiles, centred on zero), and what a typed offset may be
const OFFSET_TRACK: RangeInclusive<f32> = -2.0..=2.0;
const OFFSET_TYPED: RangeInclusive<f32> = -1000.0..=1000.0;

/// Range of a fill's rotation, a full turn
const ROTATION_RANGE: RangeInclusive<f32> = 0.0..=360.0;

/// Tri-planar hardness, the exponent weighting the three projections; `layer.fill.projection` clamps to the same range
const TRIPLANAR_HARDNESS_RANGE: RangeInclusive<f32> = 1.0..=64.0;

/// Range of a 3D projection's scale: how many times its pattern fits across the mesh
const WORLD_SCALE_RANGE: RangeInclusive<f32> = 0.01..=20.0;

/// Change per pixel dragged on a 3D offset field
const OFFSET_FIELD_SPEED: f32 = 0.01;

/// The tabs over a fill's Properties: each jumps to its section
pub(crate) fn fill_tabs() -> [(&'static str, &'static str); 2] {
    [(ic::SLIDERS_HORIZONTAL, tr("Fill")), (ic::CIRCLE_HALF, tr("Material"))]
}

/// The mesh's size for the texture set being painted, as the projections see it
fn mesh_size(app: &TextureCraftApp) -> MeshSize {
    let mesh = &app.session.mesh;
    let material = app.session.doc.project.texture_set(app.session.ts()).map_or(0, |texture_set| texture_set.material);
    MeshSize { radius: mesh.radius(), uv_density: mesh.uv_density(material).unwrap_or(1.0) }
}

/// A fill's whole Properties: the Fill section, then the Material section with one slot per channel that is on
pub(crate) fn fill_sheet(app: &mut TextureCraftApp, sheet: &mut PropertySheet<'_>, fill: &Fill, base: &Value, on: &[Channel], reader: LayerId) {
    let salt = base.to_string();
    sheet.section(tr("Fill"), false);
    match &fill.projection {
        Projection::Planar { placed: Some(decal), .. } => legacy(sheet, |ui| decal_editor(app, ui, decal, reader)),
        _ => projection_rows(app, sheet, fill, base, &salt),
    }

    sheet.section(tr("Material"), false);
    let channels = app.session.doc.project.texture_set(app.session.ts()).map(|texture_set| texture_set.channel_list()).unwrap_or_default();
    let names: Vec<&str> = channels.iter().map(|channel| tr(channel.short())).collect();
    let enabled: Vec<bool> = channels.iter().map(|channel| on.contains(channel)).collect();
    if let Some((index, solo)) = sheet.channel_pills(&names, &enabled)
        && let Some(channel) = channels.get(index)
    {
        let enable = solo || !enabled[index];
        app.run("layer.fill.set", with(base, json!({"channel": channel.id(), "enabled": enable, "solo": solo})));
    }

    let anchors = anchors_below(app, reader);
    legacy(sheet, |ui| {
        for channel in channels {
            if on.contains(&channel) {
                fill_channel(app, ui, fill, base, channel, &anchors, &salt);
            }
        }
    });
}

/// Controls not yet drawn with the sheet's primitives, at the panel's padding
fn legacy(sheet: &mut PropertySheet<'_>, body: impl FnOnce(&mut egui::Ui)) {
    sheet.close_card();
    egui::Frame::new().inner_margin(Margin::symmetric(PAD as i8, 4)).show(sheet.ui, |ui| {
        ui.spacing_mut().item_spacing = egui::vec2(8.0, 4.0);
        body(ui);
    });
}

/// Projection, Scale, the preview with size and offset, Rotation, then the Filtering and edges band
fn projection_rows(app: &mut TextureCraftApp, sheet: &mut PropertySheet<'_>, fill: &Fill, base: &Value, salt: &str) {
    let tokens = Tokens::get(sheet.ui.ctx());
    let Some(kind) = ProjectionKind::of(&fill.projection) else { return };
    let run = |app: &mut TextureCraftApp, fields: Value| {
        app.run("layer.fill.projection", with(base, fields));
    };

    let picture = |painter: &Painter, rect: Rect, kind: ProjectionKind| projection_picture(painter, rect, kind, &tokens);
    let (picked, _) = sheet.dropdown(tr("Projection"), kind, &ProjectionKind::ALL, |kind| tr(kind.label()).to_owned(), Some(&picture));
    if let Some(kind) = picked {
        run(app, json!({"projection": kind.id()}));
    }
    // Per UV tile, the source fills each tile once: nothing to place
    if kind == ProjectionKind::UvTile {
        return;
    }

    let size = mesh_size(app);
    let length = tile_length(&fill.projection, size);
    let physical = length.is_some() && fill.sampling.scale == ScaleMode::PhysicalSize;
    if length.is_some() {
        let modes = [tr("Tiling"), tr("Physical size")];
        if let Some(index) = sheet.switch(tr("Scale"), &modes, usize::from(physical))
            && let Some(mode) = ScaleMode::ALL.get(index)
        {
            run(app, json!({"scaleMode": mode.id()}));
        }
    }

    placement_rows(app, sheet, fill, base, salt, length.filter(|_| physical), size);

    if let Projection::Planar { axis, .. } | Projection::Cylindrical { axis, .. } = &fill.projection {
        let (picked, _) = sheet.dropdown(tr("Axis"), *axis, &[0, 1, 2], |axis| ["X", "Y", "Z"][usize::from(axis.min(2))].to_owned(), None);
        if let Some(axis) = picked {
            run(app, json!({"axis": axis}));
        }
    }

    if sheet.band(tr("Filtering and edges"), true, None, None).open {
        let filtering = fill.sampling.filtering;
        let picture = |painter: &Painter, rect: Rect, filtering: Filtering| filtering_picture(painter, rect, filtering, &tokens);
        let (picked, _) = sheet.dropdown(tr("Filtering"), filtering, &Filtering::ALL, |filtering| tr(filtering_label(filtering)).to_owned(), Some(&picture));
        if let Some(filtering) = picked {
            run(app, json!({"filtering": filtering.id()}));
        }
        // Tri-planar has no tile to repeat
        if !matches!(fill.projection, Projection::Triplanar { .. }) {
            let picture = |painter: &Painter, rect: Rect, wrap: UvWrap| wrap_picture(painter, rect, wrap, &tokens);
            let (picked, _) = sheet.dropdown(tr("UV Wrap"), fill.sampling.wrap, &UvWrap::ALL, |wrap| tr(wrap_label(wrap)).to_owned(), Some(&picture));
            if let Some(wrap) = picked {
                run(app, json!({"wrap": wrap.id()}));
            }
        }
    }

    world_rows(app, sheet, fill, base);
}

/// The preview square beside the size (tiling, or the tile's length while Physical size) and offset; Rotation under it
fn placement_rows(app: &mut TextureCraftApp, sheet: &mut PropertySheet<'_>, fill: &Fill, base: &Value, salt: &str, length: Option<[f32; 2]>, size: MeshSize) {
    let tokens = Tokens::get(sheet.ui.ctx());
    let (tiling, rotation, offset, offset_key) = match &fill.projection {
        Projection::Uv { scale, rotation, offset } => (*scale, *rotation, *offset, "offset"),
        Projection::Triplanar { uv, .. }
        | Projection::Planar { uv, .. }
        | Projection::Spherical { uv, .. }
        | Projection::Cylindrical { uv, .. }
        | Projection::Warp { uv, .. } => (uv.tiling, uv.rotation, uv.offset, "uvOffset"),
        Projection::UvTile => return,
    };

    let link_id = sheet.ui.make_persistent_id(("fill_tiling_link", salt));
    let mut linked = sheet.ui.data(|data| data.get_temp::<bool>(link_id)).unwrap_or(tiling[0] == tiling[1]);
    let was_linked = linked;
    let mut values = match length {
        Some(length) => length.map(|units| (units * CM_PER_UNIT).round()),
        None => tiling,
    };
    let shown = values;
    let (label, range, typed, decimals) = match length {
        Some(_) => (tr("Size (cm)"), SIZE_CM_TRACK, SIZE_CM_TYPED, 0),
        None => (tr("Tiling"), TILING_TRACK, TILING_TYPED, 2),
    };
    let size_edit = PreviewSize { label, values: &mut values, range, typed, decimals, linked: &mut linked };

    let mut lower_offset = offset;
    let mut hardness = match &fill.projection {
        Projection::Triplanar { sharpness, .. } => Some(*sharpness),
        _ => None,
    };
    let lower = match hardness.as_mut() {
        Some(hardness) => PreviewLower::Slider { label: tr("Hardness"), value: hardness, range: TRIPLANAR_HARDNESS_RANGE, decimals: 1 },
        None => PreviewLower::Offset { values: &mut lower_offset, range: OFFSET_TRACK, typed: OFFSET_TYPED },
    };
    let placement = Placement { tiling, rotation, offset };
    let picture = |painter: &Painter, rect: Rect| window_preview(painter, rect, placement, &tokens);
    let edit = sheet.preview_block(&picture, size_edit, lower);

    if linked != was_linked {
        sheet.ui.data_mut(|data| data.insert_temp(link_id, linked));
    }
    if let Some(merge) = edit.size
        && values != shown
    {
        let tiling = match length {
            Some(_) => tiling_for_length(&fill.projection, size, values.map(|cm| cm / CM_PER_UNIT)).unwrap_or(tiling),
            None => values,
        };
        app.run("layer.fill.projection", with(base, json!({"tiling": tiling, "merge": merge})));
    }
    if let Some(merge) = edit.lower {
        let fields = match hardness {
            Some(hardness) => json!({"sharpness": hardness, "merge": merge}),
            None => json!({offset_key: lower_offset, "merge": merge}),
        };
        app.run("layer.fill.projection", with(base, fields));
    }

    let mut degrees = rotation;
    if let Some(merge) = sheet.angle(tr("Rotation"), &mut degrees, ROTATION_RANGE) {
        app.run("layer.fill.projection", with(base, json!({"rotation": degrees, "merge": merge})));
    }
}

/// A 3D projection's placement on the mesh: its offset in X, Y and Z and its scale
fn world_rows(app: &mut TextureCraftApp, sheet: &mut PropertySheet<'_>, fill: &Fill, base: &Value) {
    let (scale, offset) = match &fill.projection {
        Projection::Triplanar { scale, offset, .. } | Projection::Planar { scale, offset, .. } => (*scale, Some(*offset)),
        Projection::Spherical { scale, .. } | Projection::Cylindrical { scale, .. } => (*scale, None),
        // The warp's 3D projection settings come with its panel rows (screen 02)
        Projection::UvTile | Projection::Uv { .. } | Projection::Warp { .. } => return,
    };
    if !sheet.band(tr("3D projection settings"), true, None, None).open {
        return;
    }

    if let Some(offset) = offset {
        let mut fields: Vec<NumberField> = ["X", "Y", "Z"]
            .into_iter()
            .zip(offset)
            .zip(AXIS_COLORS)
            .map(|((tag, value), color)| NumberField { tag, tag_color: Some(color), value, speed: OFFSET_FIELD_SPEED, decimals: 2 })
            .collect();
        if let Some((_, merge)) = sheet.number_fields(tr("Offset"), &mut fields, None, false) {
            let edited: Vec<f32> = fields.iter().map(|field| field.value).collect();
            app.run("layer.fill.projection", with(base, json!({"offset": edited, "merge": merge})));
        }
    }
    let mut scale = scale;
    if let Some(merge) = sheet.slider(tr("Scale"), &mut scale, WORLD_SCALE_RANGE, 2) {
        app.run("layer.fill.projection", with(base, json!({"scale": scale, "merge": merge})));
    }
}
