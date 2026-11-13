//! Properties of a levels effect: its caption, the channel it remaps (content), the levels editor over the histogram, the histogram's view (colour channels), then Reset, Auto, Invert and Clamp

use std::sync::Arc;

use egui::Ui;
use serde_json::json;
use texturecraft_engine::compose::EffectHistogram;
use texturecraft_engine::core::{Channel, ChannelKind, Layer, LayerId, Levels};

use crate::TextureCraftApp;
use crate::i18n::tr;
use crate::levels_display::LevelsDisplay;
use crate::levels_editor::levels_editor;
use crate::property_sheet::PropertySheet;
use crate::theme::Tokens;

/// Buckets of the histogram, over 0..1
const HISTOGRAM_BUCKETS: usize = 64;

/// Share of the counted texels a bucket needs to hold for Auto to keep it in the input range
const AUTO_MIN_SHARE: f64 = 0.001;

/// The properties of levels effect `effect_id` of `layer`; every edit goes through `effect.set`
pub(crate) fn levels_properties(app: &mut TextureCraftApp, ui: &mut Ui, layer: &Layer, effect_id: LayerId, levels: &Levels) {
    let is_content = layer.content_effects.iter().any(|effect| effect.id == effect_id);
    let choices = if is_content { affects_choices(app, layer, levels) } else { Vec::new() };
    let channel = levels.affects.or(choices.iter().copied().flatten().next()).unwrap_or(Channel::BaseColor);
    let histogram = histogram_counts(app, ui, effect_id, channel);
    let shows_colors = is_content && channel.kind() == ChannelKind::Color;
    let display_id = egui::Id::unique(("levels_display", effect_id.0));
    let display = if shows_colors { ui.data(|data| data.get_temp(display_id)).unwrap_or_default() } else { LevelsDisplay::Lum };
    let tokens = Tokens::get(ui.ctx());

    let mut edited = *levels;
    let mut edit = None;
    PropertySheet::show(ui, ("levels_sheet", effect_id.0), |sheet| {
        let target = if is_content { affects_name(levels.affects) } else { tr("Mask").to_string() };
        sheet.section(&format!("{} - {target}", tr("Levels")), false);
        if is_content && let (Some(Some(picked)), _) = sheet.dropdown(tr("Affects"), levels.affects, &choices, affects_name, None) {
            app.run("effect.set", json!({"effect": effect_id.0, "affects": picked.id()}));
        }

        let series = histogram.as_ref().map(|histogram| display.series(histogram, &tokens)).unwrap_or_default();
        edit = sheet.embed(|ui| levels_editor(ui, effect_id, &series, &mut edited));
        if shows_colors {
            let words: Vec<&str> = LevelsDisplay::ALL.iter().map(|view| view.word()).collect();
            let tints: Vec<&[egui::Color32]> = LevelsDisplay::ALL.iter().map(|view| view.tints()).collect();
            let chosen = LevelsDisplay::ALL.iter().position(|view| *view == display).unwrap_or_default();
            if let Some(view) = sheet.wide_switch(&words, chosen, &tints).and_then(|index| LevelsDisplay::ALL.get(index).copied()) {
                sheet.ui.data_mut(|data| data.insert_temp(display_id, view));
            }
        }

        let auto_range = histogram.as_ref().and_then(|histogram| auto_input_range(display.auto_counts(histogram)));
        let mut clamp = levels.clamp;
        let (reset, auto, invert) = (tr("Reset"), tr("Auto"), tr("Invert"));
        let actions = sheet.action_row(&[(reset, edited != edited.reset()), (auto, auto_range.is_some()), (invert, true)], tr("Clamp"), &mut clamp);
        match (actions.clicked, auto_range) {
            (Some(0), _) => edited = edited.reset(),
            (Some(1), Some((low, high))) => (edited.in_low, edited.in_high) = (low, high),
            (Some(2), _) => std::mem::swap(&mut edited.out_low, &mut edited.out_high),
            _ => {}
        }
        if actions.clicked.is_some() {
            edit = Some(false);
        }
        if actions.toggled {
            app.run("effect.set", json!({"effect": effect_id.0, "clamp": clamp}));
        }
    });

    // Auto or Invert with nothing to change would add an empty undo step
    if let Some(merge) = edit
        && edited != *levels
    {
        let Levels { in_low, in_high, gamma, out_low, out_high, .. } = edited;
        app.run(
            "effect.set",
            json!({"effect": effect_id.0, "inLow": in_low, "inHigh": in_high, "gamma": gamma, "outLow": out_low, "outHigh": out_high, "merge": merge}),
        );
    }
}

