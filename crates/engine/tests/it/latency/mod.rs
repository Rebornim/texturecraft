//! Interaction latency benches: p50 / p95 / worst per action, split into compose and display conversion.
//! Run in release, one test at a time so the timings don't share cores:
//! `cargo test --release -p texturecraft-engine --test it latency:: -- --ignored --nocapture --test-threads=1`
//! `TC_LATENCY_SAMPLES` sets the sample count (default 30). Each line reads `fixture | action | metric | p50 | p95 | worst | n`.

mod duration_samples;
mod frame_timings;
mod latency_config;

use std::time::{Duration, Instant};

use duration_samples::DurationSamples;
use frame_timings::FrameTimings;
use latency_config::LatencyConfig;
use rayon::prelude::*;
use serde_json::{Value, json};
use texturecraft_engine::compose::{DisplayKind, tile_bytes};
use texturecraft_engine::core::{BakeResolution, BakedMap, Channel, EffectKind, LayerId, LayerKind};
use texturecraft_engine::geom::glam::Vec2;
use texturecraft_engine::{Session, execute, sample_session};

/// Relative brush sizes: the UI default and a large one
const BRUSHES: [(&str, f32); 2] = [("default brush", 0.02), ("large brush", 0.08)];

/// The two stacks the benches run on
#[derive(Clone, Copy)]
enum Fixture {
    /// The startup project: the 2K material ball with Steel Painted Worn
    Ball,
    /// The ~10 layer procedural stack of `perf.rs` at 4K
    Stack,
}

impl Fixture {
    const ALL: [Fixture; 2] = [Fixture::Ball, Fixture::Stack];

    fn open(self, config: &LatencyConfig) -> (String, Session) {
        match self {
            Fixture::Ball => {
                let mut session = sample_session(config.ball_resolution, config.ball_maps_resolution).unwrap();
                session.viewport = Vec2::new(1000.0, 880.0);
                session.update();
                display(&mut session);
                (format!("ball {}", config.ball_resolution), session)
            }
            Fixture::Stack => {
                let mut session = super::perf::stack(config.stack_resolution);
                display(&mut session);
                (format!("stack {}", config.stack_resolution), session)
            }
        }
    }
}

/// Converts every texture set's changed tiles for display as the UI does (in parallel); returns the time it took
fn display(session: &mut Session) -> Duration {
    let started = Instant::now();
    for texture_set in 0..session.caches.len() {
        let tiles = session.take_uploads(texture_set);
        let baked = session.baked(texture_set).cloned().unwrap_or_default();
        let composite = &session.caches[texture_set].composite;
        let bytes: usize = tiles.par_iter().flat_map_iter(|&(x, y)| DisplayKind::ALL.map(|kind| tile_bytes(composite, &baked, kind, x, y).len())).sum();
        std::hint::black_box(bytes);
    }
    started.elapsed()
}

fn run(session: &mut Session, command: &str, params: Value) {
    execute(session, command, &params).unwrap_or_else(|error| panic!("{command} {params}: {error}"));
}

/// The name of layer `id`, for the report
fn layer_name(session: &Session, id: LayerId) -> String {
    session.doc.project.layers.get(&id).map_or_else(|| format!("layer {}", id.0), |layer| layer.name.clone())
}

/// Visible-order layers of the active texture set, bottom → top
fn walk(session: &Session) -> Vec<LayerId> {
    session.doc.project.walk(session.active_set).into_iter().map(|(id, _)| id).collect()
}

/// Fill layers of the active set with a roughness channel, bottom → top
fn roughness_fills(session: &Session) -> Vec<LayerId> {
    let layers = &session.doc.project.layers;
    walk(session)
        .into_iter()
        .filter(|id| layers.get(id).is_some_and(|layer| matches!(&layer.kind, LayerKind::Fill(fill) if fill.sources.contains_key(&Channel::Roughness))))
        .collect()
}

/// Every effect of the active set (mask and content) with the layer it sits on, bottom → top
fn effects(session: &Session) -> Vec<(LayerId, LayerId)> {
    let layers = &session.doc.project.layers;
    walk(session)
        .into_iter()
        .filter_map(|id| layers.get(&id))
        .flat_map(|layer| layer.mask.iter().flat_map(|mask| mask.effects.iter()).chain(layer.content_effects.iter()).map(|effect| (layer.id, effect.id)))
        .collect()
}

