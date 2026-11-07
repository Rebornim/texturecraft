use super::{RayFrame, VertexReach};
use crate::{BakeError, BakeRequest, MeshExtent};
use rayon::prelude::*;
use texturecraft_geom::glam::Vec3;
use texturecraft_geom::{Mesh, surface_at};

/// Where each high-poly vertex sits relative to the transfer rays of one texture set, to show which parts its bake misses
#[derive(Clone, Debug, Default)]
pub struct HighPolyReach {
    /// Per high-poly vertex; `None` where another texture set's low poly is nearer or the request has no high poly
    pub vertices: Vec<Option<VertexReach>>,
    /// World units per unit of the cage distance settings
    pub cage_units: f32,
    pub ignore_backface: bool,
}

impl HighPolyReach {
    /// The reach of `request`'s transfer rays at the high-poly vertices whose nearest low-poly point is on this set, from the same ray starts the bake uses there
    pub fn new(request: &BakeRequest) -> Result<HighPolyReach, BakeError> {
        let Some(high) = request.high else { return Ok(HighPolyReach::default()) };

        let frame = RayFrame::new(request, MeshExtent::of(request.mesh)?)?;
        let mut in_set = vec![false; request.mesh.triangle_count()];
        for triangle in request.mesh.triangles_of(request.material) {
            if let Some(flag) = in_set.get_mut(triangle) {
                *flag = true;
            }
        }

        let winding_normals = winding_normals(high.mesh);
        let frame = &frame;
        let vertices = high
            .mesh
            .positions
            .par_iter()
            .zip(&winding_normals)
            .map(|(&high_position, &high_facing)| {
                let nearest = request.bvh.nearest(high_position, f32::INFINITY, &|_| true)?;
                if !in_set.get(nearest.tri as usize).copied().unwrap_or(false) {
                    return None;
                }

                let surface = surface_at(request.mesh, nearest.tri, nearest.u, nearest.v)?;
                Some(VertexReach { low_position: surface.pos, start: frame.start(&surface, [nearest.u, nearest.v]), high_position, high_facing })
            })
            .collect();

        Ok(HighPolyReach { vertices, cage_units: frame.cage_units, ignore_backface: request.settings.high_poly.ignore_backface })
    }

    /// Whether the transfer rays miss high-poly vertex `vertex` at the `max_frontal` and `max_rear` settings; `None` when the vertex isn't this set's
    pub fn missed(&self, vertex: usize, max_frontal: f32, max_rear: f32) -> Option<bool> {
        let reach = self.vertices.get(vertex).copied().flatten()?;
        Some(reach.missed(max_frontal * self.cage_units, max_rear * self.cage_units, self.ignore_backface))
    }
}

/// Each vertex's sum of its triangles' winding normals, weighted by area: the side the bake's backface test sees as the front
fn winding_normals(mesh: &Mesh) -> Vec<Vec3> {
    let mut normals = vec![Vec3::ZERO; mesh.positions.len()];
    for corners in mesh.indices.chunks_exact(3) {
        let position = |corner: u32| mesh.positions.get(corner as usize).copied().unwrap_or(Vec3::ZERO);
        let winding_normal = (position(corners[1]) - position(corners[0])).cross(position(corners[2]) - position(corners[0]));
        for &corner in corners {
            if let Some(normal) = normals.get_mut(corner as usize) {
                *normal += winding_normal;
            }
        }
    }
    normals
}
