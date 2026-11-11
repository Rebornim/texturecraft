//! A generator effect's properties, laid out like the reference app's: attributes, resolution, parameters, image inputs.

use egui::{RichText, Ui};
use egui_phosphor::regular as ic;
use serde_json::{Value, json};
use texturecraft_engine::core::{
    BakedMap, CurvatureDetail, CurvatureMode, Generator, GeneratorKind, LayerId, OutputResolution, ResolutionMode, Source, WearParameters,
};

use crate::TextureCraftApp;
use crate::dock::{anchor_label, anchors_below, image_drop, image_menu, mesh_map_color};
use crate::i18n::{baked_map_label, tr, trf};
use crate::theme::{Tokens, caption};
use crate::widgets::{combo, section, slider_edit};

/// Random seeds stay below this, short enough to read and type.
const SEED_RANGE: u32 = 10_000;
/// Knuth's multiplicative hash constant: spreads the clock's nanoseconds over the seed range.
const SEED_MIX: u32 = 2_654_435_761;

/// Font size of the notes under a control.
const NOTE_FONT_SIZE: f32 = 10.5;

/// Edge of the picture beside each image input, its corner rounding and the size of an icon drawn in it.
const INPUT_TILE_SIZE: f32 = 32.0;
const INPUT_TILE_ROUNDING: f32 = 2.0;
const INPUT_TILE_ICON_SIZE: f32 = 16.0;

/// Width of a False/True button, so the buttons line up under each other
const TOGGLE_WIDTH: f32 = 136.0;

/// Width of the names in the Attributes section, so their values line up.
const ATTRIBUTE_KEY_WIDTH: f32 = 80.0;

/// The settings of generator effect `id`.
pub(crate) fn generator_properties(app: &mut TextureCraftApp, ui: &mut Ui, layer: LayerId, id: u64, generator: &Generator) {
    let tokens = Tokens::get(ui.ctx());
    crate::generator_slot::show(app, ui, id, generator);

    section(ui, "generator_attributes", tr("Attributes"), false, |ui| attributes(ui, generator.kind));
    section(ui, "generator_resolution", tr("Resolution"), false, |ui| resolution(app, ui, id, generator.resolution));
    section(ui, "generator_parameters", tr("Parameters"), true, |ui| {
        if let Some(defaults) = WearParameters::defaults(generator.kind) {
            // A wear generator saved before its wear controls shows them at their defaults: editing one moves it onto them
            wear_parameters(app, ui, id, generator, generator.wear.unwrap_or(defaults));
        } else {
            global_adjustments(app, ui, id, generator);
            ramp_parameters(app, ui, id, generator);
            if generator.kind == GeneratorKind::Curvature {
                curvature_detail_properties(app, ui, id, generator.detail.as_ref());
            }
        }
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(tr("Restore defaults")).clicked() {
                    app.run("effect.set", defaults_of(id, generator.kind));
                }
            });
        });
    });
    if !generator.kind.inputs().is_empty() {
        section(ui, "generator_inputs", tr("Image inputs"), true, |ui| {
            for map in generator.kind.inputs() {
                input_slot(app, ui, (layer, id), *map, generator.inputs.get(map), generator.detail.is_some());
            }
        });
    }
    if !app.session.doc.baked.contains_key(&app.session.ts()) {
        ui.label(
            RichText::new(format!("{} {}", ic::WARNING, tr("Bake mesh maps (F8) for curvature, AO and thickness based generators."))).color(tokens.warning),
        );
    }
}

