//! What the drag badge says about the place under the pointer

/// Whether the place under the pointer takes the dragged asset
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShelfDragVerdict {
    /// A drop here does something: a crosshair on a light square
    Valid,
    /// A drop here does nothing: a no-entry sign on red
    NotValid,
}