/// A paint layer added right above the lowest non-folder layer: painting it recomposites everything above
fn add_bottom_paint_layer(session: &mut Session) -> String {
    let layers = &session.doc.project.layers;
    let lowest = walk(session).into_iter().find(|id| layers.get(id).is_some_and(|layer| !matches!(layer.kind, LayerKind::Group { .. }))).unwrap();
    let name = layer_name(session, lowest);
    run(session, "layer.select", json!({"layer": lowest.0}));
    run(session, "layer.add.paint", Value::Null);
    FrameTimings::until_settled(session);
    name
}

/// One stroke of `frames` frames; each frame stamps, composites the dirty tiles and converts them (the stroke's live latency)
fn stroke(session: &mut Session, frames: usize, start: Vec2) -> (DurationSamples, DurationSamples, DurationSamples, DurationSamples) {
    let (mut stamp, mut compose, mut display_time, mut frame) =
        (DurationSamples::default(), DurationSamples::default(), DurationSamples::default(), DurationSamples::default());
    session.begin_stroke(start, session.viewport, 1.0).unwrap();
    for i in 0..frames {
        let point = start + Vec2::new(i as f32 * 5.0, (i as f32 * 0.2).sin() * 60.0);
        let started = Instant::now();
        session.continue_stroke(point, 1.0);
        let stamped = started.elapsed();

        let started = Instant::now();
        session.update();
        let composed = started.elapsed();
        let converted = display(session);

        stamp.push(stamped);
        compose.push(composed);
        display_time.push(converted);
        frame.push(stamped + composed + converted);
    }
    (stamp, compose, display_time, frame)
}

/// Stroke latency per frame, on the top paint layer and on one at the bottom of the stack, then the trailing fill after release
fn bench_strokes(config: &LatencyConfig) {
    for fixture in Fixture::ALL {
        let (name, mut session) = fixture.open(config);
        for place in ["top of stack", "bottom of stack"] {
            let place = if place == "bottom of stack" { format!("{place} (above {})", add_bottom_paint_layer(&mut session)) } else { place.to_string() };
            for (brush, size) in BRUSHES {
                session.tool.brush.size = size;
                let action = format!("stroke, {brush}, {place}");
                let (mut stamp, mut compose, mut display_time, mut frame) =
                    (DurationSamples::default(), DurationSamples::default(), DurationSamples::default(), DurationSamples::default());
                let (mut release, mut trailing, mut trailing_frames) = (DurationSamples::default(), DurationSamples::default(), FrameTimings::default());
                for repeat in 0..config.repeats {
                    let start = Vec2::new(350.0, 380.0 + repeat as f32 * 30.0);
                    let (stamps, composes, displays, frames) = stroke(&mut session, config.samples, start);
                    stamp.append(&stamps);
                    compose.append(&composes);
                    display_time.append(&displays);
                    frame.append(&frames);

                    let started = Instant::now();
                    session.end_stroke().unwrap();
                    release.push(started.elapsed());
                    trailing_frames.absorb(FrameTimings::until_settled(&mut session), &mut trailing);
                }
                stamp.report(&name, &action, "stamp");
                compose.report(&name, &action, "compose");
                display_time.report(&name, &action, "display");
                frame.report(&name, &action, "frame total");
                release.report(&name, &action, "end_stroke");
                trailing.report(&name, &action, "trailing fill");
                trailing_frames.report(&name, &format!("{action}, trailing fill"));
            }
        }
    }
}

/// `edit(step)` applied `samples` times, each followed by UI frames until the view has caught up; `prepare(step)` runs and settles first, unmeasured
fn edits_until_settled(
    session: &mut Session,
    samples: usize,
    mut prepare: impl FnMut(&mut Session, usize),
    mut edit: impl FnMut(&mut Session, usize),
) -> (DurationSamples, DurationSamples, FrameTimings) {
    let (mut command, mut settled, mut frames) = (DurationSamples::default(), DurationSamples::default(), FrameTimings::default());
    for step in 0..samples {
        prepare(session, step);
        FrameTimings::until_settled(session);

        let started = Instant::now();
        edit(session, step);
        command.push(started.elapsed());
        frames.absorb(FrameTimings::until_settled(session), &mut settled);
    }
    (command, settled, frames)
}

fn report_edits(fixture: &str, action: &str, (command, settled, frames): (DurationSamples, DurationSamples, FrameTimings)) {
    command.report(fixture, action, "command");
    settled.report(fixture, action, "until settled");
    frames.report(fixture, action);
}

