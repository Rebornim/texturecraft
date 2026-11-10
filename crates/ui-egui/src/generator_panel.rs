//! The properties of a generator effect, laid out like the reference app's generator panel:
//! the generator, its attributes, then its parameters with the global adjustments first.

use egui::{RichText, Ui};
use egui_phosphor::regular as ic;
use serde_json::{Value, json};
use texturecraft_engine::core::{CurvatureDetail, CurvatureMode, Generator, GeneratorKind, OutputResolution, ResolutionMode};

use crate::TextureCraftApp;
use crate::i18n::tr;
use crate::theme::{Tokens, caption};
use crate::widgets::{combo, section, slider_edit};

/// The settings of generator effect `id`.
pub(crate) fn generator_properties(app: &mut TextureCraftApp, ui: &mut Ui, id: u64, generator: &Generator) {
    let tk = Tokens::get(ui.ctx());
    ui.label(caption(tr("Generator")));
    egui::ComboBox::from_id_salt("gen_kind").selected_text(tr(generator.kind.label())).show_ui(ui, |ui| {
        for kind in GeneratorKind::ALL {
            if ui.selectable_label(kind == generator.kind, tr(kind.label())).clicked() {
                // Replace the generator kind: remove and add keeps ordering simple
                app.run("effect.remove", json!({"effect": id}));
                app.run("effect.add.generator", json!({"generator": format!("{kind:?}")}));
            }
        }
    });

    section(ui, "generator_attributes", tr("Attributes"), false, |ui| attributes(ui, generator.kind));
    section(ui, "generator_resolution", tr("Resolution"), false, |ui| resolution(app, ui, id, generator.resolution));
    section(ui, "generator_parameters", tr("Parameters"), true, |ui| {
        global_adjustments(app, ui, id, generator);
        ramp_parameters(app, ui, id, generator);
        if matches!(generator.kind, GeneratorKind::Curvature | GeneratorKind::EdgeWear) {
            curvature_detail_properties(app, ui, id, generator.detail.as_ref());
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(tr("Restore defaults")).clicked() {
                app.run("effect.set", defaults_of(id, generator.kind));
            }
        });
    });
    if !app.session.doc.baked.contains_key(&app.session.ts()) {
        ui.label(RichText::new(format!("{} {}", ic::WARNING, tr("Bake mesh maps (F8) for curvature, AO and thickness based generators."))).color(tk.warning));
    }
}

/// What a generator kind makes, its category and its tags (read only).
fn attributes(ui: &mut Ui, kind: GeneratorKind) {
    let tk = Tokens::get(ui.ctx());
    let (description, tags) = match kind {
        GeneratorKind::EdgeWear => {
            (tr("Convex edges from the curvature map, broken up by noise: worn paint and polished edges."), "mask, generator, grayscale, curvature")
        }
        GeneratorKind::Dirt => {
            (tr("Crevices and occluded areas from the ambient occlusion map: dirt and grime."), "mask, generator, grayscale, ambient occlusion")
        }
        GeneratorKind::ThinAreas => (tr("Thin parts of the mesh from the thickness map."), "mask, generator, grayscale, thickness"),
        GeneratorKind::Curvature => {
            (tr("Convex edges, cavities or both from the curvature map, at several detail sizes."), "mask, generator, grayscale, curvature")
        }
        GeneratorKind::AmbientOcclusion => (tr("Occluded areas from the ambient occlusion map."), "mask, generator, grayscale, ambient occlusion"),
        GeneratorKind::Position => (tr("A gradient along a world direction across the mesh."), "mask, generator, grayscale, position"),
        GeneratorKind::Direction => (tr("Surfaces facing a direction, as if lit from it."), "mask, generator, grayscale, normal"),
    };
    egui::Grid::new("generator_attributes_grid").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
        ui.label(RichText::new(tr("Description")).color(tk.text_dim));
        ui.add(egui::Label::new(description).wrap());
        ui.end_row();
        ui.label(RichText::new(tr("Category")).color(tk.text_dim));
        ui.label(tr("Generators"));
        ui.end_row();
        ui.label(RichText::new(tr("Tags")).color(tk.text_dim));
        ui.label(tags);
        ui.end_row();
    });
}

