use std::time::Duration;

/// Timings of one measured step, reported as p50 / p95 / worst
#[derive(Default)]
pub struct DurationSamples(Vec<Duration>);

impl DurationSamples {
    pub fn push(&mut self, sample: Duration) {
        self.0.push(sample);
    }

    pub fn append(&mut self, other: &DurationSamples) {
        self.0.extend_from_slice(&other.0);
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn max(&self) -> Duration {
        self.0.iter().copied().max().unwrap_or_default()
    }

    pub fn total(&self) -> Duration {
        self.0.iter().sum()
    }

    /// Nearest-rank percentile, `rank` in 0..=1
    pub fn percentile(&self, rank: f64) -> Duration {
        let mut sorted = self.0.clone();
        sorted.sort();
        let index = ((rank * sorted.len() as f64).ceil() as usize).clamp(1, sorted.len().max(1)) - 1;
        sorted.get(index).copied().unwrap_or_default()
    }

    /// One `fixture | action | metric | p50 | p95 | worst | n` line, in milliseconds
    pub fn report(&self, fixture: &str, action: &str, metric: &str) {
        let ms = |d: Duration| d.as_secs_f64() * 1000.0;
        println!(
            "{fixture} | {action} | {metric} | p50 {:.2} ms | p95 {:.2} ms | worst {:.2} ms | n {}",
            ms(self.percentile(0.5)),
            ms(self.percentile(0.95)),
            ms(self.max()),
            self.len()
        );
    }
}
