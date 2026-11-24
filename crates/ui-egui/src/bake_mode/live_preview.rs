//! The map preview: while Mesh Map Settings shows a map's settings, the 3D view shows that map on the model, re-baked small on a worker as the settings change

use std::collections::BTreeMap;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::{Arc, OnceLock};

use texturecraft_engine::bake::{BakeOutput, Progress};
use texturecraft_engine::core::{BakeResolution, BakedMap};
use texturecraft_engine::{Result, StreamedMap};

use super::live_preview_target::LivePreviewTarget;
use crate::TextureCraftApp;
use crate::docking::Panel;
use crate::state::Mode;

/// Sizes a dragged slider's previews step through, smallest first
const DRAG_SIZES: [BakeResolution; 5] = [BakeResolution::R64, BakeResolution::R128, BakeResolution::R256, BakeResolution::R512, BakeResolution::R1024];

/// A drag preview this quick (seconds) bakes one size up next time; one this slow, one size down
const DRAG_STEP_UP: f64 = 0.07;
const DRAG_STEP_DOWN: f64 = 0.14;

/// A preview bake on its worker: its size, when it started (app time) and whether a drag asked for it
struct RunningPreview {
    target: LivePreviewTarget,
    size: BakeResolution,
    started: f64,
    dragging: bool,
    progress: Arc<Progress>,
    receiver: Receiver<Result<BakeOutput>>,
}

/// The map preview's state: what it shows, the bake running for it, the drag change waiting for it, whether a drag still needs the full-size bake, and each map's drag size on this machine
#[derive(Default)]
pub(crate) struct LivePreview {
    shown: Option<LivePreviewTarget>,
    running: Option<RunningPreview>,
    drag_waiting: bool,
    refine_on_release: bool,
    drag_sizes: BTreeMap<BakedMap, BakeResolution>,
}

impl LivePreview {
    /// The map the preview shows now, if any
    pub(crate) fn shown(&self) -> Option<BakedMap> {
        self.shown.map(|target| target.map)
    }

    /// Stop the preview and give the 3D view back; a real bake calls this before it takes the view
    pub(crate) fn release(&mut self, session: &mut texturecraft_engine::Session) {
        self.cancel();
        self.refine_on_release = false;
        if self.shown.take().is_some() {
            session.bake_preview_end();
        }
    }

    fn cancel(&mut self) {
        self.drag_waiting = false;
        if let Some(running) = self.running.take() {
            running.progress.cancel.store(true, Ordering::Relaxed);
        }
    }

    /// The size `map` bakes at while a slider drags: it starts small and follows how fast this machine bakes it
    fn drag_size(&self, map: BakedMap) -> BakeResolution {
        self.drag_sizes.get(&map).copied().unwrap_or_else(|| first_drag_size(map))
    }

    /// A drag preview of `map` at `size` took `seconds`: step its drag size up or down one size, never past `chosen`
    fn learn_drag_size(&mut self, map: BakedMap, size: BakeResolution, seconds: f64, chosen: BakeResolution) {
        let Some(index) = DRAG_SIZES.iter().position(|step| *step == size) else { return };
        let next = if seconds < DRAG_STEP_UP {
            DRAG_SIZES.get(index + 1).copied().filter(|bigger| bigger.pixels() <= chosen.pixels()).unwrap_or(size)
        } else if seconds > DRAG_STEP_DOWN {
            index.checked_sub(1).and_then(|smaller| DRAG_SIZES.get(smaller).copied()).unwrap_or(size)
        } else {
            size
        };
        self.drag_sizes.insert(map, next);
    }
}

/// What the preview should show: the map whose settings Mesh Map Settings shows, in front, in Bake mode, while the preview is on and no real bake runs
pub(crate) fn wanted(app: &TextureCraftApp) -> Option<LivePreviewTarget> {
    let set = app.session.ts();
    let settings = &app.session.doc.project.texture_sets.get(set)?.bake;
    let shows = app.ui.mode == Mode::Baking && app.ui.dock.is_visible(Panel::MeshMapSettings) && app.bake.is_none();
    (shows && app.ui.bake_visualization.map_preview).then(|| LivePreviewTarget { set, map: super::selected_map(app.ui.selected_baked_map, settings) })
}

/// A map setting changed: bake the preview again. While a slider drags, a running bake finishes and shows before the newest change bakes, so pictures keep coming; otherwise the change cancels it and bakes at the chosen size
pub(crate) fn settings_changed(app: &mut TextureCraftApp, ctx: &egui::Context, dragging: bool) {
    let Some(target) = app.live_preview.shown else { return };

    // Web builds bake on the UI thread, so they wait for the release
    if dragging && cfg!(target_arch = "wasm32") {
        app.live_preview.refine_on_release = true;
        return;
    }

    app.live_preview.refine_on_release = dragging;
    if !dragging {
        start(app, ctx, target, app.ui.bake_visualization.map_preview_size, false);
    } else if app.live_preview.running.as_ref().is_some_and(|running| running.dragging) {
        app.live_preview.drag_waiting = true;
    } else {
        start(app, ctx, target, app.live_preview.drag_size(target.map), true);
    }
}

