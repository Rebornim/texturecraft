use texturecraft_engine::log::{LogEntry, SessionLog};

use super::{LogChannel, channel_of};

/// The widest line of each Log tab, measuring each line once, when it is new
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LogWidthCache {
    log_id: u64,
    pixels_per_point: f32,
    /// The newest measured line's occurrence id (ids are never reused within a log)
    last_occurrence: u64,
    /// How many lines of both tabs were measured: fewer lines in the log means some were cleared
    measured: usize,
    widest: [f32; LogChannel::ALL.len()],
}

impl LogWidthCache {
    /// The widest line of `channel`: new lines are measured, a cleared line or a new log or scale measures all again
    pub fn widest(&mut self, log: &SessionLog, log_id: u64, pixels_per_point: f32, channel: LogChannel, measure: impl Fn(&LogEntry) -> f32) -> f32 {
        let added = log.entries_after(self.last_occurrence).count();
        if self.log_id != log_id || self.pixels_per_point != pixels_per_point || self.measured + added != log.len() {
            *self = LogWidthCache { log_id, pixels_per_point, ..LogWidthCache::default() };
        }

        for (occurrence, entry) in log.entries_after(self.last_occurrence) {
            let widest = &mut self.widest[channel_of(entry) as usize];
            *widest = widest.max(measure(entry));
            self.last_occurrence = occurrence;
            self.measured += 1;
        }
        self.widest[channel as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn width(entry: &LogEntry) -> f32 {
        entry.line().len() as f32
    }

    #[test]
    fn clearing_the_widest_line_narrows_the_tab() {
        let mut log = SessionLog::new();
        log.info("Project", "short");
        log.info("Project", "a much longer line than the other one");
        let mut cache = LogWidthCache::default();
        let wide = cache.widest(&log, 1, 1.0, LogChannel::General, width);

        log.retain(|entry| !entry.line().contains("longer"));
        assert!(cache.widest(&log, 1, 1.0, LogChannel::General, width) < wide);
    }

    #[test]
    fn a_new_log_is_measured_again() {
        let mut log = SessionLog::new();
        log.info("Project", "a much longer line than the other one");
        let mut cache = LogWidthCache::default();
        let wide = cache.widest(&log, 1, 1.0, LogChannel::General, width);

        let mut next = SessionLog::new();
        next.info("Project", "short");
        assert!(cache.widest(&next, 2, 1.0, LogChannel::General, width) < wide);
    }
}