/// What the generator's output resolution is based on, the factor applied to it, and the size it comes to.
fn resolution(app: &mut TextureCraftApp, ui: &mut Ui, id: u64, resolution: OutputResolution) {
    let tk = Tokens::get(ui.ctx());
    combo(ui, tr("Resolution"), tr(resolution.mode.label()), |ui| {
        for mode in ResolutionMode::ALL {
            if ui.selectable_label(mode == resolution.mode, tr(mode.label())).clicked() {
                app.run("effect.set", json!({"effect": id, "resolution": format!("{mode:?}")}));
            }
        }
    })
    .on_hover_text(tr("Auto and Texture set use the texture set's resolution, UV tile the UV tile's, Custom a size of its own."));
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

    let context = app.session.doc.project.texture_set(app.session.ts()).map_or(2048, |set| set.resolution);
    let size = resolution.output_size(context);
    ui.horizontal(|ui| {
        ui.label(RichText::new(tr("Output size")).color(tk.text_dim));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.label(format!("{size} × {size}")));
    })
    .response
    .on_hover_text(tr("A finer output than the texture set gives the same result as the texture set's resolution."));
}

/// "Same as context", or the fraction or multiple a resolution factor stands for.
fn factor_label(factor: i8) -> String {
    match factor {
        0 => tr("Same as context").to_string(),
        negative if negative < 0 => format!("1/{}", 1u32 << negative.unsigned_abs().min(3)),
        positive => format!("×{}", 1u32 << positive.min(3)),
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

/// Where the generator's ramp starts, how soft it is, the noise breaking it up and its seed.
fn ramp_parameters(app: &mut TextureCraftApp, ui: &mut Ui, id: u64, generator: &Generator) {
    let (mut level, mut softness, mut noise, mut noise_scale) = (generator.level, generator.softness, generator.noise_amount, generator.noise_scale);
    let level_edit = slider_edit(ui, tr("Level"), &mut level, 0.0..=1.0, 2);
    let softness_edit = slider_edit(ui, tr("Softness"), &mut softness, 0.0..=1.0, 2);
    let noise_edit = slider_edit(ui, tr("Noise amount"), &mut noise, 0.0..=2.0, 2);
    let noise_scale_edit = slider_edit(ui, tr("Noise scale"), &mut noise_scale, 0.1..=64.0, 1);
    if let Some(merge) = level_edit.or(softness_edit).or(noise_edit).or(noise_scale_edit) {
        app.run("effect.set", json!({"effect": id, "level": level, "softness": softness, "noise": noise, "noiseScale": noise_scale, "merge": merge}));
    }
    seed_row(app, ui, id, generator.seed);
}

/// The seed with a button that picks a new one, and a field to type it.
fn seed_row(app: &mut TextureCraftApp, ui: &mut Ui, id: u64, seed: u32) {
    ui.horizontal(|ui| {
        ui.label(tr("Seed"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let mut typed = seed;
            if ui.add(egui::DragValue::new(&mut typed).speed(1.0)).changed() {
                app.run("effect.set", json!({"effect": id, "seed": typed}));
            }
            if ui.button(tr("Random")).clicked() {
                app.run("effect.set", json!({"effect": id, "seed": random_seed(seed)}));
            }
        });
    });
}

/// A seed different from `current`, from the clock.
fn random_seed(current: u32) -> u32 {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |elapsed| elapsed.subsec_nanos());
    let mixed = nanos.wrapping_mul(2_654_435_761) % 10_000;
    if mixed == current { (mixed + 1) % 10_000 } else { mixed }
}

/// The `effect.set` params that put generator effect `id` back to the defaults of its kind.
fn defaults_of(id: u64, kind: GeneratorKind) -> Value {
    let defaults = Generator::new(kind);
    let mut params = json!({
        "effect": id,
        "level": defaults.level,
        "softness": defaults.softness,
        "noise": defaults.noise_amount,
        "noiseScale": defaults.noise_scale,
        "seed": defaults.seed,
        "direction": defaults.direction,
        "invert": defaults.invert,
        "blur": defaults.global.blur,
        "balance": defaults.global.balance,
        "contrast": defaults.global.contrast,
        "resolution": format!("{:?}", defaults.resolution.mode),
        "customResolution": defaults.resolution.custom,
        "resolutionFactor": defaults.resolution.factor,
    });
    if let Some(detail) = defaults.detail {
        params["mode"] = json!(format!("{:?}", detail.mode));
        params["sizes"] = json!(detail.weights);
    }
    params
}

/// The curvature mode and detail size weights of generator effect `id` (`detail` None: saved before detail sizes).
fn curvature_detail_properties(app: &mut TextureCraftApp, ui: &mut Ui, id: u64, detail: Option<&CurvatureDetail>) {
    let tk = Tokens::get(ui.ctx());
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
        ui.label(RichText::new(tr("Reads the curvature map as baked: changing the mode or a size turns detail sizes on.")).color(tk.text_dim));
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
