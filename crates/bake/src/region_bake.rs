use std::collections::BTreeMap;
use std::sync::Arc;
use texturecraft_core::BakedMap;

use crate::TexelBox;

/// A bake that samples only `texels` and takes every other texel from `previous`, a whole bake of the same size
#[derive(Clone, Copy, Debug)]
pub struct RegionBake<'a> {
    pub texels: TexelBox,
    pub previous: &'a BTreeMap<BakedMap, Arc<Vec<f32>>>,
}
