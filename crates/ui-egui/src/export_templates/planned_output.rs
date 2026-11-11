/// A file an output writes for the current texture set: its name and size
#[derive(Clone)]
pub(super) struct PlannedOutput {
    pub(super) file: String,
    pub(super) size: [u64; 2],
}