/// Once a frame: follow what the preview should show, take a finished bake, and refine after a drag
pub(crate) fn poll(app: &mut TextureCraftApp, ctx: &egui::Context) {
    // A new session (a project opened, the mesh replaced) has no preview layer: start over
    if app.live_preview.shown.is_some() && !app.session.bake_preview_active() {
        app.live_preview.shown = None;
        app.live_preview.cancel();
    }

    let wanted = wanted(app);
    if wanted != app.live_preview.shown {
        app.live_preview.release(&mut app.session);
        if let Some(target) = wanted {
            show(app, ctx, target);
        }
    }

    // The release: the drag's last value at the chosen size, replacing any drag bake still running
    if app.live_preview.refine_on_release && ctx.dragged_id().is_none() {
        app.live_preview.refine_on_release = false;
        if let Some(target) = app.live_preview.shown {
            start(app, ctx, target, app.ui.bake_visualization.map_preview_size, false);
        }
    }

    let Some(running) = &app.live_preview.running else { return };
    ctx.request_repaint_after(std::time::Duration::from_millis(16));
    match running.receiver.try_recv() {
        Ok(Ok(output)) => {
            let (target, size, dragging) = (running.target, running.size, running.dragging);
            let seconds = ctx.input(|input| input.time) - running.started;
            app.live_preview.running = None;
            if dragging {
                app.live_preview.learn_drag_size(target.map, size, seconds, app.ui.bake_visualization.map_preview_size);
            }
            if let Some(data) = output.maps.get(&target.map) {
                let streamed =
                    StreamedMap { map: target.map, index: 0, total: 1, resolution: output.resolution, height: output.height, data: Arc::new(data.clone()) };
                app.session.bake_preview_map(target.set, streamed);
            }

            // The newest drag change waited for this picture
            if std::mem::take(&mut app.live_preview.drag_waiting) && app.live_preview.refine_on_release {
                start(app, ctx, target, app.live_preview.drag_size(target.map), true);
            }
        }
        // A failed or cancelled preview keeps the picture it had
        Ok(Err(_)) | Err(TryRecvError::Disconnected) => app.live_preview.running = None,
        Err(TryRecvError::Empty) => {}
    }
}

/// Start showing `target`: the map's last real bake at once when there is one, else a preview bake at the chosen size
fn show(app: &mut TextureCraftApp, ctx: &egui::Context, target: LivePreviewTarget) {
    app.session.bake_preview_begin(vec![(target.set, vec![target.map])]);
    app.live_preview.shown = Some(target);

    let baked = app.session.baked(target.set).and_then(|baked| baked.maps.get(&target.map).map(|data| (baked.resolution, data.clone())));
    match baked {
        Some((resolution, data)) => {
            let height = (data.len() / target.map.components()).checked_div(resolution as usize).unwrap_or(0) as u32;
            app.session.bake_preview_map(target.set, StreamedMap { map: target.map, index: 0, total: 1, resolution, height, data });
        }
        None => {
            let size = app.ui.bake_visualization.map_preview_size;
            start(app, ctx, target, size, false);
        }
    }
}

/// Bake `target`'s map alone at `size` on a worker (`dragging`: for a slider still moving), cancelling the preview bake still running
fn start(app: &mut TextureCraftApp, ctx: &egui::Context, target: LivePreviewTarget, size: BakeResolution, dragging: bool) {
    app.live_preview.cancel();
    let Ok(mut task) = app.session.bake_task(target.set) else { return };

    task.settings.maps.select([target.map]);
    let own = task.settings.common.resolution;
    let size = if own.pixels() < size.pixels() { own } else { size };
    task.settings.common.resolution = size;
    task.settings.common.resolution_height = None;

    let progress = Arc::new(Progress::new());
    let worker_progress = progress.clone();
    let receiver = crate::spawn_worker("texturecraft-map-preview", move || match preview_pool() {
        Some(pool) => pool.install(|| task.run(&worker_progress)),
        None => task.run(&worker_progress),
    });
    let started = ctx.input(|input| input.time);
    app.live_preview.running = Some(RunningPreview { target, size, started, dragging, progress, receiver });
}

/// A dragged slider's first preview size before this machine's speed is known: 64 for the ray-traced bakers (well over 100 ms at 128 on a 4-core machine), else 128
fn first_drag_size(map: BakedMap) -> BakeResolution {
    match map {
        BakedMap::AmbientOcclusion | BakedMap::Thickness | BakedMap::BentNormal | BakedMap::Curvature => BakeResolution::R64,
        _ => BakeResolution::R128,
    }
}

/// The preview bakes' own thread pool, a thread short of the cores, so the UI's parallel work never waits behind a preview; None where no pool can start
fn preview_pool() -> Option<&'static rayon::ThreadPool> {
    static POOL: OnceLock<Option<rayon::ThreadPool>> = OnceLock::new();
    POOL.get_or_init(|| {
        let threads = std::thread::available_parallelism().map_or(1, |cores| cores.get().saturating_sub(1).max(1));
        let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).thread_name(|i| format!("texturecraft-map-preview-{i}")).build();
        if let Err(error) = &pool {
            log::warn!("map previews share the global thread pool: {error}");
        }
        pool.ok()
    })
    .as_ref()
}