/// What a generator kind makes, its category and its tags (read only).
fn attributes(ui: &mut Ui, kind: GeneratorKind) {
    let tokens = Tokens::get(ui.ctx());
    let (description, tags) = match kind {
        GeneratorKind::EdgeWear => (tr("Worn metal edges, scratched."), [tr("mask"), tr("generator"), tr("grayscale"), tr("curvature")]),
        GeneratorKind::FiberGlassEdgeWear => (tr("Worn fibreglass edges."), [tr("mask"), tr("generator"), tr("grayscale"), tr("curvature")]),
        GeneratorKind::Dirt => (tr("Grime in cavities."), [tr("mask"), tr("generator"), tr("grayscale"), tr("ambient occlusion")]),
        GeneratorKind::ThinAreas => (tr("Thin parts of the mesh."), [tr("mask"), tr("generator"), tr("grayscale"), tr("thickness")]),
        GeneratorKind::Curvature => (tr("Convex edges and cavities."), [tr("mask"), tr("generator"), tr("grayscale"), tr("curvature")]),
        GeneratorKind::AmbientOcclusion => (tr("Occluded areas."), [tr("mask"), tr("generator"), tr("grayscale"), tr("ambient occlusion")]),
        GeneratorKind::Position => (tr("A gradient across the mesh."), [tr("mask"), tr("generator"), tr("grayscale"), tr("position")]),
        GeneratorKind::Direction => (tr("Surfaces facing a light."), [tr("mask"), tr("generator"), tr("grayscale"), tr("normal")]),
    };

    // Wrapped rows: a long line must not widen the panel past its edge
    let tags = tags.join(", ");
    for (key, value) in [(tr("Description"), description), (tr("Category"), tr("Generators")), (tr("Tags"), tags.as_str())] {
        ui.horizontal_top(|ui| {
            ui.add_sized([ATTRIBUTE_KEY_WIDTH, 0.0], egui::Label::new(RichText::new(key).color(tokens.text_dim)));
            ui.add(egui::Label::new(value).wrap());
        });
    }
}

/// What the generator's output resolution is based on, the factor applied to it, and the size it comes to.
fn resolution(app: &mut TextureCraftApp, ui: &mut Ui, id: u64, resolution: OutputResolution) {
    let tokens = Tokens::get(ui.ctx());
    combo(ui, tr("Resolution"), tr(resolution.mode.label()), |ui| {
        for mode in ResolutionMode::ALL {
            if ui.selectable_label(mode == resolution.mode, tr(mode.label())).clicked() {
                app.run("effect.set", json!({"effect": id, "resolution": format!("{mode:?}")}));
            }
        }
    })
    .on_hover_text(tr("Auto and UV tile use the UV tile's resolution, Texture set the texture set's, Custom a size of its own."));
    if resolution.mode == ResolutionMode::Custom {
        combo(ui, tr("Size"), format!("{0} × {0}", resolution.custom), |ui| {
            for size in OutputResolution::CUSTOM_SIZES {
                if ui.selectable_label(size == resolution.custom, format!("{size} × {size}")).clicked() {
                    app.run("effect.set", json!({"effect": id, "customResolution": size}));
                }
            }
        });
    }
    combo(ui, tr("Factor"), factor_label(resolution.factor), |ui| {
        for factor in OutputResolution::FACTORS {
            if ui.selectable_label(factor == resolution.factor, factor_label(factor)).clicked() {
                app.run("effect.set", json!({"effect": id, "resolutionFactor": factor}));
            }
        }
    })
    .on_hover_text(tr("Scales the base resolution: below 1 the generator is computed coarser and smoothed back up."));

    let Ok(texture_set) = app.session.doc.project.texture_set(app.session.ts()).map(|set| set.resolution) else { return };

    let size = resolution.output_size(texture_set, texture_set);
    ui.horizontal(|ui| {
        ui.label(RichText::new(tr("Output size")).color(tokens.text_dim));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.label(format!("{size} × {size}")));
    })
    .response
    .on_hover_text(tr("A finer output than the texture set gives the same result as the texture set's resolution."));
}

/// The most powers of two a resolution factor scales by, either way.
const LARGEST_FACTOR_STEPS: u8 = OutputResolution::FACTORS.end().unsigned_abs();

/// "Same as context", or the fraction or multiple a resolution factor stands for.
fn factor_label(factor: i8) -> String {
    match factor {
        0 => tr("Same as context").to_string(),
        negative if negative < 0 => format!("1/{}", 1u32 << negative.unsigned_abs().min(LARGEST_FACTOR_STEPS)),
        positive => format!("×{}", 1u32 << positive.unsigned_abs().min(LARGEST_FACTOR_STEPS)),
    }
}

