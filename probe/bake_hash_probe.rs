//! Prints a hash of every map baked on a few cage setups; compare the output of two builds

use texturecraft_bake::{BakePaintMap, BakeRequest, HighPoly, Progress, bake};
use texturecraft_core::{BakeResolution, BakeSettings, BakedMap, CageType};
use texturecraft_geom::glam::{Vec2, Vec3};
use texturecraft_geom::{Bvh, Mesh, primitives};

fn fnv(values: &[f32]) -> u64 {
    values.iter().flat_map(|value| value.to_bits().to_le_bytes()).fold(0xcbf29ce484222325, |hash, byte| (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3))
}

fn painted() -> BakePaintMap {
    let mut map = BakePaintMap::constant(0.3).resized(64, 64).unwrap();
    map.paint(&[Vec2::new(0.2, 0.3), Vec2::new(0.8, 0.6)], 0.25, 1.0, 0.5, 1.0).unwrap();
    map
}

#[test]
fn bake_hash_probe() {
    let low = primitives::rounded_box(Vec3::splat(0.5), 0.08);
    let high_mesh = primitives::sphere(0.62, 48, 24);
    let cage_mesh = Mesh { positions: low.positions.iter().zip(&low.normals).map(|(position, normal)| *position + *normal * 0.05).collect(), ..low.clone() };
    let (low_bvh, high_bvh) = (Bvh::new(&low), Bvh::new(&high_mesh));
    let high = HighPoly { file_ids: &[], mesh: &high_mesh, bvh: &high_bvh, base_color: &[] };
    let (skew, cage_paint) = (painted(), painted());
    let mut base = BakeSettings::default();
    base.common.resolution = BakeResolution::from_pixels(64).unwrap();
    base.maps.select(BakedMap::ALL);
    for rays in [&mut base.maps.ao.parameters.rays, &mut base.maps.thickness.parameters.rays, &mut base.maps.bent_normal.parameters.rays] {
        rays.rays = 8;
    }
    base.cage.max_frontal = 0.03;
    base.cage.max_rear = 0.02;

    let mut face_normals = base.clone();
    face_normals.cage.average_normals = false;
    face_normals.cage.relative_to_bounding_box = false;
    let mut custom = base.clone();
    custom.cage.kind = CageType::CustomFile;
    let cases: [(&str, &BakeSettings, bool, bool); 5] = [
        ("averaged", &base, false, false),
        ("averaged+paint", &base, true, false),
        ("face-normals+paint", &face_normals, true, false),
        ("custom-cage", &custom, false, true),
        ("low-only", &base, false, false),
    ];
    for (name, settings, with_paint, with_cage) in cases {
        let request = BakeRequest {
            low_base_color: &[],
            mesh: &low,
            bvh: &low_bvh,
            material: 0,
            settings,
            high: (name != "low-only").then_some(high),
            cage: with_cage.then_some(&cage_mesh),
            skew: with_paint.then_some(&skew),
            cage_paint: with_paint.then_some(&cage_paint),
            tiles: (1, 1),
        };
        let output = bake(&request, &Progress::new()).unwrap();
        for (map, values) in &output.maps {
            println!("HASH {name} {map:?} {:016x}", fnv(values));
        }
    }
}
