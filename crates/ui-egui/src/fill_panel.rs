//! A fill's Properties: its projection, size, placement and edges, then its material

use std::ops::RangeInclusive;

use egui::{Margin, Painter, Rect};
use egui_phosphor::regular as ic;
use serde_json::{Value, json};
use texturecraft_engine::compose::tile_length::{MeshSize, tile_length, tiling_for_length};
use texturecraft_engine::core::{Channel, Fill, Filtering, LayerId, Projection, Projector, ScaleMode, ShapeCrop, UvWrap};

use crate::TextureCraftApp;
use crate::dock::{anchors_below, fill_channel, with};
use crate::fill_pictures::{
    angle_picture, backface_picture, crop_picture, depth_picture, filtering_picture, projection_picture, warp_preview, window_preview, wrap_picture,
};
use crate::fill_placement::Placement;
use crate::i18n::{filtering_label, tr, wrap_label};
use crate::projection_kind::ProjectionKind;
use crate::projection_preview::projection_preview;
use crate::property_sheet::{NumberField, PAD, PreviewLower, PreviewSize, PropertySheet, RowLead};
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

/// Tri-planar blend's largest exponent: Hardness 1 (0 = exponent 1, 0.5 = 8)
const TRIPLANAR_SHARPNESS_MAX: f32 = 64.0;

/// Range of every Hardness the panel shows
const HARDNESS_RANGE: RangeInclusive<f32> = 0.0..=1.0;

/// What a cylindrical projection's Angle may be: the arc one repeat spans, in degrees
const CYLINDER_ANGLE_RANGE: RangeInclusive<f32> = 1.0..=360.0;

/// What Backface culling's Angle may be, in degrees from facing the projector
const BACKFACE_ANGLE_RANGE: RangeInclusive<f32> = 0.0..=180.0;

/// What the Projection Depth track spans, in lengths of the frame's longer side
const PROJECTION_DEPTH_TRACK: RangeInclusive<f32> = 0.0..=1.0;

/// Change per pixel dragged on a 3D offset field
const OFFSET_FIELD_SPEED: f32 = 0.01;

/// Change per pixel dragged on a 3D rotation field, in degrees
const ROTATION_FIELD_SPEED: f32 = 0.5;

/// Change per pixel dragged on a 3D scale field
const SCALE_FIELD_SPEED: f32 = 0.01;

/// The tabs over a fill's Properties: each jumps to its section
pub(crate) fn fill_tabs() -> [(&'static str, &'static str); 3] {
    [(ic::SLIDERS_HORIZONTAL, tr("Fill")), (ic::FLIP_HORIZONTAL, tr("Symmetry")), (ic::CIRCLE_HALF, tr("Material"))]
}

/// The mesh's size for the texture set being painted, as the projections see it
fn mesh_size(app: &TextureCraftApp) -> MeshSize {
    let mesh = &app.session.mesh;
    let material = app.session.doc.project.texture_set(app.session.ts()).map_or(0, |texture_set| texture_set.material);
    MeshSize { radius: mesh.radius(), uv_density: mesh.uv_density(material).unwrap_or(1.0) }
}

