/// What a bottom bar button asks for
pub(super) enum BottomAction {
    Save,
    Export,
    Cancel,
    /// Stop the running export after the texture set it is on
    #[cfg(not(target_arch = "wasm32"))]
    CancelExport,
}
