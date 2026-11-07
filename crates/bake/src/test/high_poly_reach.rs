use super::settings;
use crate::map::BakeContext;
use crate::{BakeRequest, HighPoly, HighPolyReach, RayStart};
use texturecraft_core::BakedMap;
use texturecraft_geom::{Bvh, primitives, surface_at};

/// Ray distances closer than this to the end of the ray's range are left out: rounding decides them
const BOUNDARY: f32 = 1e-4;

#[test]
fn a_vertex_is_missed_exactly_when_the_bake_ray_misses_it() {
    let low = primitives::plane(1.0, 8);
    let mut high_mesh = primitives::plane(1.0, 40);
    for position in &mut high_mesh.positions {
        position.y = 0.04 * (position.x * 9.0).sin() * (position.z * 7.0).cos() + 0.005;
    }
    let (low_bvh, high_bvh) = (Bvh::new(&low), Bvh::new(&high_mesh));
    let high = HighPoly { file_ids: &[], mesh: &high_mesh, bvh: &high_bvh, base_color: &[] };
    let mut checked = [0usize; 2];

    for (max_frontal, max_rear) in [(0.0, 0.01), (0.02, 0.0), (0.02, 0.01), (0.05, 0.05)] {
        let mut bake_settings = settings(16, &[BakedMap::Normal]);
        bake_settings.cage.relative_to_bounding_box = false;
        bake_settings.high_poly.ignore_backface = false;
        (bake_settings.cage.max_frontal, bake_settings.cage.max_rear) = (max_frontal, max_rear);
        let request = BakeRequest {
            low_base_color: &[],
            mesh: &low,
            bvh: &low_bvh,
            material: 0,
            settings: &bake_settings,
            high: Some(high),
            cage: None,
            skew: None,
            cage_paint: None,
            tiles: (1, 1),
        };
        let reach = HighPolyReach::new(&request).unwrap();
        let context = BakeContext::new(&request).unwrap();

        for (vertex, vertex_reach) in reach.vertices.iter().enumerate() {
            let (vertex_reach, (_, missed)) = (vertex_reach.unwrap(), reach.vertex_missed(vertex, max_frontal, max_rear).unwrap());
            let RayStart::Distance { scale, .. } = vertex_reach.start else { panic!("a distance-based cage") };
            let height = vertex_reach.high_position.y - vertex_reach.low_position.y;
            if height.abs() < BOUNDARY || (height - max_frontal * scale).abs() < BOUNDARY || (height + max_rear).abs() < BOUNDARY {
                continue;
            }

            let nearest = low_bvh.nearest(vertex_reach.high_position, f32::INFINITY, &|_| true).unwrap();
            let surface = surface_at(&low, nearest.tri, nearest.u, nearest.v).unwrap();
            let (ray, _) = context.transfer_ray(&surface, [nearest.u, nearest.v]);
            let hit = high_bvh.closest(ray.origin, ray.direction, ray.max_distance).map(|hit| ray.origin + ray.direction * hit.t);
            let reaches_the_vertex = hit.is_some_and(|point| (point - vertex_reach.high_position).length() < 1e-4);
            assert_eq!(missed, !reaches_the_vertex, "vertex {vertex} at frontal {max_frontal}, rear {max_rear}: height {height}");
            checked[usize::from(missed)] += 1;
        }
    }
    assert!(checked[0] > 100 && checked[1] > 100, "both reached and missed vertices were compared: {checked:?}");
}
