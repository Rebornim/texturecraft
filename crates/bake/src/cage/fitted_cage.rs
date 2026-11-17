use super::name_keys::NameKeys;
use crate::map::interpolate;
use crate::{BakeRequest, HighPoly, MeshExtent};
use rayon::prelude::*;
use std::collections::VecDeque;
use texturecraft_geom::Mesh;
use texturecraft_geom::glam::Vec3;

/// How much farther than the high poly the cage reaches, as a fraction of the fitted distance: the fit reads the high poly at its vertices only
const MARGIN_PER_DISTANCE: f32 = 0.1;

/// The least the cage reaches each way, as a fraction of the mesh diagonal, so a high poly lying on the low poly is still inside
const MARGIN_PER_DIAGONAL: f32 = 1e-3;

/// The automatic cage: how far the transfer rays start in front of each low-poly vertex and how far they run behind it, fitted to the high poly
pub(crate) struct FittedCage {
    /// (front, rear) in world units per low-poly vertex; `None` on parts no high poly is near
    distances: Vec<Option<(f32, f32)>>,
}

impl FittedCage {
    /// Fit the cage of `request`'s low poly around `high` along `ray_normals`: each low-poly vertex reaches the high poly straight in front of or behind it, and each high-poly vertex pushes the corners of its nearest low-poly triangle out (or in) to its height. Vertices neither finds take their neighbours' distances; a part the high poly reaches nowhere gets none
    pub(crate) fn new(request: &BakeRequest, high: HighPoly, welded_indices: &[u32], ray_normals: &[Vec3], extent: MeshExtent) -> FittedCage {
        let low = request.mesh;
        let keys = NameKeys::of(request, high);
        let welded = |vertex: usize| welded_indices.get(vertex).copied().unwrap_or(0) as usize;
        let welded_count = welded_indices.iter().max().map_or(0, |last| *last as usize + 1);
        let mut reach: Vec<Option<(f32, f32)>> = vec![None; welded_count];
        let mut widen = |vertex: usize, height: f32| {
            if let Some(distances) = reach.get_mut(welded(vertex)) {
                let (front, rear) = distances.unwrap_or((0.0, 0.0));
                *distances = Some((front.max(height), rear.max(-height)));
            }
        };

        for (vertex, height) in heights_at_low_vertices(low, high, ray_normals, &keys, extent.diagonal).into_iter().enumerate() {
            if let Some(height) = height {
                widen(vertex, height);
            }
        }
        for (triangle, height) in heights_of_high_vertices(request, high, ray_normals, &keys).into_iter().flatten() {
            for corner in low.tri(triangle).into_iter().flatten() {
                widen(corner, height);
            }
        }
        spread_to_neighbours(&mut reach, low.indices.as_chunks::<3>().0.iter().map(|corners| corners.map(|corner| welded(corner as usize))));

        let floor = MARGIN_PER_DIAGONAL * extent.diagonal;
        let padded = |distance: f32| distance * (1.0 + MARGIN_PER_DISTANCE) + floor;
        let distances =
            (0..low.positions.len()).map(|vertex| reach.get(welded(vertex)).copied().flatten().map(|(front, rear)| (padded(front), padded(rear)))).collect();

        FittedCage { distances }
    }

    /// The (front, rear) distances at `barycentric` of triangle `triangle` of the low poly `low` the cage was fitted to; `None` where the high poly reaches no corner
    pub(crate) fn distances(&self, low: &Mesh, triangle: u32, barycentric: [f32; 2]) -> Option<(f32, f32)> {
        let corners = low.tri(triangle as usize)?;
        let [first, second, third] = [0, 1, 2].map(|corner| self.distances.get(corners[corner]).copied().flatten());
        let ((first_front, first_rear), (second_front, second_rear), (third_front, third_rear)) = (first?, second?, third?);
        let [second_weight, third_weight] = barycentric;
        let first_weight = 1.0 - second_weight - third_weight;

        Some((
            first_front * first_weight + second_front * second_weight + third_front * third_weight,
            first_rear * first_weight + second_rear * second_weight + third_rear * third_weight,
        ))
    }
}

