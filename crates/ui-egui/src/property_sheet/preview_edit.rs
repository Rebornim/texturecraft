/// What a preview block's controls changed this frame: `Some(merge)` per edited part
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PreviewEdit {
    pub(crate) size: Option<bool>,
    pub(crate) lower: Option<bool>,
}
