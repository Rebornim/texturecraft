//! Where a mirror plane cuts the mesh, kept between frames

use std::sync::Arc;

use texturecraft_engine::geom::Mesh;
use texturecraft_engine::geom::glam::Vec3;

/// The segments where the plane `p[axis] = position` crosses the mesh's triangles
#[derive(Clone, Debug)]
pub(crate) struct SymmetryIntersection {
    mesh_id: u64,
    axis: usize,
    position: f32,
    pub(crate) segments: Arc<Vec<[Vec3; 2]>>,
}

impl SymmetryIntersection {
    pub(crate) fn compute(mesh: &Mesh, mesh_id: u64, axis: usize, position: f32) -> Self {
        let mut segments = Vec::new();
        for triangle in 0..mesh.triangle_count() {
            let Some([a, b, c]) = mesh.tri(triangle) else { continue };
            let (Some(&a), Some(&b), Some(&c)) = (mesh.positions.get(a), mesh.positions.get(b), mesh.positions.get(c)) else {
                continue;
            };

            let corners = [a, b, c];

            // Each edge whose ends lie on opposite sides crosses the plane once
            let side = |i: usize| corners[i][axis] - position;
            let mut crossings = [Vec3::ZERO; 2];
            let mut count = 0;
            for (a, b) in [(0, 1), (1, 2), (2, 0)] {
                if (side(a) < 0.0) != (side(b) < 0.0) && count < crossings.len() {
                    crossings[count] = corners[a].lerp(corners[b], side(a) / (side(a) - side(b)));
                    count += 1;
                }
            }
            if count == crossings.len() {
                segments.push(crossings);
            }
        }
        SymmetryIntersection { mesh_id, axis, position, segments: Arc::new(segments) }
    }

    pub(crate) fn is_for(&self, mesh_id: u64, axis: usize, position: f32) -> bool {
        self.mesh_id == mesh_id && self.axis == axis && self.position == position
    }
}
