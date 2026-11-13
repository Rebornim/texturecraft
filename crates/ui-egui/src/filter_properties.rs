//! The Filter effect's properties: FILTER over the channel pills (content), the filter card and its Parameters band

use egui::{Painter, Rect, Stroke, StrokeKind, Ui};
use serde_json::{Value, json};
use texturecraft_engine::core::{Channel, EffectKind, Filter, LayerId};
use texturecraft_engine::filter_lib::{build, find, scalar_params};

use crate::TextureCraftApp;
use crate::fill_material_panel::pill_name;
use crate::i18n::tr;
use crate::property_sheet::PropertySheet;
use crate::theme::Tokens;
use crate::widgets::color_swatch;

const SWATCH_WIDTH: f32 = 36.0;
/// The most colors `effect.set {colors}` takes for a gradient map
const MAX_GRADIENT_STOPS: usize = 16;
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
    let params = scalar_params(filter);

    if let Some(&(_, label, value, _, _)) = params.iter().find(|(key, ..)| *key == "seed") {
        let mut seed = value as u32;
        if sheet.seed(tr(label), &mut seed) {
            set(app, "seed", json!(seed), false);
        }
    }
    if let Filter::Hsl { hue, .. } = filter {
        let mut degrees = hue * 360.0;
        if let Some(merge) = sheet.signed_slider(tr("Hue shift (degrees)"), &mut degrees, -180.0..=180.0, 0) {
            set(app, "hue", json!(degrees / 360.0), merge);
        }
    }

    let divided = divided_key(filter);
    for (key, label, mut value, min, max) in params.into_iter().filter(|(key, ..)| *key != "seed") {
        let edit = if Some(key) == divided.map(|(key, _)| key) {
            intensity_row(app, sheet, id, &mut value, min..=max, divided.map_or(1, |(_, divider)| divider))
        } else if matches!(key, "angle" | "azimuth") {
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

    // The color editors keep their swatches until their own screens
    sheet.embed(|ui| color_editors(app, ui, id, filter));
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

/// The swatches of color balance, replace color and gradient map
fn color_editors(app: &mut TextureCraftApp, ui: &mut Ui, id: u64, filter: &Filter) {
    let set = |app: &mut TextureCraftApp, key: &str, value: Value, merge: bool| {
        let mut params = json!({ "effect": id, "merge": merge });
        params[key] = value;
        app.run("effect.set", params);
    };
    match filter {
        Filter::ColorBalance { shadows, midtones, highlights } => {
            for (key, label, rgb) in [("shadows", tr("Shadows"), shadows), ("midtones", tr("Midtones"), midtones), ("highlights", tr("Highlights"), highlights)]
            {
                ui.label(egui::RichText::new(label).strong());
                let mut balance = *rgb;
                let mut edit = None;
                for (component, name) in balance.iter_mut().zip([tr("Cyan / red"), tr("Magenta / green"), tr("Yellow / blue")]) {
                    edit = edit.or(crate::widgets::slider_edit(ui, name, component, -1.0..=1.0, 2));
                }
                if let Some(merge) = edit {
                    set(app, key, json!(balance), merge);
                }
            }
        }
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
        Filter::GradientMap { colors } => {
            let mut stops = colors.clone();
            let mut changed = false;
            ui.horizontal_wrapped(|ui| {
                for (index, stop) in stops.iter_mut().enumerate() {
                    changed |= color_swatch(ui, &format!("gradient_{id}_{index}"), stop, SWATCH_WIDTH);
                }
            });
            ui.horizontal(|ui| {
                let add = egui::Button::new(format!("{} {}", egui_phosphor::regular::PLUS, tr("Add color")));
                if ui.add_enabled(stops.len() < MAX_GRADIENT_STOPS, add).clicked() {
                    stops.push(stops.last().copied().unwrap_or([1.0; 3]));
                    changed = true;
                }
                let remove = egui::Button::new(format!("{} {}", egui_phosphor::regular::MINUS, tr("Remove last")));
                if ui.add_enabled(stops.len() > 1, remove).clicked() {
                    stops.pop();
                    changed = true;
                }
            });
            if changed {
                set(app, "colors", json!(stops), false);
            }
        }
        _ => {}
    }
}
