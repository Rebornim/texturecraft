//! The map preview: while Mesh Map Settings shows a map's settings, the 3D view shows that map on the model, re-baked small on a worker as the settings change

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, TryRecvError};

use texturecraft_engine::bake::{BakeOutput, Progress};
use texturecraft_engine::core::{BakeResolution, BakedMap};
use texturecraft_engine::{Result, StreamedMap};

use super::live_preview_target::LivePreviewTarget;
use crate::TextureCraftApp;
use crate::docking::Panel;
use crate::state::Mode;

/// A preview bake on its worker
struct RunningPreview {
    target: LivePreviewTarget,
    progress: Arc<Progress>,
    receiver: Receiver<Result<BakeOutput>>,
}

/// The map preview's state: what it shows, the bake running for it, and whether a drag's small bakes still need the full-size one
#[derive(Default)]
pub(crate) struct LivePreview {
    shown: Option<LivePreviewTarget>,
    running: Option<RunningPreview>,
    refine_on_release: bool,
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
        if let Some(running) = self.running.take() {
            running.progress.cancel.store(true, Ordering::Relaxed);
        }
    }
}

/// What the preview should show: the map whose settings Mesh Map Settings shows, in front, in Bake mode, while the preview is on and no real bake runs
pub(crate) fn wanted(app: &TextureCraftApp) -> Option<LivePreviewTarget> {
    let set = app.session.ts();
    let settings = &app.session.doc.project.texture_sets.get(set)?.bake;
    let shows = app.ui.mode == Mode::Baking && app.ui.dock.is_visible(Panel::MeshMapSettings) && app.bake.is_none();
    (shows && app.ui.bake_visualization.map_preview).then(|| LivePreviewTarget { set, map: super::selected_map(app.ui.selected_baked_map, settings) })
}

/// A map setting changed: bake the preview again, small while a slider is dragged and at the chosen size otherwise
pub(crate) fn settings_changed(app: &mut TextureCraftApp, dragging: bool) {
    let Some(target) = app.live_preview.shown else { return };

    // Web builds bake on the UI thread, so they wait for the release
    if dragging && cfg!(target_arch = "wasm32") {
        app.live_preview.refine_on_release = true;
        return;
    }

    let size = if dragging { drag_size(target.map) } else { app.ui.bake_visualization.map_preview_size };
    app.live_preview.refine_on_release = dragging;
    start(app, target, size);
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
            show(app, target);
        }
    }

    if app.live_preview.refine_on_release && ctx.dragged_id().is_none() {
        app.live_preview.refine_on_release = false;
        if let Some(target) = app.live_preview.shown {
            start(app, target, app.ui.bake_visualization.map_preview_size);
        }
    }

    let Some(running) = &app.live_preview.running else { return };
    ctx.request_repaint_after(std::time::Duration::from_millis(16));
    match running.receiver.try_recv() {
        Ok(Ok(output)) => {
            let target = running.target;
            app.live_preview.running = None;
            if let Some(data) = output.maps.get(&target.map) {
                let streamed =
                    StreamedMap { map: target.map, index: 0, total: 1, resolution: output.resolution, height: output.height, data: Arc::new(data.clone()) };
                app.session.bake_preview_map(target.set, streamed);
            }
        }
        // A failed or cancelled preview keeps the picture it had
        Ok(Err(_)) | Err(TryRecvError::Disconnected) => app.live_preview.running = None,
        Err(TryRecvError::Empty) => {}
    }
}

/// Start showing `target`: the map's last real bake at once when there is one, else a preview bake at the chosen size
fn show(app: &mut TextureCraftApp, target: LivePreviewTarget) {
    app.session.bake_preview_begin(vec![(target.set, vec![target.map])]);
    app.live_preview.shown = Some(target);

    let baked = app.session.baked(target.set).and_then(|baked| baked.maps.get(&target.map).map(|data| (baked.resolution, data.clone())));
    match baked {
        Some((resolution, data)) => {
            let height = (data.len() / target.map.components()).checked_div(resolution as usize).unwrap_or(0) as u32;
            app.session.bake_preview_map(target.set, StreamedMap { map: target.map, index: 0, total: 1, resolution, height, data });
        }
        None => start(app, target, app.ui.bake_visualization.map_preview_size),
    }
}

/// Bake `target`'s map alone at `size` on a worker, cancelling the preview bake still running
fn start(app: &mut TextureCraftApp, target: LivePreviewTarget, size: BakeResolution) {
    app.live_preview.cancel();
    let Ok(mut task) = app.session.bake_task(target.set) else { return };

    task.settings.maps.select([target.map]);
    let own = task.settings.common.resolution;
    task.settings.common.resolution = if own.pixels() < size.pixels() { own } else { size };
    task.settings.common.resolution_height = None;

    let progress = Arc::new(Progress::new());
    let worker_progress = progress.clone();
    let receiver = crate::spawn_worker("texturecraft-map-preview", move || task.run(&worker_progress));
    app.live_preview.running = Some(RunningPreview { target, progress, receiver });
}

/// The size a dragged slider's previews bake at: 128, or 64 for the bakers that can't keep 128 under about 100 ms
fn drag_size(map: BakedMap) -> BakeResolution {
    match map {
        BakedMap::Thickness | BakedMap::BentNormal => BakeResolution::R64,
        _ => BakeResolution::R128,
    }
}
