//! Temporary bench (never committed): hide/show the default ball's smart material.

use std::time::Instant;

use serde_json::json;
use texturecraft_engine::compose::{DisplayKind, tile_bytes};
use texturecraft_engine::{Session, execute, sample_session};
use rayon::prelude::*;

fn settle(s: &mut Session) -> (f64, usize) {
    let t = Instant::now();
    let mut passes = 0;
    while s.pending_tiles() > 0 {
        s.update();
        passes += 1;
    }
    (t.elapsed().as_secs_f64(), passes)
}

fn convert(s: &mut Session) -> (f64, usize) {
    let t = Instant::now();
    let mut n = 0;
    for ts in 0..s.caches.len() {
        let tiles = s.take_uploads(ts);
        n += tiles.len();
        let baked = s.baked(ts).cloned().unwrap_or_default();
        let comp = &s.caches[ts].composite;
        let _: usize = tiles.par_iter().flat_map_iter(|&(tx, ty)| DisplayKind::ALL.map(|k| tile_bytes(comp, &baked, k, tx, ty).len())).sum();
    }
    (t.elapsed().as_secs_f64(), n)
}

#[test]
#[ignore]
fn bench_hide_smart_material() {
    let t = Instant::now();
    let mut s = sample_session(2048, 1024).unwrap();
    println!("setup {:.2?}", t.elapsed());
    let (first, _) = settle(&mut s);
    convert(&mut s);
    println!("initial composite {first:.3}s");
    let p = &s.doc.project;
    let set = &p.texture_sets[0];
    let id = set.stack.iter().copied().find(|id| p.layers.get(id).is_some_and(|l| l.name.contains("Steel"))).expect("smart material layer");
    for (i, label) in ["hide", "show", "hide"].iter().enumerate() {
        execute(&mut s, "layer.visible", &json!({"layer": id.0})).unwrap();
        let dirty = s.pending_tiles();
        let (secs, passes) = settle(&mut s);
        let (csecs, up) = convert(&mut s);
        println!("RESULT toggle{} {label}: composite {secs:.3}s ({dirty} tiles, {passes} passes), display conversion {csecs:.3}s ({up} tiles)", i + 1);
    }
}