/// Global invert, blur, balance and contrast: what every generator applies to its finished value.
fn global_adjustments(app: &mut TextureCraftApp, ui: &mut Ui, id: u64, generator: &Generator) {
    let (mut invert, mut global) = (generator.invert, generator.global);
    let invert_edit = ui.checkbox(&mut invert, tr("Global invert")).changed().then_some(false);
    let blur_edit = slider_edit(ui, tr("Global blur"), &mut global.blur, 0.0..=1.0, 2);
    let balance_edit = slider_edit(ui, tr("Global balance"), &mut global.balance, 0.0..=1.0, 2);
    let contrast_edit = slider_edit(ui, tr("Global contrast"), &mut global.contrast, 0.0..=1.0, 2);
    if let Some(merge) = invert_edit.or(blur_edit).or(balance_edit).or(contrast_edit) {
        app.run(
            "effect.set",
            json!({"effect": id, "invert": invert, "blur": global.blur, "balance": global.balance, "contrast": global.contrast, "merge": merge}),
        );
    }
}

/// The wear controls of generator effect `id` in the reference app's order, showing `wear`
fn wear_parameters(app: &mut TextureCraftApp, ui: &mut Ui, id: u64, generator: &Generator, wear: WearParameters) {
    seed_row(app, ui, id, generator.seed);
    let mut invert = generator.invert;
    if true_false(ui, tr("Invert"), &mut invert) {
        app.run("effect.set", json!({"effect": id, "invert": invert}));
    }

    let dirt = generator.kind == GeneratorKind::Dirt;
    let (level_label, contrast_label) = if dirt { (tr("Dirt level"), tr("Dirt contrast")) } else { (tr("Wear level"), tr("Wear contrast")) };
    let mut edited = wear;
    let mut edit = slider_edit(ui, level_label, &mut edited.level, 0.0..=1.0, 2);
    edit = edit.or(slider_edit(ui, contrast_label, &mut edited.contrast, 0.0..=1.0, 2));
    let triplanar_edit = true_false(ui, tr("Use triplanar"), &mut edited.triplanar).then_some(false);
    edit = edit.or(triplanar_edit).or(slider_edit(ui, tr("Triplanar blending contrast"), &mut edited.triplanar_contrast, 0.0..=1.0, 2));
    edit = edit.or(slider_edit(ui, tr("Grunge amount"), &mut edited.grunge_amount, 0.0..=1.0, 2));
    if generator.kind != GeneratorKind::FiberGlassEdgeWear {
        edit = edit.or(slider_edit(ui, tr("Grunge scale"), &mut edited.grunge_scale, WearParameters::GRUNGE_SCALE_RANGE, 0));
    }
    if dirt {
        edit = edit.or(slider_edit(ui, tr("Edges masking"), &mut edited.edges_masking, 0.0..=1.0, 2));
    } else {
        edit = edit.or(slider_edit(ui, tr("Edges smoothness"), &mut edited.edges_smoothness, WearParameters::EDGES_SMOOTHNESS_RANGE, 2));
        edit = edit.or(slider_edit(ui, tr("Ambient occlusion masking"), &mut edited.ao_masking, 0.0..=1.0, 2));
        edit = edit.or(slider_edit(ui, tr("Curvature weight"), &mut edited.curvature_weight, 0.0..=1.0, 2));
    }

    if let Some(merge) = edit {
        let mut params = wear_params(generator.kind, &edited);
        params["effect"] = json!(id);
        params["merge"] = json!(merge);
        app.run("effect.set", params);
    }
}

/// The `effect.set` params that set every wear control a generator of `kind` has to `wear`
fn wear_params(kind: GeneratorKind, wear: &WearParameters) -> Value {
    let mut params = json!({
        "wearLevel": wear.level,
        "wearContrast": wear.contrast,
        "grungeAmount": wear.grunge_amount,
        "grungeScale": wear.grunge_scale,
        "triplanar": wear.triplanar,
        "triplanarContrast": wear.triplanar_contrast,
        "edgesSmoothness": wear.edges_smoothness,
        "aoMasking": wear.ao_masking,
        "curvatureWeight": wear.curvature_weight,
        "edgesMasking": wear.edges_masking,
    });
    if let Some(params) = params.as_object_mut() {
        params.retain(|key, _| texturecraft_engine::mask_cmds::wear_key_applies(kind, key));
    }
    params
}

