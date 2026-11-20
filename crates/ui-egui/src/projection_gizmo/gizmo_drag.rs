use egui::Pos2;

use super::Gizmo;

/// A handle of a projection gizmo being dragged (kept in egui's memory between frames)
#[derive(Clone, Debug)]
pub(crate) struct GizmoDrag<G: Gizmo> {
    pub(crate) grab: G::Grab,
    /// Where the drag started and the gizmo then
    pub(crate) from: Pos2,
    pub(crate) start: G,
    /// The session revision the drag's last edit left: later updates merge into its undo step while the history is still there
    pub(crate) edited_revision: Option<u64>,
}
