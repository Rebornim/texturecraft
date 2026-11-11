//! How fills read their image sources (filtering), through engine commands.

use serde_json::{Value, json};
use texturecraft_engine::core::{Channel, Fill, Filtering, LayerId, LayerKind, Sampling};
use texturecraft_engine::geom::NO_TRI;
use texturecraft_engine::geom::primitives;
use texturecraft_engine::{Session, execute};

fn session() -> Session {
    let mut s = Session::new_project("t", primitives::sphere(1.0, 48, 24), "builtin:sphere", None, 256).unwrap();
    s.update();
    s
}

fn png(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 4]) -> Vec<u8> {
    let mut out = Vec::new();
    image::RgbaImage::from_fn(w, h, |x, y| image::Rgba(f(x, y))).write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).unwrap();
    out
}

/// A fill layer whose base color is a one-texel black and white checker, tiled `tiling` times.
fn checker_fill(s: &mut Session, tiling: f32) -> LayerId {
    let name = s.import_resource_bytes("checker.png", png(64, 64, |x, y| if (x + y) % 2 == 0 { [255; 4] } else { [0, 0, 0, 255] })).unwrap();
    let layer = LayerId(execute(s, "layer.add.fill", &Value::Null).unwrap()["layer"].as_u64().unwrap());
    execute(s, "layer.fill.set", &json!({"layer": layer.0, "channel": "baseColor", "image": name})).unwrap();
    execute(s, "layer.fill.projection", &json!({"layer": layer.0, "scale": tiling})).unwrap();
    layer
}

/// Lowest and highest base color red across the covered texels of the set's middle band.
fn spread(s: &mut Session) -> (f32, f32) {
    s.update();
    let cache = &s.caches[0];
    let (w, h) = cache.size;
    let mut range = (f32::MAX, f32::MIN);
    for y in h * 3 / 8..h * 5 / 8 {
        for x in w / 4..w * 3 / 4 {
            if cache.gbuf.tri[(y * w + x) as usize] != NO_TRI {
                let red = cache.composite.value(Channel::BaseColor, x, y)[0];
                range = (range.0.min(red), range.1.max(red));
            }
        }
    }
    range
}

fn filtering(s: &mut Session, layer: LayerId, id: &str) {
    execute(s, "layer.fill.projection", &json!({"layer": layer.0, "filtering": id})).unwrap();
}

/// A checker shrunk to about two image texels per surface texel: Bilinear | HQ averages it to
/// an even gray, Bilinear | Sharp aliases it into bands, Nearest keeps pure black and white.
#[test]
fn filtering_decides_how_a_shrunk_image_reads() {
    let mut s = session();
    let layer = checker_fill(&mut s, 7.3);
    let (lo, hi) = spread(&mut s);
    assert!(hi - lo < 0.2, "a new fill reads with Bilinear | HQ: an even gray, got {lo}..{hi}");

    filtering(&mut s, layer, "bilinearSharp");
    let (lo, hi) = spread(&mut s);
    assert!(hi - lo > 0.5, "Bilinear | Sharp aliases a shrunk checker: {lo}..{hi}");

    filtering(&mut s, layer, "nearest");
    s.update();
    let cache = &s.caches[0];
    let (w, h) = cache.size;
    let pure = (h * 3 / 8..h * 5 / 8)
        .flat_map(|y| (w / 4..w * 3 / 4).map(move |x| (x, y)))
        .filter(|&(x, y)| cache.gbuf.tri[(y * w + x) as usize] != NO_TRI)
        .all(|(x, y)| {
            let red = cache.composite.value(Channel::BaseColor, x, y)[0];
            red < 0.01 || red > 0.99
        });
    assert!(pure, "Nearest reads one image texel: black or white only");

    let inspect = execute(&mut s, "project.inspect", &Value::Null).unwrap();
    let row = inspect["layers"].as_array().unwrap().iter().find(|l| l["id"] == layer.0).unwrap().clone();
    assert_eq!(row["fill"]["filtering"], json!("nearest"));
    assert!(execute(&mut s, "layer.fill.projection", &json!({"layer": layer.0, "filtering": "trilinear"})).is_err());
}

/// A fill saved before the Filtering choice existed keeps reading its images as it did
/// (plain bilinear); a fill made now starts on Bilinear | HQ.
#[test]
fn fills_saved_before_filtering_keep_their_look() {
    let mut s = session();
    let layer = checker_fill(&mut s, 1.0);
    let LayerKind::Fill(fill) = &s.doc.project.layer(layer).unwrap().kind else { panic!("a fill layer") };
    assert_eq!(fill.sampling.filtering, Filtering::BilinearHq);

    let mut saved = serde_json::to_value(fill).unwrap();
    saved.as_object_mut().unwrap().remove("sampling");
    let loaded: Fill = serde_json::from_value(saved).unwrap();
    assert_eq!(loaded.sampling, Sampling::SAVED_BEFORE);
    assert_eq!(loaded.sampling.filtering, Filtering::BilinearSharp);
}
