//! The Filter effect's properties: FILTER over the channel pills (content), the filter card and its Parameters band

use egui::{Painter, Rect, Stroke, StrokeKind, Ui};
use serde_json::{Value, json};
use texturecraft_engine::core::{Channel, CurvePreset, DirectionalLight, EffectKind, Filter, GrayscaleMode, LayerId, LightingOutput, StylizedLighting};
use texturecraft_engine::filter_lib::{build, find, scalar_params};

use crate::TextureCraftApp;
use crate::fill_material_panel::pill_name;
use crate::i18n::tr;
use crate::property_sheet::PropertySheet;
use crate::theme::{AXIS_COLORS, OPPOSITE_COLORS, Tokens};
use crate::widgets::color_swatch;

const SWATCH_WIDTH: f32 = 36.0;
/// The widest Divider chip text: the chip keeps one width whatever it shows
const WIDEST_DIVIDER: &str = "÷1000";

/// The properties of filter effect `effect` (`filter`), with its channel pills when it sits on the layer content
pub(crate) fn filter_properties(app: &mut TextureCraftApp, ui: &mut Ui, effect: LayerId, filter: &Filter) {
    let on_mask = crate::filter_slot::is_on_mask(app, effect.0);
    let off: Vec<Channel> = app.session.doc.project.effect(effect).map(|(_, found)| found.off_channels.iter().copied().collect()).unwrap_or_default();
    PropertySheet::show(ui, ("filter_sheet", effect.0), |sheet| {
        sheet.section(tr("Filter"), false);
        if !on_mask {
            channel_pills(app, sheet, effect, &off);
        }
        filter_card(app, sheet, effect, filter);
        if *filter == Filter::Empty {
            return;
        }

        let defaults = defaults_of(filter);
        let band = sheet.band(tr("Parameters"), true, Some(defaults.as_ref().is_some_and(|defaults| defaults != filter)), None);
        if band.reset {
            app.run("effect.filter.restoreDefaults", json!({"effect": effect.0}));
        }
        if band.open {
            parameters(app, sheet, effect.0, filter);
        }
    });
}

/// The library defaults of `filter` (None for an empty filter)
fn defaults_of(filter: &Filter) -> Option<Filter> {
    let def = find(filter.label())?;
    match build(def.id, &Value::Null) {
        Ok(EffectKind::Filter(defaults)) => Some(defaults),
        _ => None,
    }
}

/// One pill per texture set channel, lit where the filter works; Alt+click keeps only the clicked one
fn channel_pills(app: &mut TextureCraftApp, sheet: &mut PropertySheet<'_>, effect: LayerId, off: &[Channel]) {
    let channels = app.session.doc.project.texture_set(app.session.ts()).map(|texture_set| texture_set.channel_list()).unwrap_or_default();
    let names: Vec<&str> = channels.iter().map(|channel| tr(pill_name(*channel))).collect();
    let on: Vec<bool> = channels.iter().map(|channel| !off.contains(channel)).collect();
    let Some((index, solo)) = sheet.channel_pills(&names, &on) else { return };
    let Some(clicked) = channels.get(index) else { return };

    let switched: serde_json::Map<String, Value> = if solo {
        channels.iter().map(|channel| (channel.id().to_string(), json!(channel == clicked))).collect()
    } else {
        std::iter::once((clicked.id().to_string(), json!(!on.get(index).copied().unwrap_or(true)))).collect()
    };
    app.run("effect.set", json!({"effect": effect.0, "channels": switched}));
}

