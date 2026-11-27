//! What [`super::press_edit`] keeps of an editor's last change

/// The press an editor's last change belonged to, and the frame it was seen in
#[derive(Clone, Copy)]
pub(super) struct PressEdit {
    pub(super) press: u64,
    pub(super) frame: u64,
}
