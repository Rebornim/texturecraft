use super::{covered_values, settings, transfer};
use crate::{BakeRequest, CageShell, HighPoly, HighPolyReach};
use texturecraft_core::{BakeSettings, BakedMap, CageType};
use texturecraft_geom::glam::Vec3;
use texturecraft_geom::{Bvh, Mesh, primitives};

/// A distance setting far larger than any fitted distance in these tests, to tell the two apart
const UNFITTED_DISTANCE: f32 = 0.5;

/// Settings baking `maps` with the automatic cage, and distance settings no fitted cage comes near
fn automatic(maps: &[BakedMap]) -> BakeSettings {
    let mut bake_settings = settings(32, maps);
    bake_settings.cage.kind = CageType::Automatic;
    bake_settings.cage.relative_to_bounding_box = false;
    (bake_settings.cage.max_frontal, bake_settings.cage.max_rear) = (UNFITTED_DISTANCE, UNFITTED_DISTANCE);
    bake_settings
}

/// How far each corner of the automatic cage of the flat `low` around `high` rises above it, with the corner's low-poly position
fn cage_heights(low: &Mesh, high: &Mesh, bake_settings: &BakeSettings) -> Vec<(Vec3, f32)> {
    let (low_bvh, high_bvh) = (Bvh::new(low), Bvh::new(high));
    let request = BakeRequest {
        low_base_color: &[],
        mesh: low,
        bvh: &low_bvh,
        material: 0,
        settings: bake_settings,
        high: Some(HighPoly { file_ids: &[], mesh: high, bvh: &high_bvh, base_color: &[] }),
        cage: None,
        skew: None,
        cage_paint: None,
        tiles: (1, 1),
    };
    let shell = CageShell::new(&request).unwrap();
    let corners = shell.corners(bake_settings.cage.max_frontal);
    shell.positions.iter().zip(corners).map(|(position, corner)| (*position, corner.y - position.y)).collect()
}

#[test]
fn the_automatic_cage_reaches_a_high_poly_the_default_distances_miss() {
    let low = primitives::cube(1.0);
    let high = primitives::sphere(0.8, 32, 16);
    let opacity_of = |bake_settings: &BakeSettings| -> Vec<f32> {
        let output = transfer(&low, &high, None, bake_settings).unwrap();
        covered_values(&low, &output, BakedMap::Opacity).into_iter().map(|(_, opacity)| opacity).collect()
    };

    let distance_based = opacity_of(&settings(32, &[BakedMap::Opacity]));
    assert!(distance_based.contains(&0.0), "the sphere is beyond the default distances at the middle of each face");

    let mut bake_settings = settings(32, &[BakedMap::Opacity]);
    bake_settings.cage.kind = CageType::Automatic;
    let fitted = opacity_of(&bake_settings);
    assert!(fitted.iter().all(|opacity| *opacity == 1.0), "every ray of the automatic cage finds the sphere");

    // The triangles at the sphere's poles have no area, so a pole vertex has no side to face the ray with
    bake_settings.high_poly.ignore_backface = false;
    let (low_bvh, high_bvh) = (Bvh::new(&low), Bvh::new(&high));
    let request = BakeRequest {
        low_base_color: &[],
        mesh: &low,
        bvh: &low_bvh,
        material: 0,
        settings: &bake_settings,
        high: Some(HighPoly { file_ids: &[], mesh: &high, bvh: &high_bvh, base_color: &[] }),
        cage: None,
        skew: None,
        cage_paint: None,
        tiles: (1, 1),
    };
    let reach = HighPolyReach::new(&request).unwrap();
    for vertex in 0..high.positions.len() {
        assert_eq!(reach.missed(vertex, 0.0, 0.0), Some(false), "high-poly vertex {vertex} is inside the cage whatever the distance settings");
    }
}

#[test]
fn the_automatic_cage_stays_low_where_the_high_poly_is_low() {
    const LOW_HEIGHT: f32 = 0.01;
    const RAISED_HEIGHT: f32 = 0.2;
    const RAISED_FROM_X: f32 = 0.25;

    let low = primitives::plane(1.0, 8);
    let mut high = primitives::plane(1.0, 40);
    for position in &mut high.positions {
        position.y = if position.x > RAISED_FROM_X { RAISED_HEIGHT } else { LOW_HEIGHT };
    }

    let heights = cage_heights(&low, &high, &automatic(&[BakedMap::Normal]));

    for (position, height) in heights {
        if position.x < 0.0 {
            assert!((LOW_HEIGHT..RAISED_HEIGHT / 2.0).contains(&height), "the cage at {position} hugs the low part: {height}");
        } else if position.x > RAISED_FROM_X + 0.125 {
            assert!((RAISED_HEIGHT..RAISED_HEIGHT * 2.0).contains(&height), "the cage at {position} clears the raised part: {height}");
        }
    }
}

#[test]
fn low_poly_vertices_far_from_any_high_poly_vertex_take_their_neighbours_distances() {
    const HEIGHT: f32 = 0.05;

    let low = primitives::plane(1.0, 8);
    let mut high = primitives::plane(1.0, 1);
    for position in &mut high.positions {
        position.y = HEIGHT;
    }

    let heights = cage_heights(&low, &high, &automatic(&[BakedMap::Normal]));

    assert!(heights.iter().all(|(_, height)| (HEIGHT..HEIGHT * 2.0).contains(height)), "the four corners' fit spreads over the whole plane");
}

#[test]
fn a_part_no_high_poly_part_matches_keeps_the_distance_settings() {
    let mut low = primitives::plane(1.0, 2);
    let mut high = primitives::plane(1.0, 2);
    low.submeshes[0].name = "panel_low".into();
    high.submeshes[0].name = "door_high".into();
    let mut bake_settings = automatic(&[BakedMap::Normal]);
    bake_settings.high_poly.match_mode = texturecraft_core::MeshMatchMode::ByMeshName;

    let heights = cage_heights(&low, &high, &bake_settings);

    assert!(heights.iter().all(|(_, height)| *height == UNFITTED_DISTANCE));
}