/// A slider value at drag step `step` of `steps`, from 0.2 to 0.8
fn slider_value(step: usize, steps: usize) -> f32 {
    0.2 + 0.6 * step as f32 / steps.saturating_sub(1).max(1) as f32
}

/// Slider drags: a fill's roughness and a generator's level, one undo step per drag as in the UI
fn bench_slider_drags(config: &LatencyConfig) {
    for fixture in Fixture::ALL {
        let (name, mut session) = fixture.open(config);
        let fill = *roughness_fills(&session).last().unwrap();
        let action = format!("roughness drag on fill {}", layer_name(&session, fill));
        let drag = edits_until_settled(
            &mut session,
            config.samples,
            |_, _| {},
            |session, step| {
                let value = slider_value(step, config.samples);
                run(session, "layer.fill.set", json!({"layer": fill.0, "channel": "roughness", "value": value, "merge": step > 0}));
            },
        );
        report_edits(&name, &action, drag);

        let generators: Vec<_> = effects(&session)
            .into_iter()
            .filter(|(_, effect)| matches!(session.doc.project.effect(*effect), Some((_, e)) if matches!(e.kind, EffectKind::Generator(_))))
            .collect();
        let (owner, generator) = *generators.last().unwrap();
        let action = format!("level drag on the generator of {}", layer_name(&session, owner));
        let drag = edits_until_settled(
            &mut session,
            config.samples,
            |_, _| {},
            |session, step| {
                let level = slider_value(step, config.samples);
                run(session, "effect.set", json!({"effect": generator.0, "level": level, "merge": step > 0}));
            },
        );
        report_edits(&name, &action, drag);
    }
}

/// Hiding and showing the first folder (the ball's smart material): the operator's "hide Steel Painted Worn takes 8 s"
fn bench_visibility(config: &LatencyConfig) {
    for fixture in Fixture::ALL {
        let (name, mut session) = fixture.open(config);
        let layers = &session.doc.project.layers;
        let folder = walk(&session).into_iter().find(|id| layers.get(id).is_some_and(|layer| matches!(layer.kind, LayerKind::Group { .. }))).unwrap();
        let folder_name = layer_name(&session, folder);
        for visible in [false, true] {
            let action = format!("{} folder {folder_name}", if visible { "show" } else { "hide" });
            // Each measured toggle starts from its unmeasured opposite
            let toggles = edits_until_settled(
                &mut session,
                config.samples,
                |session, _| run(session, "layer.visible", json!({"layer": folder.0, "visible": !visible})),
                |session, _| run(session, "layer.visible", json!({"layer": folder.0, "visible": visible})),
            );
            report_edits(&name, &action, toggles);
        }
    }
}

/// An edit at the bottom of the stack: everything above recomposites
fn bench_bottom_edit(config: &LatencyConfig) {
    for fixture in Fixture::ALL {
        let (name, mut session) = fixture.open(config);
        let fill = roughness_fills(&session)[0];
        let action = format!("roughness edit on the bottom fill {}", layer_name(&session, fill));
        let edits = edits_until_settled(
            &mut session,
            config.samples,
            |_, _| {},
            |session, step| {
                let value = if step % 2 == 0 { 0.3 } else { 0.7 };
                run(session, "layer.fill.set", json!({"layer": fill.0, "channel": "roughness", "value": value}));
            },
        );
        report_edits(&name, &action, edits);
    }
}

/// Undo and redo of a large stroke on the top paint layer
fn bench_undo_redo(config: &LatencyConfig) {
    for fixture in Fixture::ALL {
        let (name, mut session) = fixture.open(config);
        run(&mut session, "tool.brush", json!({"size": 40}));
        run(&mut session, "tool.stroke", json!({"points": [[250, 300], [450, 420], [650, 380]]}));
        FrameTimings::until_settled(&mut session);
        // Each measured step starts from its unmeasured opposite: the stroke done before an undo, undone before a redo
        let undo = edits_until_settled(
            &mut session,
            config.samples,
            |session, _| {
                if session.can_redo() {
                    session.redo().unwrap();
                }
            },
            |session, _| {
                session.undo().unwrap();
            },
        );
        report_edits(&name, "undo of a large stroke", undo);

        let redo = edits_until_settled(
            &mut session,
            config.samples,
            |session, _| {
                if !session.can_redo() {
                    session.undo().unwrap();
                }
            },
            |session, _| {
                session.redo().unwrap();
            },
        );
        report_edits(&name, "redo of a large stroke", redo);
    }
}

