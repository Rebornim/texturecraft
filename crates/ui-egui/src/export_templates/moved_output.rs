/// Drag payload of an output's handle: its index, to move it where it's dropped
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct MovedOutput(pub(super) usize);
