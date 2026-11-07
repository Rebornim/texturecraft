//! Symmetry around a moved plane or axis, driven through engine commands the way agents paint

use serde_json::json;
use texturecraft_engine::core::Channel;
use texturecraft_engine::geom::glam::{Vec2, Vec3, Vec4};
use texturecraft_engine::geom::{Mesh, primitives};
use texturecraft_engine::{Session, execute};

const RES: f32 = 256.0;
const VIEWPORT: f32 = 400.0;

fn session(mesh: Mesh) -> Session {
    let mut s = Session::new_project("t", mesh, "builtin:test", None, 256).unwrap();
    s.viewport = Vec2::splat(VIEWPORT);
    s.camera.yaw = 0.0;
    s.camera.pitch = 0.0;
    s.update();
    execute(&mut s, "tool.material", &json!({"channel": "baseColor", "color": [1.0, 0.0, 0.0]})).unwrap();
    execute(&mut s, "tool.brush", &json!({"size": 6, "hardness": 90})).unwrap();
    s
}

/// A 2×2 wall in the XY plane facing the camera (+Z)
fn wall() -> Mesh {
    let mut m = primitives::plane(2.0, 8);
    for p in &mut m.positions {
        *p = Vec3::new(p.x, -p.z, 0.0);
    }
    m.normals.iter_mut().for_each(|n| *n = Vec3::Z);
    m.tangents.iter_mut().for_each(|t| *t = Vec4::new(1.0, 0.0, 0.0, 1.0));
    m
}

/// Viewport pixel of world point `p`
fn screen(s: &Session, p: Vec3) -> Vec2 {
    let ndc = s.camera.project(p, 1.0).unwrap();
    Vec2::new((ndc.x * 0.5 + 0.5) * VIEWPORT, (0.5 - ndc.y * 0.5) * VIEWPORT)
}

/// Is the surface at world point `p` painted red?
fn red_at(s: &Session, p: Vec3) -> bool {
    let hit = s.pick(screen(s, p), s.viewport).unwrap();
    let c = s.caches[0].composite.value(Channel::BaseColor, (hit.uv.x * RES) as u32, (hit.uv.y * RES) as u32);
    c[0] > 0.85 && c[1] < 0.15
}

/// Where a viewport pixel lands on the mesh
fn surface(s: &Session, x: f32, y: f32) -> Vec3 {
    s.pick(Vec2::new(x, y), s.viewport).unwrap().pos
}

#[test]
fn a_moved_mirror_plane_paints_the_copy_at_the_mirrored_point() {
    let mut s = session(wall());
    let r = execute(&mut s, "tool.brush", &json!({"symmetryX": true, "symmetryOffset": [0.25, 0.0, 0.0]})).unwrap();

    let stroke = surface(&s, 300.0, 200.0);
    execute(&mut s, "tool.stroke", &json!({"points": [[300, 200]]})).unwrap();
    s.update();

    let mirrored = Vec3::new(0.5 - stroke.x, stroke.y, stroke.z);
    assert!(red_at(&s, stroke), "the stroke");
    assert!(red_at(&s, mirrored), "its copy across x = 0.25");
    assert!(!red_at(&s, Vec3::new(-stroke.x, stroke.y, stroke.z)), "nothing across the mesh center");
    assert_eq!(r["symmetryOffset"], json!([0.25, 0.0, 0.0]), "agents read the offset back");
}

#[test]
fn radial_copies_turn_around_the_moved_axis() {
    let mut s = session(wall());
    let axis = Vec3::new(0.2, -0.1, 0.0);
    execute(&mut s, "tool.brush", &json!({"symmetryRadial": 2, "symmetryAxis": "z", "symmetryOffset": axis.to_array()})).unwrap();

    let stroke = surface(&s, 290.0, 160.0);
    execute(&mut s, "tool.stroke", &json!({"points": [[290, 160]]})).unwrap();
    s.update();

    // Half a turn around Z through the moved axis
    assert!(red_at(&s, stroke), "the stroke");
    assert!(red_at(&s, axis * 2.0 - stroke), "its copy half a turn around the moved axis");
    assert!(!red_at(&s, Vec3::new(-stroke.x, -stroke.y, 0.0)), "nothing around the mesh center");
}

#[test]
fn a_closed_path_fill_mirrors_around_the_moved_plane() {
    let mut s = session(wall());
    execute(&mut s, "tool.brush", &json!({"symmetryX": true, "symmetryOffset": [-0.2, 0.0, 0.0]})).unwrap();

    let square = json!([[250, 180], [280, 180], [280, 220], [250, 220]]);
    execute(&mut s, "tool.path", &json!({"points": square, "closed": true, "fill": true, "commit": true})).unwrap();

    let inside = surface(&s, 265.0, 200.0);
    assert!(red_at(&s, inside), "the filled lasso");
    assert!(red_at(&s, Vec3::new(-0.4 - inside.x, inside.y, 0.0)), "its copy across x = -0.2");
    assert!(!red_at(&s, Vec3::new(-inside.x, inside.y, 0.0)), "nothing across the mesh center");
}

/// FNV-1a over the bits of every base color texel of the composite (stable across Rust versions)
fn composite_hash(s: &Session) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for y in 0..RES as u32 {
        for x in 0..RES as u32 {
            for v in s.caches[0].composite.value(Channel::BaseColor, x, y) {
                for byte in v.to_bits().to_le_bytes() {
                    hash = (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
                }
            }
        }
    }
    hash
}

/// The hash `main` gives for the strokes below, before symmetry could move
const MAIN_COMPOSITE_HASH: u64 = 0x9d4d_ce55_1238_97d6;

#[test]
fn default_symmetry_paints_exactly_as_before() {
    let mut s = session(primitives::sphere(1.0, 48, 24));
    execute(&mut s, "tool.brush", &json!({"symmetryX": true, "symmetryY": true, "symmetryRadial": 3, "symmetryAxis": "y"})).unwrap();
    execute(&mut s, "tool.stroke", &json!({"points": [[230, 170], [260, 190], [270, 230]]})).unwrap();
    let square = json!([[150, 150], [180, 150], [180, 180], [150, 180]]);
    execute(&mut s, "tool.path", &json!({"points": square, "closed": true, "fill": true, "commit": true})).unwrap();
    s.update();

    let hash = composite_hash(&s);
    eprintln!("composite hash {hash:#018x}");
    assert_eq!(hash, MAIN_COMPOSITE_HASH, "mesh-centered symmetry paints differently from main");
}