/// What a content levels can remap: its texture set's channels but normal maps (levels pass them through); levels from before the choice first, remapping all of them
fn affects_choices(app: &TextureCraftApp, layer: &Layer, levels: &Levels) -> Vec<Option<Channel>> {
    let project = &app.session.doc.project;
    let channels = project.locate(layer.id).and_then(|(set, _, _)| project.texture_set(set).ok()).map(|set| set.channel_list()).unwrap_or_default();
    let remappable = channels.into_iter().filter(|channel| channel.kind() != ChannelKind::NormalMap).map(Some);
    levels.affects.is_none().then_some(None).into_iter().chain(remappable).collect()
}

/// The name of what a levels remaps
fn affects_name(affects: Option<Channel>) -> String {
    affects.map_or_else(|| tr("All channels").to_string(), |channel| tr(channel.label()).to_string())
}

/// The histograms levels effect `effect_id` remaps, recomputed after a change once no pointer button is held
fn histogram_counts(app: &TextureCraftApp, ui: &Ui, effect_id: LayerId, channel: Channel) -> Option<Arc<EffectHistogram>> {
    let key = egui::Id::unique(("levels_histogram", effect_id.0, channel));
    let owner = ui.make_persistent_id("levels_histogram_owner");

    // Selection changes the revision, so old effects' histograms cannot be reused
    ui.data_mut(|data| {
        let previous = std::mem::replace(data.get_temp_mut_or(owner, key), key);
        if previous != key {
            data.remove::<(u64, Arc<EffectHistogram>)>(previous);
        }
    });

    let revision = app.session.revision;
    match ui.data(|data| data.get_temp::<(u64, Arc<EffectHistogram>)>(key)) {
        Some((cached, histogram)) if cached == revision || ui.input(|input| input.pointer.any_down()) => Some(histogram),
        _ => app.session.effect_histogram(effect_id, channel, HISTOGRAM_BUCKETS).map(|histogram| {
            let histogram = Arc::new(histogram);
            ui.data_mut(|data| data.insert_temp(key, (revision, histogram.clone())));
            histogram
        }),
    }
}

/// The input range Auto picks: the first to the last bucket holding more than [`AUTO_MIN_SHARE`] of the texels
fn auto_input_range(counts: &[u32]) -> Option<(f32, f32)> {
    let total: u64 = counts.iter().map(|&count| u64::from(count)).sum();
    let holds_enough = |count: &u32| f64::from(*count) > total as f64 * AUTO_MIN_SHARE;
    let first = counts.iter().position(holds_enough)?;
    let last = counts.iter().rposition(holds_enough)?;
    let bucket_count = counts.len() as f32;
    Some((first as f32 / bucket_count, (last + 1) as f32 / bucket_count))
}

#[cfg(test)]
mod tests {
    use super::{HISTOGRAM_BUCKETS, auto_input_range};

    #[test]
    fn auto_skips_buckets_under_the_share() {
        let mut counts = vec![0u32; HISTOGRAM_BUCKETS];
        counts[3] = 1;
        counts[10] = 5_000;
        counts[40] = 5_000;
        counts[60] = 1;

        assert_eq!(auto_input_range(&counts), Some((10.0 / HISTOGRAM_BUCKETS as f32, 41.0 / HISTOGRAM_BUCKETS as f32)));
    }

    #[test]
    fn auto_has_nothing_to_fit_in_an_empty_histogram() {
        assert_eq!(auto_input_range(&[0; HISTOGRAM_BUCKETS]), None);
        assert_eq!(auto_input_range(&[]), None);
    }
}
