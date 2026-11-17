//! The lower envelope of parabolas that turns a line of squared distances into each place's squared distance to the nearest feature

/// The lower envelope of the parabolas over a line of squared distances, which gives each place's squared distance to the nearest feature along it and the line before
#[derive(Default)]
pub(crate) struct DistanceEnvelope {
    /// The parabolas still lowest somewhere: their vertex and where their stretch begins
    parabolas: Vec<(usize, f32)>,
    /// The line as it was
    heights: Vec<f32>,
}

impl DistanceEnvelope {
    /// Replace `line` with each place's squared distance to the nearest feature
    pub(crate) fn lower(&mut self, line: &mut [f32]) {
        self.heights.clear();
        self.heights.extend_from_slice(line);
        self.parabolas.clear();
        let heights = &self.heights;

        // Squared in f64, where it is exact, so a wide line cannot overflow a 32-bit usize
        let lifted = |at: usize| heights.get(at).copied().unwrap_or(f32::MAX) + (at as f64 * at as f64) as f32;
        for at in 0..heights.len() {
            let mut start = f32::NEG_INFINITY;
            while let Some(&(vertex, from)) = self.parabolas.last() {
                let crossing = (lifted(at) - lifted(vertex)) / (2.0 * (at - vertex) as f32);
                if crossing <= from {
                    self.parabolas.pop();
                } else {
                    start = crossing;
                    break;
                }
            }
            self.parabolas.push((at, start));
        }

        let mut current = 0;
        for (at, out) in line.iter_mut().enumerate() {
            while self.parabolas.get(current + 1).is_some_and(|(_, from)| *from < at as f32) {
                current += 1;
            }
            if let Some(&(vertex, _)) = self.parabolas.get(current) {
                *out = (at as f32 - vertex as f32).powi(2) + heights.get(vertex).copied().unwrap_or(f32::MAX);
            }
        }
    }
}