/// An on/off control as the reference app draws it: the name, then a full-width button reading False or True; returns whether it was clicked
fn true_false(ui: &mut Ui, label: &str, value: &mut bool) -> bool {
    let mut clicked = false;
    ui.horizontal(|ui| {
        ui.label(label);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let text = if *value { tr("True") } else { tr("False") };
            if ui.add_sized([TOGGLE_WIDTH, ui.spacing().interact_size.y], egui::Button::new(text)).clicked() {
                *value = !*value;
                clicked = true;
            }
        });
    });
    clicked
}

/// Where the generator's ramp starts, how soft it is, the noise breaking it up, its offset and its seed.
fn ramp_parameters(app: &mut TextureCraftApp, ui: &mut Ui, id: u64, generator: &Generator) {
    let (mut level, mut softness, mut noise, mut noise_scale, mut offset) =
        (generator.level, generator.softness, generator.noise_amount, generator.noise_scale, generator.offset);
    let level_edit = slider_edit(ui, tr("Level"), &mut level, 0.0..=1.0, 2);
    let softness_edit = slider_edit(ui, tr("Softness"), &mut softness, 0.0..=1.0, 2);
    let noise_edit = slider_edit(ui, tr("Noise amount"), &mut noise, 0.0..=2.0, 2);
    let noise_scale_edit = slider_edit(ui, tr("Noise scale"), &mut noise_scale, 0.1..=64.0, 1);
    let offset_edit = slider_edit(ui, tr("Offset"), &mut offset, -1.0..=1.0, 2);
    if let Some(merge) = level_edit.or(softness_edit).or(noise_edit).or(noise_scale_edit).or(offset_edit) {
        app.run(
            "effect.set",
            json!({"effect": id, "level": level, "softness": softness, "noise": noise, "noiseScale": noise_scale, "offset": offset, "merge": merge}),
        );
    }
    seed_row(app, ui, id, generator.seed);
}

/// The seed with a button that picks a new one and a pencil that opens a field to type it, as in the reference.
fn seed_row(app: &mut TextureCraftApp, ui: &mut Ui, id: u64, seed: u32) {
    let typing_id = ui.make_persistent_id(("generator_seed_typing", id));
    let mut typing = ui.data(|data| data.get_temp::<bool>(typing_id)).unwrap_or(false);
    ui.horizontal(|ui| {
        ui.label(tr("Seed"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.selectable_label(typing, ic::PENCIL_SIMPLE).on_hover_text(tr("Type the seed")).clicked() {
                typing = !typing;
            }
            if typing {
                let mut typed = seed;
                let field = ui.add(egui::DragValue::new(&mut typed).speed(1.0));
                if field.changed() {
                    // The later changes of one drag continue its first undo step
                    let merge = field.dragged() && !field.drag_started();
                    app.run("effect.set", json!({"effect": id, "seed": typed, "merge": merge}));
                }
            } else if ui.button(tr("Random")).on_hover_text(trf("Seed {seed}", &[("seed", &seed)])).clicked() {
                app.run("effect.set", json!({"effect": id, "seed": random_seed(seed)}));
            }
        });
    });
    ui.data_mut(|data| data.insert_temp(typing_id, typing));
}

/// A seed different from `current`, from the clock.
fn random_seed(current: u32) -> u32 {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |elapsed| elapsed.subsec_nanos());
    let mixed = nanos.wrapping_mul(SEED_MIX) % SEED_RANGE;
    if mixed == current { (mixed + 1) % SEED_RANGE } else { mixed }
}

