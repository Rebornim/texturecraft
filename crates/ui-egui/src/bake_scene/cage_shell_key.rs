use std::collections::BTreeMap;
use std::sync::Arc;

use texturecraft_engine::Session;
use texturecraft_engine::core::{BakePaintTarget, CageType, SkewSettings};

/// Everything a texture set's cage shell is built from except the distances, which only move its corners
#[derive(Clone, Debug, PartialEq)]
pub(super) struct CageShellKey {
    mesh_id: u64,
    /// Identity of the merged high poly
    high: usize,
    /// Identity of the cage file
    cage_file: usize,
    material: usize,
    uv_tiles: Vec<u32>,
    tile_resolutions: BTreeMap<u32, u32>,
    cage_kind: CageType,
    relative_to_bounding_box: bool,
    average_normals: bool,
    use_low_as_high: bool,
    skew: SkewSettings,
    /// Identity of the painted skew and cage maps
    paint: [usize; 2],
}

impl CageShellKey {
    pub(super) fn of(session: &Session, ts: usize) -> Option<CageShellKey> {
        let set = session.doc.project.texture_sets.get(ts)?;
        let identity = |pointer: Option<*const ()>| pointer.map_or(0, |pointer| pointer as usize);
        let paint = |target| identity(session.doc.bake_paint.get(&(ts, target)).map(|map| Arc::as_ptr(map).cast()));
        Some(CageShellKey {
            mesh_id: session.mesh_id,
            high: identity(session.bake_meshes.merged_high().map(|high| Arc::as_ptr(&high.mesh).cast())),
            cage_file: identity(session.bake_meshes.cage.as_ref().map(|cage| Arc::as_ptr(&cage.mesh).cast())),
            material: set.material,
            uv_tiles: set.uv_tiles.clone(),
            tile_resolutions: set.tile_resolutions.clone(),
            cage_kind: set.bake.cage.kind,
            relative_to_bounding_box: set.bake.cage.relative_to_bounding_box,
            average_normals: set.bake.cage.average_normals,
            use_low_as_high: set.bake.high_poly.use_low_as_high,
            skew: set.bake.skew,
            paint: [paint(BakePaintTarget::Skew), paint(BakePaintTarget::Cage)],
        })
    }
}
