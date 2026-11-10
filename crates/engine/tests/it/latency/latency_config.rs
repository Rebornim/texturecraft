/// Sizes and sample counts of the latency benches
pub struct LatencyConfig {
    /// The default material ball's texture size
    pub ball_resolution: u32,
    /// Bake size of the mesh maps the ball's smart materials read
    pub ball_maps_resolution: u32,
    /// Texture size of the procedural test stack
    pub stack_resolution: u32,
    /// Bake size of the bake bench
    pub bake_resolution: u32,
    /// Frames per stroke, steps per slider drag, toggles per visibility direction, edits, undos
    pub samples: usize,
    /// Repeats of the long actions: strokes, bakes, each row of the effect cost table
    pub repeats: usize,
}

impl LatencyConfig {
    /// The measured setup; `TC_LATENCY_SAMPLES` overrides the sample count
    pub fn full() -> LatencyConfig {
        let samples = std::env::var("TC_LATENCY_SAMPLES").ok().and_then(|value| value.parse().ok()).unwrap_or(30);
        LatencyConfig { ball_resolution: 2048, ball_maps_resolution: 512, stack_resolution: 4096, bake_resolution: 2048, samples, repeats: 3 }
    }

    /// Tiny sizes for the smoke test: proves every bench body still runs
    pub fn smoke() -> LatencyConfig {
        LatencyConfig { ball_resolution: 128, ball_maps_resolution: 64, stack_resolution: 128, bake_resolution: 64, samples: 2, repeats: 1 }
    }
}