/// Per low-poly vertex, how far along its ray normal the nearest high-poly surface in front of it (positive) or behind it (negative) is, within `max_distance`
fn heights_at_low_vertices(low: &Mesh, high: HighPoly, ray_normals: &[Vec3], keys: &NameKeys, max_distance: f32) -> Vec<Option<f32>> {
    low.positions
        .par_iter()
        .zip(ray_normals)
        .enumerate()
        .map(|(vertex, (&position, &up))| {
            let low_key = keys.low_vertices.get(vertex);
            let rays_reach = |triangle: u32| NameKeys::matches(low_key, keys.high_triangles.get(triangle as usize));
            let distance = |direction: Vec3| high.bvh.closest_filtered(position, direction, max_distance, &rays_reach).map(|hit| hit.t);
            match (distance(up), distance(-up)) {
                (Some(front), Some(rear)) => Some(if front <= rear { front } else { -rear }),
                (Some(front), None) => Some(front),
                (None, Some(rear)) => Some(-rear),
                (None, None) => None,
            }
        })
        .collect()
}

/// Per high-poly vertex, its nearest low-poly triangle and its height over it along the ray normal there
fn heights_of_high_vertices(request: &BakeRequest, high: HighPoly, ray_normals: &[Vec3], keys: &NameKeys) -> Vec<Option<(usize, f32)>> {
    let low = request.mesh;
    high.mesh
        .positions
        .par_iter()
        .enumerate()
        .map(|(vertex, &high_position)| {
            let high_key = keys.high_vertices.get(vertex);
            let rays_reach = |triangle: u32| NameKeys::matches(keys.low_triangles.get(triangle as usize), high_key);
            let nearest = request.bvh.nearest(high_position, f32::INFINITY, &rays_reach)?;
            let barycentric = [nearest.u, nearest.v];
            let low_position = interpolate(low, &low.positions, nearest.tri, barycentric)?;
            let up = interpolate(low, ray_normals, nearest.tri, barycentric)?.try_normalize()?;
            Some((nearest.tri as usize, (high_position - low_position).dot(up)))
        })
        .collect()
}

/// Give each vertex without distances the largest of its nearest fitted neighbours', walking outward along the edges of `triangles`
fn spread_to_neighbours(reach: &mut [Option<(f32, f32)>], triangles: impl Iterator<Item = [usize; 3]>) {
    let mut neighbours = vec![Vec::new(); reach.len()];
    for corners in triangles {
        for (corner, next) in [(0, 1), (1, 2), (2, 0)] {
            if let (true, Some(list)) = (corners[next] < reach.len(), neighbours.get_mut(corners[corner])) {
                list.push(corners[next]);
            }
            if let (true, Some(list)) = (corners[corner] < reach.len(), neighbours.get_mut(corners[next])) {
                list.push(corners[corner]);
            }
        }
    }

    // Steps from the nearest fitted vertex; a vertex only takes distances from vertices one step nearer
    let mut steps: Vec<Option<u32>> = reach.iter().map(|distances| distances.map(|_| 0)).collect();
    let mut queue: VecDeque<usize> = (0..reach.len()).filter(|vertex| reach[*vertex].is_some()).collect();
    while let Some(vertex) = queue.pop_front() {
        let (Some((front, rear)), Some(step)) = (reach[vertex], steps[vertex]) else { continue };

        for &neighbour in &neighbours[vertex] {
            match steps[neighbour] {
                None => {
                    steps[neighbour] = Some(step + 1);
                    reach[neighbour] = Some((front, rear));
                    queue.push_back(neighbour);
                }
                Some(neighbour_step) if neighbour_step == step + 1 => {
                    let (neighbour_front, neighbour_rear) = reach[neighbour].unwrap_or((0.0, 0.0));
                    reach[neighbour] = Some((neighbour_front.max(front), neighbour_rear.max(rear)));
                }
                Some(_) => {}
            }
        }
    }
}