/// A fill's whole Properties: the Fill section, the Symmetry section, then the Material section with one slot per channel that is on
pub(crate) fn fill_sheet(app: &mut TextureCraftApp, sheet: &mut PropertySheet<'_>, fill: &Fill, base: &Value, on: &[Channel], reader: LayerId) {
    let salt = base.to_string();
    sheet.section(tr("Fill"), false);
    projection_rows(app, sheet, fill, base, &salt, reader);
    crate::fill_symmetry_panel::symmetry_section(app, sheet, &fill.projection, fill.sampling, base);

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

/// Projection, Move on model, Scale, the preview with size and offset, Rotation, the culling rows, then the Filtering and edges and 3D projection settings bands
fn projection_rows(app: &mut TextureCraftApp, sheet: &mut PropertySheet<'_>, fill: &Fill, base: &Value, salt: &str, reader: LayerId) {
    let tokens = Tokens::get(sheet.ui.ctx());
    let kind = ProjectionKind::of(&fill.projection);
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

    // Placing a frame on the surface is what moves it today; screen 05 moves every 3D projection
    if fill.projection.placed().is_some() {
        let placing = app.placing_decal == Some(reader);
        let response = sheet.button(ic::HAND, tr("Move on model"));
        let response = sheet.tip(response, tr("The next click in the 3D view moves the projection there"));
        if response.clicked() {
            app.placing_decal = if placing { None } else { Some(reader) };
        }
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
    frame_rows(app, sheet, fill, base);
    culling_rows(app, sheet, fill, base);

    if sheet.band(tr("Filtering and edges"), true, None, None).open {
        let filtering = fill.sampling.filtering;
        let picture = |painter: &Painter, rect: Rect, filtering: Filtering| filtering_picture(painter, rect, filtering, &tokens);
        let (picked, _) = sheet.dropdown(tr("Filtering"), filtering, &Filtering::ALL, |filtering| tr(filtering_label(filtering)).to_owned(), Some(&picture));
        if let Some(filtering) = picked {
            run(app, json!({"filtering": filtering.id()}));
        }
        // Tri-planar has no tile to repeat
        if kind != ProjectionKind::Triplanar {
            let picture = |painter: &Painter, rect: Rect, wrap: UvWrap| wrap_picture(painter, rect, wrap, &tokens);
            let (picked, _) = sheet.dropdown(tr("UV Wrap"), fill.sampling.wrap, &UvWrap::ALL, |wrap| tr(wrap_label(wrap)).to_owned(), Some(&picture));
            if let Some(wrap) = picked {
                run(app, json!({"wrap": wrap.id()}));
            }
        }
        if let Some(projector) = projector_of(&fill.projection) {
            // Its two values differ only at their end: the label stacks so they show in full
            let picture = |painter: &Painter, rect: Rect, crop: ShapeCrop| crop_picture(painter, rect, crop, &tokens);
            let lead = RowLead::Label { text: tr("Shape crop"), stack: true };
            let (picked, _) = sheet.dropdown_led(lead, projector.crop, &ShapeCrop::ALL, |crop| tr(crop_label(crop)).to_owned(), Some(&picture));
            if let Some(crop) = picked {
                run(app, json!({"shapeCrop": crop.id()}));
            }
        }
    }

    world_rows(app, sheet, fill, base, salt);
}

/// The settings of the projector itself, shared by every 3D projection
fn projector_of(projection: &Projection) -> Option<&Projector> {
    match projection {
        Projection::Triplanar { projector, .. }
        | Projection::Planar { projector, .. }
        | Projection::Spherical { projector, .. }
        | Projection::Cylindrical { projector, .. }
        | Projection::Warp { projector, .. } => Some(projector),
        Projection::Uv { .. } | Projection::UvTile => None,
    }
}

/// The name of a Shape crop choice
fn crop_label(crop: ShapeCrop) -> &'static str {
    use crate::i18n::n_;
    match crop {
        ShapeCrop::Cropped => n_("Projection Cropped to Shape"),
        ShapeCrop::ExtendsOutside => n_("Projection Extends Outside Shape"),
    }
}

/// Tri-planar Hardness as the panel shows it (0..1) from the blend's exponent (1..64): exponent = 64^hardness
fn triplanar_hardness(sharpness: f32) -> f32 {
    (sharpness.max(1.0).ln() / TRIPLANAR_SHARPNESS_MAX.ln()).clamp(0.0, 1.0)
}

/// The blend's exponent for a tri-planar Hardness the panel shows
fn triplanar_sharpness(hardness: f32) -> f32 {
    TRIPLANAR_SHARPNESS_MAX.powf(hardness.clamp(0.0, 1.0))
}

/// A warp's Projection Depth and Reset frame
fn frame_rows(app: &mut TextureCraftApp, sheet: &mut PropertySheet<'_>, fill: &Fill, base: &Value) {
    let Projection::Warp { depth, .. } = &fill.projection else { return };
    let mut depth = *depth;
    // Long, and alone in its panel: it keeps the column every projection shares
    if let Some(merge) = sheet.slider_led(RowLead::Loose(tr("Projection Depth")), &mut depth, PROJECTION_DEPTH_TRACK, 2, "") {
        app.run("layer.fill.projection", with(base, json!({"projectionDepth": depth, "merge": merge})));
    }
    if sheet.button(ic::ARROW_CLOCKWISE, tr("Reset frame")).clicked() {
        app.run("layer.fill.resetFrame", base.clone());
    }
}

/// Cylindrical Angle, then Depth culling (planar, warp) and Backface culling (planar, cylindrical), each with its settings under it
fn culling_rows(app: &mut TextureCraftApp, sheet: &mut PropertySheet<'_>, fill: &Fill, base: &Value) {
    let tokens = Tokens::get(sheet.ui.ctx());
    let run = |app: &mut TextureCraftApp, fields: Value| {
        app.run("layer.fill.projection", with(base, fields));
    };
    let (projector, angle) = match &fill.projection {
        Projection::Cylindrical { projector, angle, .. } => (projector, Some(*angle)),
        Projection::Planar { projector, .. } | Projection::Warp { projector, .. } => (projector, None),
        _ => return,
    };

    if let Some(mut angle) = angle {
        let arc = angle;
        let picture = |painter: &Painter, rect: Rect| angle_picture(painter, rect, arc, &tokens);
        let lead = RowLead::Picture { picture: &picture, text: tr("Angle") };
        if let Some(merge) = sheet.slider_led(lead, &mut angle, CYLINDER_ANGLE_RANGE, 0, "°") {
            run(app, json!({"angle": angle, "merge": merge}));
        }
    }

    if !matches!(fill.projection, Projection::Cylindrical { .. }) {
        let mut culling = projector.depth;
        let picture = |painter: &Painter, rect: Rect| depth_picture(painter, rect, &tokens);
        if sheet.picture_toggle(Some(&picture), tr("Depth culling"), &mut culling.on, false) {
            run(app, json!({"depthCulling": culling.on}));
        }
        if let Some(merge) = sheet.slider_led(RowLead::Child(tr("Hardness")), &mut culling.hardness, HARDNESS_RANGE, 2, "") {
            run(app, json!({"depthCullingHardness": culling.hardness, "merge": merge}));
        }
    }

    if !matches!(fill.projection, Projection::Warp { .. }) {
        let mut backface = projector.backface;
        let culled = backface.on;
        let picture = |painter: &Painter, rect: Rect| backface_picture(painter, rect, culled, &tokens);
        if sheet.picture_toggle(Some(&picture), tr("Backface culling"), &mut backface.on, false) {
            run(app, json!({"backfaceCulling": backface.on}));
        }
        // Cylindrical culls by the angle to its axis's outward direction alone
        if angle.is_none()
            && let Some(merge) = sheet.angle_led(RowLead::Child(tr("Angle")), &mut backface.angle, BACKFACE_ANGLE_RANGE)
        {
            run(app, json!({"backfaceAngle": backface.angle, "merge": merge}));
        }
        if let Some(merge) = sheet.slider_led(RowLead::Child(tr("Hardness")), &mut backface.hardness, HARDNESS_RANGE, 2, "") {
            run(app, json!({"backfaceHardness": backface.hardness, "merge": merge}));
        }
    }
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
        Projection::Triplanar { sharpness, .. } => Some(triplanar_hardness(*sharpness)),
        _ => None,
    };
    let lower = match hardness.as_mut() {
        Some(hardness) => PreviewLower::Slider { label: tr("Hardness"), value: hardness, range: HARDNESS_RANGE, decimals: 2 },
        None => PreviewLower::Offset { values: &mut lower_offset, range: OFFSET_TRACK, typed: OFFSET_TYPED },
    };
    let placement = Placement { tiling, rotation, offset };
    let corners = match &fill.projection {
        Projection::Warp { corners, .. } => Some(*corners),
        _ => None,
    };
    let projection = &fill.projection;
    let picture = |painter: &Painter, rect: Rect| match corners {
        Some(corners) => warp_preview(painter, rect, corners, &tokens),
        None => {
            if projection_preview(painter, rect, projection, &tokens).is_none() {
                window_preview(painter, rect, placement, &tokens);
            }
        }
    };
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
            Some(hardness) => json!({"sharpness": triplanar_sharpness(hardness), "merge": merge}),
            None => json!({offset_key: lower_offset, "merge": merge}),
        };
        app.run("layer.fill.projection", with(base, fields));
    }

    let mut degrees = rotation;
    if let Some(merge) = sheet.angle(tr("Rotation"), &mut degrees, ROTATION_RANGE) {
        app.run("layer.fill.projection", with(base, json!({"rotation": degrees, "merge": merge})));
    }
}

