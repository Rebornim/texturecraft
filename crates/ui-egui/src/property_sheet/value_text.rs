//! Where a slider's value sits and how it reads

/// A value drawn right-aligned at `right` in a `width` wide box centred on `center_y`, reading `shown`; typing may end in `suffix`
pub(super) struct ValueText<'a> {
    pub(super) right: f32,
    pub(super) width: f32,
    pub(super) center_y: f32,
    pub(super) shown: &'a str,
    pub(super) suffix: &'a str,
}