/// The `effect.set` params that put generator effect `id` back to the defaults of its kind.
fn defaults_of(id: u64, kind: GeneratorKind) -> Value {
    let defaults = Generator::new(kind);
    let mut params = json!({
        "effect": id,
        "seed": defaults.seed,
        "invert": defaults.invert,
        "resolution": format!("{:?}", defaults.resolution.mode),
        "customResolution": defaults.resolution.custom,
        "resolutionFactor": defaults.resolution.factor,
    });

    // A wear generator restores its wear controls only: the earlier model's keys would move it back onto that model
    let own = match &defaults.wear {
        Some(wear) => wear_params(kind, wear),
        None => json!({
            "level": defaults.level,
            "softness": defaults.softness,
            "noise": defaults.noise_amount,
            "noiseScale": defaults.noise_scale,
            "offset": defaults.offset,
            "direction": defaults.direction,
            "blur": defaults.global.blur,
            "balance": defaults.global.balance,
            "contrast": defaults.global.contrast,
        }),
    };
    if let (Some(params), Value::Object(own)) = (params.as_object_mut(), own) {
        params.extend(own);
    }
    if let (None, Some(detail)) = (&defaults.wear, defaults.detail) {
        params["mode"] = json!(format!("{:?}", detail.mode));
        params["sizes"] = json!(detail.weights);
    }
    params
}

/// The curvature mode and detail size weights of generator effect `id` (`detail` None: saved before detail sizes).
fn curvature_detail_properties(app: &mut TextureCraftApp, ui: &mut Ui, id: u64, detail: Option<&CurvatureDetail>) {
    let tokens = Tokens::get(ui.ctx());
    let shown = detail.cloned().unwrap_or_default();
    combo(ui, tr("Mode"), curvature_mode_label(shown.mode), |ui| {
        for mode in CurvatureMode::ALL {
            if ui.selectable_label(mode == shown.mode, curvature_mode_label(mode)).clicked() {
                app.run("effect.set", json!({"effect": id, "mode": format!("{mode:?}")}));
            }
        }
    });

    ui.label(caption(tr("Detail sizes")));
    let mut weights = shown.weights;
    let mut edit = None;
    for (weight, size) in weights.iter_mut().zip(CurvatureDetail::SIZES) {
        edit = edit.or(slider_edit(ui, &format!("{size} px"), weight, 0.0..=1.0, 2));
    }
    if let Some(merge) = edit {
        app.run("effect.set", json!({"effect": id, "sizes": weights, "merge": merge}));
    }
    if detail.is_none() {
        ui.label(RichText::new(tr("Reads the curvature map as baked: changing the mode or a size turns detail sizes on.")).color(tokens.text_dim));
    }
}

/// The translated name of curvature mode `mode`.
fn curvature_mode_label(mode: CurvatureMode) -> &'static str {
    match mode {
        CurvatureMode::Edges => tr("Edges"),
        CurvatureMode::Cavities => tr("Cavities"),
        CurvatureMode::Both => tr("Edges and cavities"),
        CurvatureMode::Raw => tr("Raw curvature"),
    }
}

