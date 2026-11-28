/// The memory limits PSD planning holds an export to: a browser tab's address space needs them, a native export renders one layer at a time without them
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct PsdBudget {
    /// Pixel buffers of one document, including the renderer's padding and encoder
    pub(super) working_bytes: Option<u64>,
    /// Encoded documents one batch holds in memory, with headroom for a document's working buffers and ZIP packaging
    pub(super) batch_bytes: Option<u64>,
}

impl PsdBudget {
    /// A browser tab's: the export and the app share its 4 GiB address space
    pub(super) const WEB: Self = Self { working_bytes: Some(2 * 1024 * 1024 * 1024), batch_bytes: Some(1024 * 1024 * 1024) };
    /// A desktop export's: the machine bounds it, as it does every other file type
    pub(super) const NATIVE: Self = Self { working_bytes: None, batch_bytes: None };
    /// The budget of the platform this build runs on
    pub(super) const TARGET: Self = if cfg!(target_arch = "wasm32") { Self::WEB } else { Self::NATIVE };
}
