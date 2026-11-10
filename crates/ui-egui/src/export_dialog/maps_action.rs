/// The Output maps menu's choices
#[derive(Clone, Copy, PartialEq)]
pub(super) enum MapsAction {
    Check,
    Uncheck,
    Invert,
    Override,
    Inherit,
}
