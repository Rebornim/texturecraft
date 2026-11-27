/// What one texture set row shows, read before drawing so commands can run while the rows draw
pub(crate) struct SetRow {
    pub(crate) name: String,
    pub(crate) description: String,
    /// The mesh material's name, when the set has one to go back to
    pub(crate) original_name: Option<String>,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) visible: bool,
    /// UDIM and resolution of each UV tile, in UDIM order
    pub(crate) tiles: Vec<(u32, u32)>,
}
