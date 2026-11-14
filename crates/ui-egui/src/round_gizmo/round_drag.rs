use egui::Pos2;

use super::{RoundGizmo, RoundGrab};

/// A projector handle being dragged (kept in egui's memory between frames)
#[derive(Clone, Debug)]
pub(super) struct RoundDrag {
    pub(super) grab: RoundGrab,
    pub(super) from: Pos2,
    pub(super) start: RoundGizmo,
    /// The drag made its undo step: later updates merge into it
    pub(super) stepped: bool,
}