/// One X, Y, Z row of the 3D projection settings
fn xyz_fields(values: [f32; 3], speed: f32, decimals: usize, suffix: &'static str) -> [NumberField<'static>; 3] {
    let mut fields = ["X", "Y", "Z"].map(|tag| NumberField { tag, tag_color: None, value: 0.0, speed, decimals, suffix });
    for ((field, value), color) in fields.iter_mut().zip(values).zip(AXIS_COLORS) {
        field.value = value;
        field.tag_color = Some(color);
    }
    fields
}

/// The 3D projection settings: the projector's Offset, Rotation and Scale in X, Y and Z; a frame on the surface has no Offset (it moves with the frame)
fn world_rows(app: &mut TextureCraftApp, sheet: &mut PropertySheet<'_>, fill: &Fill, base: &Value, salt: &str) {
    let Some(projector) = projector_of(&fill.projection) else { return };
    let offset = match &fill.projection {
        Projection::Planar { placed: Some(_), .. } | Projection::Warp { .. } => None,
        Projection::Triplanar { offset, .. }
        | Projection::Planar { offset, .. }
        | Projection::Spherical { offset, .. }
        | Projection::Cylindrical { offset, .. } => Some(*offset),
        Projection::Uv { .. } | Projection::UvTile => None,
    };
    if !sheet.band(tr("3D projection settings"), true, None, None).open {
        return;
    }
    let edited = |fields: &[NumberField]| -> Vec<f32> { fields.iter().map(|field| field.value).collect() };

    if let Some(offset) = offset {
        let mut fields = xyz_fields(offset, OFFSET_FIELD_SPEED, 2, "");
        if let Some((_, merge)) = sheet.number_fields(tr("Offset"), &mut fields, None, true) {
            app.run("layer.fill.projection", with(base, json!({"offset": edited(&fields), "merge": merge})));
        }
    }
    let mut fields = xyz_fields(projector.rotation, ROTATION_FIELD_SPEED, 0, "°");
    if let Some((_, merge)) = sheet.number_fields(tr("Rotation"), &mut fields, None, true) {
        app.run("layer.fill.projection", with(base, json!({"projectorRotation": edited(&fields), "merge": merge})));
    }

    let link_id = sheet.ui.make_persistent_id(("projector_scale_link", salt));
    let [x, y, z] = projector.scale;
    let mut linked = sheet.ui.data(|data| data.get_temp::<bool>(link_id)).unwrap_or(x == y && y == z);
    let was_linked = linked;
    let mut fields = xyz_fields(projector.scale, SCALE_FIELD_SPEED, 2, "");
    if let Some((axis, merge)) = sheet.number_fields(tr("Scale"), &mut fields, Some(&mut linked), false) {
        let mut scale = edited(&fields);
        if linked && let Some(&value) = scale.get(axis) {
            scale = vec![value; 3];
        }
        app.run("layer.fill.projection", with(base, json!({"projectorScale": scale, "merge": merge})));
    }
    if linked != was_linked {
        sheet.ui.data_mut(|data| data.insert_temp(link_id, linked));
    }
}
