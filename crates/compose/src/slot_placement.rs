/// Where a generator's image slot is read: `scale` times across the UV square (or the mesh's bounds, triplanar), plus `offset`
pub(crate) struct SlotPlacement {
    pub scale: f32,
    pub offset: f32,
    /// How sharply the three projections meet, or None to read in UV
    pub triplanar_contrast: Option<f32>,
}