/// A CPU bake of the ball's first texture set at the bake size: each map alone, then all maps together
fn bench_bake(config: &LatencyConfig) {
    let (name, session) = Fixture::Ball.open(config);
    let resolution = BakeResolution::from_pixels(config.bake_resolution).unwrap();
    let maps: Vec<(String, Vec<BakedMap>)> =
        BakedMap::ALL.into_iter().map(|map| (map.id().to_string(), vec![map])).chain([("all maps".to_string(), BakedMap::ALL.to_vec())]).collect();
    for (label, selected) in maps {
        let mut times = DurationSamples::default();
        for _ in 0..config.repeats {
            let mut task = session.bake_task(0).unwrap();
            task.tracer = None;
            task.settings.common.resolution = resolution;
            task.settings.maps.select(selected.iter().copied());

            let started = Instant::now();
            match task.run(&texturecraft_engine::bake::Progress::new()) {
                Ok(_) => times.push(started.elapsed()),
                Err(error) => println!("{name} | bake {label} | skipped: {error}"),
            }
        }
        times.report(&name, &format!("bake {}² {label}", config.bake_resolution), "time");
    }
}

/// A full recomposite of the active set with nothing cached: the median of `repeats`
fn full_composite(session: &mut Session, repeats: usize) -> Duration {
    let texture_set = session.active_set;
    let mut times = DurationSamples::default();
    for _ in 0..repeats {
        let cache = &mut session.caches[texture_set];
        cache.below.clear();
        cache.invalidate_all();

        let started = Instant::now();
        session.update();
        times.push(started.elapsed());
        session.take_uploads(texture_set);
    }
    times.percentile(0.5)
}

/// What each layer and effect costs in a full recomposite: the time saved by hiding it alone (folders include their children, so rows overlap)
fn bench_effect_costs(config: &LatencyConfig) {
    for fixture in Fixture::ALL {
        let (name, mut session) = fixture.open(config);
        let baseline = full_composite(&mut session, config.repeats);
        println!("{name} | cost table | full recomposite {:.2} ms", baseline.as_secs_f64() * 1000.0);

        let mut rows = Vec::new();
        for id in walk(&session) {
            let label = layer_name(&session, id);
            run(&mut session, "layer.visible", json!({"layer": id.0, "visible": false}));
            rows.push((baseline.saturating_sub(full_composite(&mut session, config.repeats)), format!("layer {label}")));
            run(&mut session, "layer.visible", json!({"layer": id.0, "visible": true}));
        }
        for (owner, effect) in effects(&session) {
            let label = format!("{} > {}", layer_name(&session, owner), session.doc.project.effect(effect).unwrap().1.name);
            run(&mut session, "effect.set", json!({"effect": effect.0, "visible": false}));
            rows.push((baseline.saturating_sub(full_composite(&mut session, config.repeats)), format!("effect {label}")));
            run(&mut session, "effect.set", json!({"effect": effect.0, "visible": true}));
        }

        rows.sort_by(|a, b| b.0.cmp(&a.0));
        for (saved, label) in rows {
            let share = 100.0 * saved.as_secs_f64() / baseline.as_secs_f64().max(f64::EPSILON);
            println!("{name} | cost table | {label} | saves {:.2} ms | {share:.1}%", saved.as_secs_f64() * 1000.0);
        }
    }
}

#[test]
#[ignore]
fn latency_strokes() {
    bench_strokes(&LatencyConfig::full());
}

#[test]
#[ignore]
fn latency_slider_drags() {
    bench_slider_drags(&LatencyConfig::full());
}

#[test]
#[ignore]
fn latency_visibility() {
    bench_visibility(&LatencyConfig::full());
}

#[test]
#[ignore]
fn latency_bottom_edit() {
    bench_bottom_edit(&LatencyConfig::full());
}

#[test]
#[ignore]
fn latency_undo_redo() {
    bench_undo_redo(&LatencyConfig::full());
}

#[test]
#[ignore]
fn latency_bake() {
    bench_bake(&LatencyConfig::full());
}

#[test]
#[ignore]
fn latency_effect_costs() {
    bench_effect_costs(&LatencyConfig::full());
}

#[test]
fn every_latency_bench_runs_at_a_tiny_size() {
    let config = LatencyConfig::smoke();
    bench_strokes(&config);
    bench_slider_drags(&config);
    bench_visibility(&config);
    bench_bottom_edit(&config);
    bench_undo_redo(&config);
    bench_bake(&config);
    bench_effect_costs(&config);
}
