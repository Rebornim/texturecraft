/// What the preset list asks for this frame
pub(super) enum ListAction {
    Select(String),
    New,
    Duplicate,
    /// From a preset's right-click menu
    DuplicateNamed(String),
    StartRename(String),
    Rename(String),
    Remove,
    RemoveNamed(String),
    Import,
    Export,
}
