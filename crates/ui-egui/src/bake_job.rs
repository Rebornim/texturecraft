use texturecraft_engine::core::BakedMap;

use crate::RunningBake;

/// A background bake of one or more texture sets.
pub struct BakeJob {
    pub queue: Vec<usize>,
    pub current: Option<RunningBake>,
    pub total: usize,
    /// The one map to bake (a row's quick bake) instead of the checked ones
    pub only: Option<BakedMap>,
}
