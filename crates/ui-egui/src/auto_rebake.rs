//! Auto-rebake: a texture set's auto-rebake map is baked again in the background once what it is baked from has stopped changing

use texturecraft_engine::core::{BakeSource, BakedMap};

use crate::{BakeJob, TextureCraftApp, bake_report, state};

/// How long what an out-of-date auto-rebake map is baked from must stay the same before the rebake starts
const SETTLE_SECONDS: f64 = 0.5;

/// An out-of-date auto-rebake map: set, map, what a bake reads now, and the newest stroke when only painting changed it
pub type Due = (usize, BakedMap, BakeSource, Option<u64>);

/// What the app has seen of the out-of-date auto-rebake maps
#[derive(Default)]
pub struct AutoRebake {
    /// The due maps and the frame time they were first seen as they are
    pending: Option<(Vec<Due>, f64)>,
    /// Rebakes started that are still due: one that failed or was canceled is not tried again until something changes
    tried: Vec<Due>,
}

/// Start, or restart, the rebake of an out-of-date auto-rebake map; only in Bake mode, where its settings are changed
pub(crate) fn poll(app: &mut TextureCraftApp, ctx: &egui::Context) {
    if app.ui.mode != state::Mode::Baking {
        app.auto_rebake.pending = None;
        return;
    }

    let stale = app.session.auto_rebake_due().into_iter().map(|(set, map, source)| (set, map, source, None));
    let painted = app.session.auto_rebake_painted().into_iter().map(|(set, map, source, stroke)| (set, map, source, Some(stroke)));
    let due: Vec<Due> = stale.chain(painted).collect();
    if let Some(job) = &app.bake {
        // Newest wins: a rebake whose map changed again while it ran starts over with the new settings
        if job.rebake.as_ref().is_some_and(|rebake| !due.contains(rebake)) {
            app.cancel_bake();
        }
        return;
    }

    app.auto_rebake.tried.retain(|tried| due.contains(tried));
    let waiting: Vec<Due> = due.into_iter().filter(|due| !app.auto_rebake.tried.contains(due)).collect();
    if waiting.is_empty() {
        app.auto_rebake.pending = None;
        return;
    }

    let now = app.last_time;
    ctx.request_repaint_after(std::time::Duration::from_secs_f64(SETTLE_SECONDS));

    // A held press (a slider dragged and paused) is still editing: the wait starts once it is let go
    if ctx.input(|input| input.pointer.any_down()) {
        app.auto_rebake.pending = Some((waiting, now));
        return;
    }
    match &app.auto_rebake.pending {
        Some((seen, since)) if *seen == waiting && now - since >= SETTLE_SECONDS => {}
        Some((seen, _)) if *seen == waiting => return,
        _ => {
            app.auto_rebake.pending = Some((waiting, now));
            return;
        }
    }

    app.auto_rebake.pending = None;
    if let Some(rebake) = waiting.first().copied() {
        app.auto_rebake.tried.push(rebake);
        start(app, rebake);
    }
}

/// Rebake only the map of `rebake`'s texture set in the background, shown in the Baking Log like any bake
fn start(app: &mut TextureCraftApp, rebake: Due) {
    let (set, map, ..) = rebake;

    // Restore the previous view before the rebake takes it over
    app.live_preview.release(&mut app.session);
    crate::id_color_pick::IdColorPick::end(app);

    // The progress footer names the maps this rebake makes, not every checked map of the set
    let maps = app.session.auto_rebake_maps(set, map);
    app.session.bake_preview_begin(vec![(set, maps.clone())]);
    app.session.begin_map_bake_log(&[set], &maps);
    app.bake_report = bake_report::BakeReport { auto_rebake: true, ..bake_report::BakeReport::started(app.last_time) };
    app.bake = Some(BakeJob { queue: vec![set], current: None, total: 1, mesh_id: app.session.mesh_id, only: None, rebake: Some(rebake) });
}
