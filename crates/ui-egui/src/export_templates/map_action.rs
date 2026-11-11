use super::output_kind::OutputKind;

/// What the output cards ask for this frame
pub(super) enum MapAction {
    Add(OutputKind),
    Duplicate(usize),
    /// From, to
    Move(usize, usize),
    Remove(usize),
}