/// One input of generator effect `id` on `layer`: what it reads (its mesh map unless `source` replaces it), a menu to replace it, and a button to read the mesh map again.
fn input_slot(app: &mut TextureCraftApp, ui: &mut Ui, (layer, id): (LayerId, u64), map: BakedMap, source: Option<&Source>, detail_sizes: bool) {
    let tokens = Tokens::get(ui.ctx());
    let input = map.id();
    let current = match source {
        None => trf("Mesh map: {map}", &[("map", &tr(baked_map_label(map)))]),
        Some(Source::Image { name, .. }) => trf("Image: {name}", &[("name", name)]),
        Some(Source::Uniform(_)) => tr("Uniform value").to_string(),
        Some(Source::Anchor { anchor, channel }) => anchor_label(app, *anchor, *channel),
        Some(Source::Procedural { .. }) => tr("Procedural").to_string(),
    };
    let group = ui.group(|ui| {
        ui.horizontal(|ui| {
            input_tile(app, ui, map, source);
            ui.vertical(|ui| {
                ui.label(RichText::new(tr(baked_map_label(map))).strong());
                ui.label(RichText::new(&current).size(NOTE_FONT_SIZE).color(tokens.text_dim));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if source.is_some() && ui.button(ic::X).on_hover_text(tr("Read the mesh map again")).clicked() {
                    app.run("effect.set", json!({"effect": id, "input": input, "inputBaked": true}));
                }
            });
        });
        // Each input's menu needs its own id: the dropdown's id comes from its label
        ui.push_id(input, |ui| {
            combo(ui, tr("Source"), current.clone(), |ui| {
                let anchors = anchors_below(app, layer);
                if ui.selectable_label(source.is_none(), tr("Mesh map")).clicked() {
                    app.run("effect.set", json!({"effect": id, "input": input, "inputBaked": true}));
                }
                if ui.selectable_label(matches!(source, Some(Source::Uniform(_))), tr("Uniform value")).clicked() {
                    app.run("effect.set", json!({"effect": id, "input": input, "inputValue": map.neutral()[0]}));
                }
                for (anchor, label) in &anchors {
                    if ui
                        .selectable_label(
                            matches!(source, Some(Source::Anchor { anchor: picked, .. }) if picked == anchor),
                            trf("Anchor: {name}", &[("name", label)]),
                        )
                        .clicked()
                    {
                        app.run("effect.set", json!({"effect": id, "input": input, "inputAnchor": anchor.0}));
                    }
                }
                if let Some(name) = image_menu(app, ui) {
                    app.run("effect.set", json!({"effect": id, "input": input, "inputImage": name}));
                }
            })
        });
        if let Some(Source::Uniform(value)) = source {
            let mut value = value[0];
            if let Some(merge) = slider_edit(ui, tr("Value"), &mut value, 0.0..=1.0, 2) {
                app.run("effect.set", json!({"effect": id, "input": input, "inputValue": value, "merge": merge}));
            }
        }
        if detail_sizes && map == BakedMap::Curvature && source.is_some_and(|source| !matches!(source, Source::Uniform(_))) {
            ui.label(
                RichText::new(tr("Detail sizes blur the mesh map, so a replaced curvature input is read at its own size only."))
                    .size(NOTE_FONT_SIZE)
                    .color(tokens.text_dim),
            );
        }
    });
    if let Some(name) = image_drop(ui, &group.response) {
        app.run("effect.set", json!({"effect": id, "input": input, "inputImage": name}));
    }
}

/// The picture of what an input reads: the image itself, the uniform value's gray, or the mesh map's swatch (as in the texture set's list).
fn input_tile(app: &mut TextureCraftApp, ui: &mut Ui, map: BakedMap, source: Option<&Source>) {
    let tokens = Tokens::get(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(egui::vec2(INPUT_TILE_SIZE, INPUT_TILE_SIZE), egui::Sense::hover());
    let painter = ui.painter();
    let whole = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
    match source {
        Some(Source::Image { name, .. }) => match app.session.images.get(name) {
            Some(image) => {
                let texture = app.thumbs.image(ui.ctx(), name, image);
                painter.image(texture.id(), rect, whole, egui::Color32::WHITE);
            }
            None => {
                painter.rect_filled(rect, INPUT_TILE_ROUNDING, tokens.button);
            }
        },
        Some(Source::Uniform(value)) => {
            painter.rect_filled(rect, INPUT_TILE_ROUNDING, egui::Color32::from_gray((value[0].clamp(0.0, 1.0) * 255.0).round() as u8));
        }
        Some(Source::Anchor { .. } | Source::Procedural { .. }) => {
            let glyph = if matches!(source, Some(Source::Anchor { .. })) { ic::ANCHOR } else { ic::MAGIC_WAND };
            painter.rect_filled(rect, INPUT_TILE_ROUNDING, tokens.button);
            painter.text(rect.center(), egui::Align2::CENTER_CENTER, glyph, egui::FontId::proportional(INPUT_TILE_ICON_SIZE), tokens.text_dim);
        }
        None => {
            let baked = app.session.doc.baked.get(&app.session.ts()).is_some_and(|baked| baked.maps.contains_key(&map));
            painter.rect_filled(rect, INPUT_TILE_ROUNDING, if baked { mesh_map_color(map) } else { tokens.button });
        }
    }
    painter.rect_stroke(rect, INPUT_TILE_ROUNDING, egui::Stroke::new(1.0, tokens.button_border), egui::StrokeKind::Outside);
}
