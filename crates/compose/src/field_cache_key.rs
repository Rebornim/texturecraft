use std::sync::Weak;

use texturecraft_core::BlockScales;

/// What a field was built from: the working layout (the g-buffer's address, size, coverage fingerprint, UV tile block and tile scales), the mesh's positions and UVs, the mesh maps it reads and its settings' bits
#[derive(Clone)]
pub(crate) struct FieldCacheKey {
    pub(crate) layout: usize,
    pub(crate) size: (u32, u32),
    pub(crate) fingerprint: u64,
    pub(crate) block: (u32, u32),
    pub(crate) scales: BlockScales,
    pub(crate) mesh: (usize, usize, usize, usize),
    pub(crate) maps: Vec<Weak<Vec<f32>>>,
    pub(crate) settings: Vec<u32>,
}

impl PartialEq for FieldCacheKey {
    fn eq(&self, other: &FieldCacheKey) -> bool {
        let same_maps = self.maps.len() == other.maps.len() && self.maps.iter().zip(&other.maps).all(|(a, b)| a.strong_count() > 0 && a.ptr_eq(b));
        same_maps
            && self.layout == other.layout
            && self.size == other.size
            && self.fingerprint == other.fingerprint
            && self.block == other.block
            && self.scales == other.scales
            && self.mesh == other.mesh
            && self.settings == other.settings
    }
}
