use super::{RayFrame, VertexReach};
use crate::{BakeError, BakeRequest, MeshExtent};
use rayon::prelude::*;
use texturecraft_geom::glam::Vec3;
use texturecraft_geom::surface_at;

/// Where each high-poly vertex sits relative to the transfer rays of one texture set, to show which parts its bake misses
#[derive(Clone, Debug, Default)]
pub struct HighPolyReach {
    /// Per high-poly vertex; `None` when the set has no low-poly triangle or the request no high poly
    pub vertices: Vec<Option<VertexReach>>,
    /// World units per unit of the cage distance settings
    pub cage_units: f32,
    pub ignore_backface: bool,
}

impl HighPolyReach {
    /// The reach of `request`'s transfer rays, from the same ray starts the bake uses at each vertex's nearest low-poly point
    pub fn new(request: &BakeRequest) -> Result<HighPolyReach, BakeError> {
        let frame = RayFrame::new(request, MeshExtent::of(request.mesh)?)?;
        let Some(high) = request.high else { return Ok(HighPolyReach::default()) };

        let mut in_set = vec![false; request.mesh.triangle_count()];
        for triangle in request.mesh.triangles_of(request.material) {
            if let Some(flag) = in_set.get_mut(triangle) {
                *flag = true;
            }
        }

        let frame = &frame;
        let in_set = |triangle: u32| in_set.get(triangle as usize).copied().unwrap_or(false);
        let vertices = high
            .mesh
            .positions
            .par_iter()
            .enumerate()
            .map(|(vertex, &high_position)| {
                let nearest = request.bvh.nearest(high_position, f32::INFINITY, &in_set)?;
                let barycentric = [nearest.u, nearest.v];
                let surface = surface_at(request.mesh, nearest.tri, nearest.u, nearest.v)?;
                Some(VertexReach {
                    distance: nearest.t,
                    low_position: surface.pos,
                    start: frame.start(&surface, barycentric),
                    high_position,
                    high_normal: high.mesh.normals.get(vertex).copied().unwrap_or(Vec3::ZERO),
                })
            })
            .collect();

        Ok(HighPolyReach { vertices, cage_units: frame.cage_units, ignore_backface: request.settings.high_poly.ignore_backface })
    }

    /// For high-poly vertex `vertex`: its distance from this set's low poly and whether the transfer rays miss it at the `max_frontal` and `max_rear` settings
    pub fn vertex_missed(&self, vertex: usize, max_frontal: f32, max_rear: f32) -> Option<(f32, bool)> {
        let reach = self.vertices.get(vertex).copied().flatten()?;
        Some((reach.distance, reach.missed(max_frontal * self.cage_units, max_rear * self.cage_units, self.ignore_backface)))
    }
}
