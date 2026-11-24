// Scratch timing for the map preview (drop before the PR): each baker alone at the preview sizes
use texturecraft_engine::bake::Progress;
use texturecraft_engine::core::{BakeResolution, BakedMap};

#[test]
#[ignore]
fn preview_bakers() {
    let path = std::env::var("PREVIEW_MESH").unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let mut session = texturecraft_engine::Session::from_mesh_bytes("low.fbx", &bytes, 2048, "bakers").unwrap();
    session.add_high_poly_bytes("high.fbx", bytes).unwrap();
    session.doc.project.texture_sets[0].bake.high_poly.use_low_as_high = false;
    println!("MESH sets {} low tris {}", session.doc.project.texture_sets.len(), session.mesh.triangle_count());
    for map in BakedMap::ALL {
        let mut line = format!("BAKER {:<18}", map.id());
        for size in [BakeResolution::R64, BakeResolution::R128, BakeResolution::R256] {
            let mut times = Vec::new();
            for _ in 0..5 {
                let mut task = session.bake_task(0).unwrap();
                task.settings.maps.select([map]);
                task.settings.common.resolution = size;
                let started = std::time::Instant::now();
                task.run(&Progress::new()).unwrap();
                times.push(started.elapsed().as_secs_f64() * 1000.0);
            }
            times.sort_by(f64::total_cmp);
            line += &format!(" {}px {:>7.1} ms", size.pixels(), times[2]);
        }
        let started = std::time::Instant::now();
        let _ = session.bake_task(0).unwrap();
        line += &format!("  (task setup {:.1} ms)", started.elapsed().as_secs_f64() * 1000.0);
        println!("{line}");
    }
}
