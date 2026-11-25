//! Auto-rebake: a texture set's auto-rebake map is baked again in the background once what it is baked from has stopped changing

use texturecraft_engine::core::{BakeSource, BakedMap};

use crate::{BakeJob, TextureCraftApp, bake_report, state};

/// How long what an out-of-date auto-rebake map is baked from must stay the same before the rebake starts
const SETTLE_SECONDS: f64 = 0.5;

/// An out-of-date auto-rebake map: its texture set, the map and what a bake would read now
pub type Due = (usize, BakedMap, BakeSource);

/// What the app has seen of the out-of-date auto-rebake maps
#[derive(Default)]
pub struct AutoRebake {
    /// The due maps and the frame time they were first seen as they are
    pending: Option<(Vec<Due>, f64)>,
    /// The due maps when the last rebake started: a failed or canceled rebake is not tried again until something changes
    started: Vec<Due>,
}

/// Start, or restart, the rebake of an out-of-date auto-rebake map; only in Bake mode, where its settings are changed
pub(crate) fn poll(app: &mut TextureCraftApp, ctx: &egui::Context) {
    if app.ui.mode != state::Mode::Baking {
        app.auto_rebake.pending = None;
        return;
    }

    let due = app.session.auto_rebake_due();
    if let Some(job) = &app.bake {
        // Newest wins: a rebake whose map changed again while it ran starts over with the new settings
        if job.rebake.is_some_and(|rebake| !due.contains(&rebake)) {
            app.cancel_bake();
        }
        return;
    }

    if due.is_empty() || due == app.auto_rebake.started {
        app.auto_rebake.pending = None;
        return;
    }

    let now = app.last_time;
    ctx.request_repaint_after(std::time::Duration::from_secs_f64(SETTLE_SECONDS));
    match &app.auto_rebake.pending {
        Some((seen, since)) if *seen == due && now - since >= SETTLE_SECONDS => {}
        Some((seen, _)) if *seen == due => return,
        _ => {
            app.auto_rebake.pending = Some((due, now));
            return;
        }
    }

    app.auto_rebake.pending = None;
    app.auto_rebake.started = due.clone();
    if let Some(rebake) = due.first().copied() {
        start(app, rebake);
    }
}

/// Rebake only the map of `rebake`'s texture set in the background, shown in the Baking Log like any bake
fn start(app: &mut TextureCraftApp, rebake: Due) {
    let (set, map, _) = rebake;
    let orders = match app.session.bake_preview_orders(&[set], Some(map)) {
        Ok(orders) => orders,
        Err(error) => {
            app.toast(error.to_string(), true);
            return;
        }
    };

    // Restore the previous view before the rebake takes it over
    app.live_preview.release(&mut app.session);
    crate::id_color_pick::IdColorPick::end(app);
    app.session.bake_preview_begin(orders);
    app.session.begin_map_bake_log(&[set], map);
    app.bake_report = bake_report::BakeReport { auto_rebake: true, ..bake_report::BakeReport::started(app.last_time) };
    app.bake = Some(BakeJob { queue: vec![set], current: None, total: 1, mesh_id: app.session.mesh_id, only: Some(map), rebake: Some(rebake) });
}
