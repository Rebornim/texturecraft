use egui::Pos2;
use texturecraft_engine::core::LayerId;

use super::{FrameGizmo, FrameGrab};

/// A frame handle being dragged (kept in egui's memory between frames)
#[derive(Clone, Copy, Debug)]
pub(super) struct FrameDrag {
    pub(super) layer: LayerId,
    pub(super) effect: Option<LayerId>,
    pub(super) grab: FrameGrab,
    /// Where the drag started and the frame then
    pub(super) from: Pos2,
    pub(super) start: FrameGizmo,
    /// The drag made its undo step: later updates merge into it
    pub(super) stepped: bool,
}
