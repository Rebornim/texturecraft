//! A Move on model drag under way, kept in egui's memory between frames

use egui::Pos2;
use texturecraft_engine::core::Projection;

use crate::frame_gizmo::FrameGizmo;

/// A trackpad drag under way: where it started and the projection then
#[derive(Clone, Debug)]
pub(crate) struct TrackpadDrag {
    pub from: Pos2,
    pub start: Projection,
    /// The frame and its gizmo when the projection moves by its frame
    pub frame: Option<FrameGizmo>,
    pub turn: bool,
    pub stepped: bool,
}
