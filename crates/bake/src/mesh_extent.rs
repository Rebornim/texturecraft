use crate::BakeError;
use texturecraft_geom::Mesh;
use texturecraft_geom::glam::Vec3;

/// The low-poly mesh's size, which scales the bake's relative distances
#[derive(Clone, Copy, Debug)]
pub(crate) struct MeshExtent {
    pub(crate) bounds_min: Vec3,
    pub(crate) diagonal: f32,
    /// The bounding box's largest side
    pub(crate) scale: f32,
}

impl MeshExtent {
    pub(crate) fn of(mesh: &Mesh) -> Result<MeshExtent, BakeError> {
        let (bounds_min, bounds_max) = mesh.bounds();
        let diagonal = (bounds_max - bounds_min).length().max(1e-6);
        if !diagonal.is_finite() {
            return Err(BakeError::Invalid("mesh bounds are too large".into()));
        }

        Ok(MeshExtent { bounds_min, diagonal, scale: (bounds_max - bounds_min).max_element().max(1e-6) })
    }

    /// How far a ray starts past the surface it leaves
    pub(crate) fn ray_epsilon(&self) -> f32 {
        1e-6 * self.diagonal
    }
}
