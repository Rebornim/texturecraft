use std::time::{Duration, Instant};

use texturecraft_engine::Session;

use super::DurationSamples;

/// The UI's compositing slice per frame (`COMPOSITE_BUDGET_MS` in `ui-egui/src/gpu.rs`)
const FRAME_BUDGET_MS: f64 = 30.0;

/// The UI frames one change takes until nothing is left to composite
#[derive(Default)]
pub struct FrameTimings {
    /// From the first frame until no tile is pending
    pub total: Duration,
    pub compose: DurationSamples,
    pub display: DurationSamples,
    /// Compose plus display, per frame
    pub frame: DurationSamples,
}

impl FrameTimings {
    /// Frames as the UI runs them (a budgeted compositing slice, then the display conversion) until `session` has no pending tile
    pub fn until_settled(session: &mut Session) -> FrameTimings {
        let mut timings = FrameTimings::default();
        let clock = Instant::now();
        let elapsed_ms = || clock.elapsed().as_secs_f64() * 1000.0;
        while session.pending_tiles() > 0 {
            let started = Instant::now();
            session.update_for(FRAME_BUDGET_MS, elapsed_ms);
            let compose = started.elapsed();
            let display = super::display(session);

            timings.compose.push(compose);
            timings.display.push(display);
            timings.frame.push(compose + display);
        }
        timings.total = clock.elapsed();
        timings
    }

    /// Adds `other`'s frames; its total goes to `totals`
    pub fn absorb(&mut self, other: FrameTimings, totals: &mut DurationSamples) {
        self.compose.append(&other.compose);
        self.display.append(&other.display);
        self.frame.append(&other.frame);
        totals.push(other.total);
    }

    /// The per-frame lines of these timings
    pub fn report(&self, fixture: &str, action: &str) {
        self.compose.report(fixture, action, "frame compose");
        self.display.report(fixture, action, "frame display");
        self.frame.report(fixture, action, "frame total");
    }
}