/// The card naming the filter (or None): a click opens the filter picker under it, × empties it
fn filter_card(app: &mut TextureCraftApp, sheet: &mut PropertySheet<'_>, effect: LayerId, filter: &Filter) {
    let ctx = sheet.ui.ctx().clone();
    let empty = *filter == Filter::Empty;
    let thumbnail = find(filter.label()).and_then(|def| app.thumbs.filter(&ctx, def.id));
    let tokens = Tokens::get(&ctx);
    let picture = |painter: &Painter, rect: Rect| match &thumbnail {
        Some(texture) => {
            painter.image(texture.id(), rect, Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
        }
        None => {
            painter.rect_stroke(rect.shrink(1.0), 3, Stroke::new(1.0, tokens.text_dim), StrokeKind::Inside);
        }
    };
    let (name, source) = if empty { (tr("Filter"), tr("None")) } else { (tr(filter.label()), tr("Filter")) };
    let slot = sheet.slot(&picture, name, source, None, !empty, false);
    if slot.clear {
        app.run("effect.set", json!({"effect": effect.0, "filter": "none"}));
    }
    crate::filter_slot::picker(app, &slot.pick, effect.0, filter);
}

/// The filter's own rows: Seed first, Intensity with its Divider chip, angles with their dial, then the color editors
fn parameters(app: &mut TextureCraftApp, sheet: &mut PropertySheet<'_>, id: u64, filter: &Filter) {
    let set = |app: &mut TextureCraftApp, key: &str, value: Value, merge: bool| {
        let mut params = json!({ "effect": id, "merge": merge });
        params[key] = value;
        app.run("effect.set", params);
    };
    match filter {
        Filter::ColorBalance { shadows, midtones, highlights } => return color_balance(app, sheet, id, [highlights, midtones, shadows]),
        Filter::ColorCorrect { shadows, midtones, highlights } => return color_correct(app, sheet, id, [shadows, midtones, highlights]),
        Filter::Hsl { hue, saturation, lightness, perceptive } => return hsl(app, sheet, id, [*hue, *saturation, *lightness], *perceptive),
        Filter::GradientMap { .. } => return gradient(app, sheet, id, filter),
        Filter::GradientCurve { preset, invert, balance, contrast } => return gradient_curve(app, sheet, id, *preset, *invert, [*balance, *contrast]),
        Filter::GradientDynamic { vertical, position, color } => return gradient_dynamic(app, sheet, id, *vertical, *position, *color),
        Filter::HaloGlow { on_mask, precise, color, .. } => return glow(app, sheet, id, filter, (*on_mask, *precise), *color),
        Filter::DropShadow { color, .. } => return drop_shadow(app, sheet, id, filter, *color),
        Filter::BakedLighting { .. } => return baked_lighting_stylized(app, sheet, id, filter),
        _ => {}
    }
    let params = scalar_params(filter);

    if let Some(&(_, label, value, _, _)) = params.iter().find(|(key, ..)| *key == "seed") {
        let mut seed = value as u32;
        if sheet.seed(tr(label), &mut seed) {
            set(app, "seed", json!(seed), false);
        }
    }
    let divided = divided_key(filter);
    for (key, label, mut value, min, max) in params.into_iter().filter(|(key, ..)| *key != "seed") {
        let edit = if Some(key) == divided.map(|(key, _)| key) {
            intensity_row(app, sheet, id, &mut value, min..=max, divided.map_or(1, |(_, divider)| divider))
        } else if matches!(key, "angle" | "azimuth" | "horizontalRotation" | "verticalRotation") {
            sheet.angle(tr(label), &mut value, min..=max)
        } else if min < 0.0 {
            sheet.signed_slider(tr(label), &mut value, min..=max, decimals(key, min, max))
        } else {
            sheet.slider(tr(label), &mut value, min..=max, decimals(key, min, max))
        };
        if let Some(merge) = edit {
            set(app, key, json!(value), merge);
        }
    }

    // Blur and Bevel read past the texture's edges when they wrap
    let wrap = match filter {
        Filter::Blur { wrap, .. } => Some((tr("Blur Wrap"), *wrap)),
        Filter::Bevel { wrap, .. } => Some((tr("Bevel Wrap"), *wrap)),
        _ => None,
    };
    if let Some((label, mut on)) = wrap
        && sheet.toggle(label, &mut on)
    {
        set(app, "wrap", json!(on), false);
    }

    // The color editors keep their swatches until their own screens
    sheet.embed(|ui| color_editors(app, ui, id, filter));
}

/// The tone groups' names and the `effect.set` keys they edit, in the order a panel shows them
const SHADOWS: (&str, &str) = ("Shadows", "shadows");
const MIDTONES: (&str, &str) = ("Midtones", "midtones");
const HIGHLIGHTS: (&str, &str) = ("Highlights", "highlights");

/// Color Balance: a card per tone (Highlights, Midtones, Shadows), each with Cyan ↔ Red, Magenta ↔ Green and Yellow ↔ Blue
fn color_balance(app: &mut TextureCraftApp, sheet: &mut PropertySheet<'_>, id: u64, tones: [&[f32; 3]; 3]) {
    let ends = [(tr("Cyan"), tr("Red")), (tr("Magenta"), tr("Green")), (tr("Yellow"), tr("Blue"))];
    let rights = ends.map(|(_, right)| right);
    for ((name, key), values) in [HIGHLIGHTS, MIDTONES, SHADOWS].into_iter().zip(tones) {
        if !sheet.card(tr(name), Some(true)) {
            sheet.close_card();
            continue;
        }
        let mut balance = *values;
        let mut edit = None;
        for (component, (value, (left, right))) in balance.iter_mut().zip(ends).enumerate() {
            let (left_tint, right_tint) =
                (OPPOSITE_COLORS.get(component).copied().unwrap_or_default(), AXIS_COLORS.get(component).copied().unwrap_or_default());
            edit = edit.or(sheet.tinted_slider((left, left_tint), (right, right_tint), &rights, value));
        }
        sheet.close_card();
        if let Some(merge) = edit {
            app.run("effect.set", json!({"effect": id, key: balance, "merge": merge}));
        }
    }
}

/// Color Correct: a card per tone (Shadows, Midtones, Highlights), each with Contrast, Luminosity and Saturation
fn color_correct(app: &mut TextureCraftApp, sheet: &mut PropertySheet<'_>, id: u64, tones: [&[f32; 3]; 3]) {
    for ((name, key), values) in [SHADOWS, MIDTONES, HIGHLIGHTS].into_iter().zip(tones) {
        if !sheet.card(tr(name), Some(true)) {
            sheet.close_card();
            continue;
        }
        let [mut contrast, mut luminosity, mut saturation] = *values;
        let edit = sheet
            .signed_slider(tr("Contrast"), &mut contrast, -1.0..=1.0, 2)
            .or(sheet.signed_slider(tr("Luminosity"), &mut luminosity, -1.0..=1.0, 2))
            .or(sheet.slider(tr("Saturation"), &mut saturation, 0.0..=1.0, 2));
        sheet.close_card();
        if let Some(merge) = edit {
            app.run("effect.set", json!({"effect": id, key: [contrast, luminosity, saturation], "merge": merge}));
        }
    }
}

/// HSL Perceptive: Mode (Regular | Perceptive), then Hue, Saturation and Lightness on 0..1 with 0.5 leaving them as they are
fn hsl(app: &mut TextureCraftApp, sheet: &mut PropertySheet<'_>, id: u64, [hue, saturation, lightness]: [f32; 3], perceptive: bool) {
    if let Some(mode) = sheet.switch(tr("Mode"), &[tr("Regular"), tr("Perceptive")], usize::from(perceptive)) {
        app.run("effect.set", json!({"effect": id, "mode": if mode == 1 { "perceptive" } else { "regular" }}));
    }

    // Shown around 0.5: the hue as half a turn either way, saturation and lightness as −1..1 halved
    let rows = [
        ("hue", tr("Hue"), 0.5 + hue, 1.0),
        ("saturation", tr("Saturation"), 0.5 + saturation / 2.0, 2.0),
        ("lightness", tr("Lightness"), 0.5 + lightness / 2.0, 2.0),
    ];
    for (key, label, shown, scale) in rows {
        let mut value = shown.clamp(0.0, 1.0);
        if let Some(merge) = sheet.centered_slider(label, &mut value, 0.0..=1.0, 0.5, 2) {
            app.run("effect.set", json!({"effect": id, key: (value - 0.5) * scale, "merge": merge}));
        }
    }
}

/// Gradient: Display Gradient (Debug), the Input Transformation card, then the Colors card (the ramp, Color Quantity, Blend Color in Linear and a row per color)
fn gradient(app: &mut TextureCraftApp, sheet: &mut PropertySheet<'_>, id: u64, filter: &Filter) {
    let Filter::GradientMap { colors, positions, input, linear, debug, .. } = filter else { return };
    let set = |app: &mut TextureCraftApp, params: Value| {
        let mut params = params;
        params["effect"] = json!(id);
        app.run("effect.set", params);
    };
    let mut shown = *debug;
    if sheet.toggle(tr("Display Gradient (Debug)"), &mut shown) {
        set(app, json!({"debug": shown}));
    }

    if sheet.card(tr("Input Transformation"), Some(true)) {
        let name = |mode: GrayscaleMode| tr(mode.label()).to_owned();
        if let (Some(mode), _) = sheet.dropdown(tr("Grayscale Mode"), input.mode, &GrayscaleMode::ALL, name, None) {
            set(app, json!({"grayscaleMode": mode.id()}));
        }
        for (key, label, value) in
            [("blur", tr("Blur Intensity"), input.blur), ("balance", tr("Balance"), input.balance), ("contrast", tr("Contrast"), input.contrast)]
        {
            let mut value = value;
            let range = if key == "blur" { 0.0..=16.0 } else { 0.0..=1.0 };
            if let Some(merge) = sheet.slider(label, &mut value, range, 2) {
                set(app, json!({key: value, "merge": merge}));
            }
        }
        let mut invert = input.invert;
        if sheet.toggle(tr("Invert"), &mut invert) {
            set(app, json!({"invert": invert}));
        }
    }
    sheet.close_card();

    if sheet.card(tr("Colors"), Some(true)) {
        let used = filter.ramp_colors();
        // An evenly spaced ramp (projects saved before) shows its places, and keeps them once one moves
        let last = used.len().saturating_sub(1).max(1) as f32;
        let mut places: Vec<f32> = (0..colors.len()).map(|index| positions.get(index).copied().unwrap_or(index as f32 / last)).collect();
        if let Some(drag) = sheet.gradient_ramp(used, &places, &|t| filter.ramp_at(t).unwrap_or([0.0; 3])) {
            if let Some(place) = places.get_mut(drag.index) {
                *place = drag.position;
            }
            set(app, json!({"positions": places, "merge": drag.merge}));
        }
        let mut quantity = used.len() as f32;
        if let Some(merge) = sheet.slider(tr("Color Quantity"), &mut quantity, 2.0..=f32::from(Filter::MAX_GRADIENT_COLORS), 0) {
            let quantity = quantity.round();
            if quantity as usize != used.len() {
                set(app, json!({"quantity": quantity, "merge": merge}));
            }
        }
        let mut in_linear = *linear;
        if sheet.toggle(tr("Blend Color in Linear"), &mut in_linear) {
            set(app, json!({"linear": in_linear}));
        }
        for (index, color) in used.iter().enumerate() {
            let label = format!("{} {}", tr("Color"), index + 1);
            let mut place = places.get(index).copied().unwrap_or(0.0);
            let (pick, moved) = sheet.color_stop(&label, *color, &mut place, app.picking_color, &app.session.doc.project.swatches);
            if let Some(picked) = crate::fill_material_panel::take_color_pick(app, pick) {
                let mut recolored = colors.clone();
                if let Some(slot) = recolored.get_mut(index) {
                    *slot = picked;
                }
                set(app, json!({"colors": recolored, "merge": pick.merge}));
            }
            if let Some(merge) = moved {
                if let Some(slot) = places.get_mut(index) {
                    *slot = place;
                }
                set(app, json!({"positions": places, "merge": merge}));
            }
        }
    }
    sheet.close_card();
}

/// Gradient Curve: Preset (each drawn as its curve), Invert, Balance and Contrast
fn gradient_curve(app: &mut TextureCraftApp, sheet: &mut PropertySheet<'_>, id: u64, preset: CurvePreset, invert: bool, [balance, contrast]: [f32; 2]) {
    let tokens = Tokens::get(sheet.ui.ctx());
    let picture = |painter: &Painter, rect: Rect, preset: CurvePreset| curve_picture(painter, rect, preset, &tokens);
    let name = |preset: CurvePreset| tr(preset.label()).to_owned();
    if let (Some(picked), _) = sheet.dropdown(tr("Preset"), preset, &CurvePreset::ALL, name, Some(&picture)) {
        app.run("effect.set", json!({"effect": id, "preset": picked.id()}));
    }
    let mut inverted = invert;
    if sheet.toggle(tr("Invert"), &mut inverted) {
        app.run("effect.set", json!({"effect": id, "invert": inverted}));
    }
    for (key, label, value) in [("balance", tr("Balance"), balance), ("contrast", tr("Contrast"), contrast)] {
        let mut value = value;
        if let Some(merge) = sheet.slider(label, &mut value, 0.0..=1.0, 2) {
            app.run("effect.set", json!({"effect": id, key: value, "merge": merge}));
        }
    }
}

/// A curve preset's shape over a dark square
fn curve_picture(painter: &Painter, rect: Rect, preset: CurvePreset, tokens: &Tokens) {
    painter.rect_filled(rect, 2, tokens.field);
    let inner = rect.shrink(CURVE_PICTURE_INSET);
    let points: Vec<egui::Pos2> = (0..=CURVE_PICTURE_STEPS)
        .map(|step| {
            let t = step as f32 / CURVE_PICTURE_STEPS as f32;
            egui::pos2(inner.left() + inner.width() * t, inner.bottom() - inner.height() * preset.at(t))
        })
        .collect();
    painter.add(egui::Shape::line(points, Stroke::new(CURVE_PICTURE_STROKE, tokens.accent)));
}

/// A curve picture's inset in its square, the points it is drawn through and its line's width
const CURVE_PICTURE_INSET: f32 = 2.0;
const CURVE_PICTURE_STEPS: usize = 30;
const CURVE_PICTURE_STROKE: f32 = 1.5;

/// Gradient Dynamic: Gradient Orientation and Gradient Input Position, then its Gradient Source under Image inputs
fn gradient_dynamic(app: &mut TextureCraftApp, sheet: &mut PropertySheet<'_>, id: u64, vertical: bool, position: f32, color: [f32; 3]) {
    if let Some(picked) = sheet.switch(tr("Gradient Orientation"), &[tr("Horizontal"), tr("Vertical")], usize::from(vertical)) {
        app.run("effect.set", json!({"effect": id, "orientation": if picked == 1 { "vertical" } else { "horizontal" }}));
    }
    let mut value = position;
    if let Some(merge) = sheet.slider(tr("Gradient Input Position"), &mut value, 0.0..=1.0, 2) {
        app.run("effect.set", json!({"effect": id, "position": value, "merge": merge}));
    }

    if !sheet.band(tr("Image inputs"), true, None, None).open {
        return;
    }
    let swatch = |painter: &Painter, rect: Rect| {
        painter.rect_filled(rect, 3, crate::fill_material_panel::shown_color(color));
    };
    sheet.slot(&swatch, tr("Gradient Source"), tr("Uniform"), None, false, false);
    let pick = sheet.color_bar(tr("Gradient Source"), color, app.picking_color, &app.session.doc.project.swatches);
    sheet.close_card();
    if let Some(picked) = crate::fill_material_panel::take_color_pick(app, pick) {
        app.run("effect.set", json!({"effect": id, "color": picked, "merge": pick.merge}));
    }
}

/// Glow: Used on a Mask, Mode (Soft | Precise), Width (inner below 0), Spread, Opacity and its Color
fn glow(app: &mut TextureCraftApp, sheet: &mut PropertySheet<'_>, id: u64, filter: &Filter, (on_mask, precise): (bool, bool), color: [f32; 3]) {
    let mut used = on_mask;
    if sheet.toggle(tr("Used on a Mask"), &mut used) {
        app.run("effect.set", json!({"effect": id, "usedOnMask": used}));
    }
    if let Some(mode) = sheet.switch(tr("Mode"), &[tr("Soft"), tr("Precise")], usize::from(precise)) {
        app.run("effect.set", json!({"effect": id, "mode": if mode == 1 { "precise" } else { "soft" }}));
    }
    for (key, label, mut value, min, max) in scalar_params(filter) {
        let edit = if min < 0.0 { sheet.signed_slider(tr(label), &mut value, min..=max, 2) } else { sheet.slider(tr(label), &mut value, min..=max, 2) };
        if let Some(merge) = edit {
            app.run("effect.set", json!({"effect": id, key: value, "merge": merge}));
        }
    }
    color_row(app, sheet, id, color);
}

/// Drop Shadow: Angle with its dial, Distance (either way), Size, Spread, Opacity and its Color
fn drop_shadow(app: &mut TextureCraftApp, sheet: &mut PropertySheet<'_>, id: u64, filter: &Filter, color: [f32; 3]) {
    for (key, label, mut value, min, max) in scalar_params(filter) {
        let edit = if key == "angle" {
            sheet.angle(tr(label), &mut value, min..=max)
        } else if min < 0.0 {
            sheet.signed_slider(tr(label), &mut value, min..=max, 2)
        } else {
            sheet.slider(tr(label), &mut value, min..=max, 2)
        };
        if let Some(merge) = edit {
            app.run("effect.set", json!({"effect": id, key: value, "merge": merge}));
        }
    }
    color_row(app, sheet, id, color);
}

/// Baked Lighting Stylized: a card each for the Sun, the Sky, Light 1 and 2, Occlusion and cavity, the Material and Normal details (one saved before shows its plain settings)
fn baked_lighting_stylized(app: &mut TextureCraftApp, sheet: &mut PropertySheet<'_>, id: u64, filter: &Filter) {
    let Filter::BakedLighting { azimuth, elevation, light, ambient, occlusion, stylized } = filter else { return };
    let s = stylized.as_deref().unwrap_or(&StylizedLighting::PLAIN);
    let set = |app: &mut TextureCraftApp, params: Value| {
        let mut params = params;
        params["effect"] = json!(id);
        app.run("effect.set", params);
    };
    let swatches = app.session.doc.project.swatches.clone();

    if sheet.card(tr("Sun"), Some(true)) {
        let mut value = *light;
        if let Some(merge) = sheet.slider(tr("Sun Intensity"), &mut value, 0.0..=10.0, 2) {
            set(app, json!({"light": value, "merge": merge}));
        }
        for (key, label, value, range) in
            [("azimuth", tr("Horizontal Angle"), *azimuth, 0.0..=360.0), ("elevation", tr("Vertical Angle"), elevation.rem_euclid(360.0), 0.0..=360.0)]
        {
            let mut value = value;
            if let Some(merge) = sheet.angle(label, &mut value, range) {
                set(app, json!({key: value, "merge": merge}));
            }
        }
        let pick = sheet.color_bar_led(tr("Sun Color"), s.sun_color, app.picking_color, &swatches);
        if let Some(picked) = crate::fill_material_panel::take_color_pick(app, pick) {
            set(app, json!({"sunColor": picked, "merge": pick.merge}));
        }
    }
    sheet.close_card();

    if sheet.card(tr("Sky"), Some(true)) {
        let mut value = *ambient;
        if let Some(merge) = sheet.slider(tr("Sky Intensity"), &mut value, 0.0..=2.0, 2) {
            set(app, json!({"ambient": value, "merge": merge}));
        }
        for (key, label, color) in [
            ("skyColor", tr("Sky Color"), s.sky_color),
            ("horizonColor", tr("Horizon Color"), s.horizon_color),
            ("groundColor", tr("Ground Color"), s.ground_color),
        ] {
            let pick = sheet.color_bar_led(label, color, app.picking_color, &swatches);
            if let Some(picked) = crate::fill_material_panel::take_color_pick(app, pick) {
                set(app, json!({key: picked, "merge": pick.merge}));
            }
        }
    }
    sheet.close_card();

    for (key, title, extra) in [("light1", tr("Light 1"), s.lights[0]), ("light2", tr("Light 2"), s.lights[1])] {
        if sheet.card(title, Some(true)) {
            extra_light(app, sheet, id, key, extra, &swatches);
        }
        sheet.close_card();
    }

    if sheet.card(tr("Occlusion and cavity"), Some(true)) {
        let rows = [
            ("occlusion", tr("Diffuse AO"), *occlusion, 1.0),
            ("diffuseCavity", tr("Diffuse Cavity"), s.diffuse_cavity, 1.0),
            ("specularAo", tr("Specular AO"), s.specular_ao, 1.0),
            ("specularCavity", tr("Specular Cavity"), s.specular_cavity, 1.0),
            ("cavitySmoothness", tr("Cavity Smoothness"), s.cavity_smoothness, 4.0),
            ("edgesIntensity", tr("Edges Intensity"), s.edges_intensity, 1.0),
            ("edgesSmoothness", tr("Edges Smoothness"), s.edges_smoothness, 1.0),
        ];
        for (key, label, value, max) in rows {
            let mut value = value;
            if let Some(merge) = sheet.slider(label, &mut value, 0.0..=max, 2) {
                set(app, json!({key: value, "merge": merge}));
            }
        }
    }
    sheet.close_card();

    if sheet.card(tr("Material"), Some(true)) {
        let input = |glossiness: bool| tr(if glossiness { "Specular / Glossiness" } else { "Metal / Roughness" }).to_owned();
        if let (Some(glossiness), _) = sheet.dropdown(tr("Input"), s.specular_glossiness, &[false, true], input, None) {
            set(app, json!({"input": if glossiness { "specularGlossiness" } else { "metalRoughness" }}));
        }
        let output = |output: LightingOutput| tr(output.label()).to_owned();
        if let (Some(picked), _) = sheet.dropdown(tr("Output"), s.output, &LightingOutput::ALL, output, None) {
            set(app, json!({"output": picked.id()}));
        }
        let mut value = s.dielectric_reflectance;
        if let Some(merge) = sheet.slider(tr("Dielectric Reflectance"), &mut value, 0.0..=0.16, 2) {
            set(app, json!({"dielectricReflectance": value, "merge": merge}));
        }
    }
    sheet.close_card();

    if sheet.card(tr("Normal details"), Some(true)) {
        let details = |mesh_only: bool| tr(if mesh_only { "Mesh only" } else { "Mesh + Height + Normal" }).to_owned();
        if let (Some(mesh_only), _) = sheet.dropdown(tr("Normal Details Type"), s.mesh_only, &[true, false], details, None) {
            set(app, json!({"normalDetails": if mesh_only { "mesh" } else { "meshHeightNormal" }}));
        }
        let mut value = s.height_to_normal;
        if let Some(merge) = sheet.slider(tr("Height to Normal Intensity"), &mut value, 0.0..=3.0, 2) {
            set(app, json!({"heightToNormal": value, "merge": merge}));
        }
    }
    sheet.close_card();
}

/// Light 1 or 2's rows (`key` names it to `effect.set`): Horizontal and Vertical Angle with their dials, Intensity and Color
fn extra_light(app: &mut TextureCraftApp, sheet: &mut PropertySheet<'_>, id: u64, key: &str, light: DirectionalLight, swatches: &[[f32; 3]]) {
    let mut edits: Vec<(Value, bool)> = Vec::new();
    for (part, label, value, range) in
        [("azimuth", tr("Horizontal Angle"), light.azimuth, 0.0..=360.0), ("elevation", tr("Vertical Angle"), light.elevation.rem_euclid(360.0), 0.0..=360.0)]
    {
        let mut value = value;
        if let Some(merge) = sheet.angle(label, &mut value, range) {
            edits.push((json!({part: value}), merge));
        }
    }
    let mut intensity = light.intensity;
    if let Some(merge) = sheet.slider(tr("Intensity"), &mut intensity, 0.0..=2.0, 2) {
        edits.push((json!({"intensity": intensity}), merge));
    }
    let pick = sheet.color_bar_led(tr("Color"), light.color, app.picking_color, swatches);
    if let Some(picked) = crate::fill_material_panel::take_color_pick(app, pick) {
        edits.push((json!({"color": picked}), pick.merge));
    }
    for (part, merge) in edits {
        app.run("effect.set", json!({"effect": id, key: part, "merge": merge}));
    }
}

/// A light or shadow filter's Color: its bar opens the colour picker, the eyedropper picks from the screen
fn color_row(app: &mut TextureCraftApp, sheet: &mut PropertySheet<'_>, id: u64, color: [f32; 3]) {
    let pick = sheet.color_bar_led(tr("Color"), color, app.picking_color, &app.session.doc.project.swatches);
    if let Some(picked) = crate::fill_material_panel::take_color_pick(app, pick) {
        app.run("effect.set", json!({"effect": id, "color": picked, "merge": pick.merge}));
    }
}

/// The parameter a filter's Divider divides and the divider it has
fn divided_key(filter: &Filter) -> Option<(&'static str, u16)> {
    match filter {
        Filter::Warp { divider, .. } | Filter::DirectionalWarp { divider, .. } => Some(("intensity", *divider)),
        Filter::SlopeBlur { divider, .. } => Some(("radius", *divider)),
        _ => None,
    }
}

/// Intensity with its "÷10" Divider chip before the value; the chip's list picks 1, 10, 100 or 1000
fn intensity_row(
    app: &mut TextureCraftApp,
    sheet: &mut PropertySheet<'_>,
    id: u64,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    divider: u16,
) -> Option<bool> {
    let shown = |divider: u16| format!("÷{divider}");
    let (edit, chip) = sheet.slider_with_chip(tr("Intensity"), value, range, 2, &shown(divider), WIDEST_DIVIDER);
    let (picked, _) = sheet.hanging_list(&chip, divider, &Filter::DIVIDERS, &shown, None);
    if let Some(divider) = picked {
        app.run("effect.set", json!({"effect": id, "divider": divider}));
    }
    edit
}

/// Decimals a parameter's value shows: whole numbers for counts, one for wide ranges
fn decimals(key: &str, min: f32, max: f32) -> usize {
    if key == "levels" {
        0
    } else if max - min > 10.0 {
        1
    } else {
        2
    }
}

/// The swatches of replace color
fn color_editors(app: &mut TextureCraftApp, ui: &mut Ui, id: u64, filter: &Filter) {
    let set = |app: &mut TextureCraftApp, key: &str, value: Value, merge: bool| {
        let mut params = json!({ "effect": id, "merge": merge });
        params[key] = value;
        app.run("effect.set", params);
    };
    match filter {
        Filter::ReplaceColor { from, to, .. } => {
            for (key, label, rgb) in [("from", tr("Color to replace"), from), ("to", tr("Replacement color"), to)] {
                let mut color = *rgb;
                ui.horizontal(|ui| {
                    if color_swatch(ui, &format!("replace_{key}_{id}"), &mut color, SWATCH_WIDTH) {
                        set(app, key, json!(color), false);
                    }
                    ui.label(label);
                });
            }
        }
        _ => {}
    }
}
