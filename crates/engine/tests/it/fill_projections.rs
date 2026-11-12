//! The 3D projection settings and the projection kinds of a fill, through engine commands

use serde_json::{Value, json};
use texturecraft_engine::core::{Channel, DepthCulling, FillSymmetry, LayerId, LayerKind, Projection, Projector, Sampling};
use texturecraft_engine::geom::primitives;
use texturecraft_engine::{Session, execute};

fn session() -> Session {
    let mut s = Session::new_project("t", primitives::sphere(1.0, 48, 24), "builtin:sphere", None, 128).unwrap();
    s.update();
    s
}

/// A fill layer with a checker procedural in base color
fn checker_fill(s: &mut Session) -> LayerId {
    let layer = LayerId(execute(s, "layer.add.fill", &Value::Null).unwrap()["layer"].as_u64().unwrap());
    execute(s, "layer.fill.set", &json!({"layer": layer.0, "channel": "baseColor", "procedural": "Checker", "scale": 4, "low": [0, 0, 0], "high": [1, 1, 1]}))
        .unwrap();
    layer
}

fn set(s: &mut Session, layer: LayerId, mut p: Value) {
    p["layer"] = json!(layer.0);
    execute(s, "layer.fill.projection", &p).unwrap();
}

fn projection(s: &Session, layer: LayerId) -> Projection {
    match &s.doc.project.layer(layer).unwrap().kind {
        LayerKind::Fill(f) => f.projection.clone(),
        _ => panic!("a fill layer"),
    }
}

/// Base color red of every texel
fn snapshot(s: &mut Session) -> Vec<f32> {
    s.update();
    let cache = &s.caches[0];
    let (w, h) = cache.size;
    (0..w * h).map(|i| cache.composite.value(Channel::BaseColor, i % w, i / w)[0]).collect()
}

/// Rotation, per-axis scale and offset move every 3D projection; the defaults and older saves change nothing
#[test]
fn three_d_projection_settings_turn_stretch_and_move_every_3d_projection() {
    let mut s = session();
    let layer = checker_fill(&mut s);
    for kind in ["triplanar", "planar", "spherical", "cylindrical"] {
        set(&mut s, layer, json!({"projection": kind}));
        let plain = snapshot(&mut s);

        set(&mut s, layer, json!({"projectorRotation": [30, 45, 60]}));
        assert_ne!(snapshot(&mut s), plain, "{kind}: rotation turns the pattern");
        set(&mut s, layer, json!({"projectorRotation": [0, 0, 0], "projectorScale": [2, 0.5, 1]}));
        assert_ne!(snapshot(&mut s), plain, "{kind}: per-axis scale stretches the pattern");
        set(&mut s, layer, json!({"projectorScale": 1, "offset": [0.3, 0.2, 0.1]}));
        assert_ne!(snapshot(&mut s), plain, "{kind}: offset moves the pattern");
        set(&mut s, layer, json!({"offset": 0}));
        assert_eq!(snapshot(&mut s), plain, "{kind}: the defaults change nothing");

        set(&mut s, layer, json!({"projectorRotation": [10, 20, 30], "projectorScale": [1, 2, 3]}));
        let mut saved = serde_json::to_value(projection(&s, layer)).unwrap();
        let fields = saved.as_object_mut().unwrap().values_mut().next().unwrap().as_object_mut().unwrap();
        fields.remove("projector");
        let loaded: Projection = serde_json::from_value(saved).unwrap();
        let settings = match loaded {
            Projection::Triplanar { projector, .. }
            | Projection::Planar { projector, .. }
            | Projection::Spherical { projector, .. }
            | Projection::Cylindrical { projector, .. } => projector,
            other => panic!("{kind}: loaded as {other:?}"),
        };
        assert_eq!(settings, Projector::default(), "{kind}: an older save has no turn or stretch");
        set(&mut s, layer, json!({"projectorRotation": 0, "projectorScale": 1}));
    }

    let inspect = execute(&mut s, "project.inspect", &Value::Null).unwrap();
    let row = inspect["layers"].as_array().unwrap().iter().find(|l| l["id"] == layer.0).unwrap().clone();
    assert_eq!(row["fill"]["projectionSettings"]["Cylindrical"]["projector"]["scale"], json!([1.0, 1.0, 1.0]));
}

/// Fill (Match per UV Tile) shows the same pattern on both UDIM tiles, where UV runs on across them
#[test]
fn match_per_uv_tile_repeats_the_source_once_per_tile() {
    let mut s = Session::new_project("t", primitives::udim_plane(), "builtin:udim_plane", None, 128).unwrap();
    let layer = LayerId(execute(&mut s, "layer.add.fill", &Value::Null).unwrap()["layer"].as_u64().unwrap());
    execute(
        &mut s,
        "layer.fill.set",
        &json!({"layer": layer.0, "channel": "baseColor", "procedural": "Clouds", "scale": 3, "low": [0, 0, 0], "high": [1, 1, 1]}),
    )
    .unwrap();
    let tiles = |s: &mut Session| {
        s.update();
        let composite = &s.caches[0].composite;
        let tile = |block: u32| (0..128 * 128).map(|i| composite.value(Channel::BaseColor, block * 128 + i % 128, i / 128)[0]).collect::<Vec<f32>>();
        (tile(0), tile(1))
    };
    let (left, right) = tiles(&mut s);
    assert_ne!(left, right, "a UV projection runs on into the next tile");

    set(&mut s, layer, json!({"projection": "uvTile"}));
    assert_eq!(projection(&s, layer), Projection::UvTile);
    let (left, right) = tiles(&mut s);
    let worst = left.iter().zip(&right).map(|(a, b)| (a - b).abs()).fold(0.0, f32::max);
    assert!(worst < 1e-4, "every tile shows the same pattern (worst difference {worst})");
}

/// Cylindrical Angle is the arc one repeat spans; older saves go the whole way round
#[test]
fn cylindrical_angle_is_the_arc_one_repeat_spans() {
    let mut s = session();
    let layer = checker_fill(&mut s);
    set(&mut s, layer, json!({"projection": "cylindrical"}));
    let plain = snapshot(&mut s);
    set(&mut s, layer, json!({"angle": 120}));
    assert!(matches!(projection(&s, layer), Projection::Cylindrical { angle: 120.0, .. }));
    assert_ne!(snapshot(&mut s), plain, "a third of the way round repeats three times");
    set(&mut s, layer, json!({"angle": 360}));
    assert_eq!(snapshot(&mut s), plain, "360 degrees is the whole way round");

    let mut saved = serde_json::to_value(projection(&s, layer)).unwrap();
    saved["Cylindrical"].as_object_mut().unwrap().remove("angle");
    assert!(matches!(serde_json::from_value(saved).unwrap(), Projection::Cylindrical { angle: 360.0, .. }));
}

/// Texel indices whose surface normal has a Z above `above` (and below `below`)
fn facing_z(s: &Session, above: f32, below: f32) -> Vec<usize> {
    let cache = &s.caches[0];
    let (w, h) = cache.size;
    (0..(w * h) as usize).filter(|&i| cache.gbuf.surface(&s.mesh, i).is_some_and(|surface| surface.nrm.z > above && surface.nrm.z < below)).collect()
}

/// Cropped to Shape stops a shrunk tri-planar box short of the mesh; the default box holds it all
#[test]
fn shape_crop_stops_a_projection_at_its_shape() {
    let mut s = session();
    let layer = checker_fill(&mut s);
    execute(&mut s, "layer.visible", &json!({"layer": layer.0, "visible": false})).unwrap();
    let empty = snapshot(&mut s);
    execute(&mut s, "layer.visible", &json!({"layer": layer.0, "visible": true})).unwrap();
    set(&mut s, layer, json!({"projection": "triplanar"}));
    let plain = snapshot(&mut s);
    set(&mut s, layer, json!({"shapeCrop": "cropped"}));
    assert_eq!(snapshot(&mut s), plain, "the default box holds the whole mesh");

    set(&mut s, layer, json!({"projectorScale": 0.5}));
    let cropped = snapshot(&mut s);
    let gone = cropped.iter().zip(&empty).filter(|(a, b)| a == b).count();
    set(&mut s, layer, json!({"shapeCrop": "extendsOutside"}));
    let extends = snapshot(&mut s);
    let gone_extending = extends.iter().zip(&empty).filter(|(a, b)| a == b).count();
    assert!(gone > gone_extending + 1000, "the half-size box leaves the rest of the mesh bare ({gone} vs {gone_extending} bare texels)");
    assert!(execute(&mut s, "layer.fill.projection", &json!({"layer": layer.0, "shapeCrop": "clipped"})).is_err());
}

/// Backface culling leaves the far side of the sphere bare and the near side unchanged
#[test]
fn backface_culling_keeps_a_planar_projection_off_faces_turned_away() {
    let mut s = session();
    let layer = checker_fill(&mut s);
    execute(&mut s, "layer.visible", &json!({"layer": layer.0, "visible": false})).unwrap();
    let empty = snapshot(&mut s);
    execute(&mut s, "layer.visible", &json!({"layer": layer.0, "visible": true})).unwrap();
    set(&mut s, layer, json!({"projection": "planar", "axis": 2}));
    let plain = snapshot(&mut s);
    set(&mut s, layer, json!({"backfaceCulling": true, "backfaceAngle": 90, "backfaceHardness": 1}));
    let culled = snapshot(&mut s);
    let (far, near) = (facing_z(&s, -1.1, -0.05), facing_z(&s, 0.05, 1.1));
    assert!(!far.is_empty() && !near.is_empty());
    assert!(far.iter().all(|&i| culled[i] == empty[i]), "faces turned away get none of the projection");
    assert!(near.iter().all(|&i| culled[i] == plain[i]), "faces turned towards it keep all of it");

    set(&mut s, layer, json!({"backfaceAngle": 45}));
    let narrow = snapshot(&mut s);
    let sideways = facing_z(&s, 0.1, 0.6);
    assert!(sideways.iter().all(|&i| narrow[i] == empty[i]), "past 45 degrees nothing is left");
}

/// A decal saved before decals were planar projections loads as a planar projection placed on its frame, cropped to it and off faces turned away
#[test]
fn saved_decals_load_as_planar_projections_placed_on_the_surface() {
    use texturecraft_engine::core::{Backface, Decal, Fill, ShapeCrop};
    let frame =
        Decal { center: [0.0, 0.0, 1.0], normal: [0.0, 0.0, 1.0], up: [0.0, 1.0, 0.0], size: [0.5, 0.4], rotation: 10.0, offset: [0.1, 0.0], depth: 0.3 };
    let mut saved = serde_json::to_value(Fill::default()).unwrap();
    saved["projection"] = json!({ "Decal": frame });
    let fill: Fill = serde_json::from_value(saved).unwrap();
    let Projection::Planar { placed: Some(placed), projector, .. } = fill.projection else { panic!("{:?}", fill.projection) };
    assert_eq!(placed, frame);
    assert_eq!(projector.crop, ShapeCrop::Cropped);
    assert_eq!(projector.backface, Backface { on: true, angle: 90.0, hardness: 0.5 });
    assert_eq!(serde_json::to_value(&fill.projection).unwrap()["Planar"]["placed"], json!(frame), "it saves as a planar projection");
}

/// A warp projects from a frame around the mesh: pulling a corner bends the image, Projection Depth bounds it when cropped, and Reset frame unpulls the corners
#[test]
fn warp_projection_bends_to_its_corners_reaches_its_depth_and_resets_its_frame() {
    let mut s = session();
    let layer = checker_fill(&mut s);
    execute(&mut s, "layer.visible", &json!({"layer": layer.0, "visible": false})).unwrap();
    let empty = snapshot(&mut s);
    execute(&mut s, "layer.visible", &json!({"layer": layer.0, "visible": true})).unwrap();
    set(&mut s, layer, json!({"projection": "warp"}));
    let Projection::Warp { frame, corners, depth, .. } = projection(&s, layer) else { panic!("{:?}", projection(&s, layer)) };
    assert_eq!((corners, depth), (Projection::FRAME_CORNERS, Projection::WARP_DEPTH));
    assert_eq!((frame.normal, frame.up), ([0.0, 0.0, 1.0], [0.0, 1.0, 0.0]), "seen from the front");
    assert!(frame.size.iter().all(|side| (side - 2.0).abs() < 1e-3) && frame.center.iter().all(|c| c.abs() < 1e-3), "around the unit sphere: {frame:?}");
    let unpulled = snapshot(&mut s);
    assert_ne!(unpulled, empty);

    let pulled = json!([[-0.5, 0.5], [0.9, 0.9], [0.5, -0.5], [-0.5, -0.5]]);
    set(&mut s, layer, json!({"warpCorners": pulled}));
    let bent = snapshot(&mut s);
    assert_ne!(bent, unpulled, "a pulled corner bends the image");
    set(&mut s, layer, json!({"projection": "warp", "rotation": 0}));
    assert_eq!(snapshot(&mut s), bent, "picking warp again keeps the corners");

    set(&mut s, layer, json!({"shapeCrop": "cropped", "warpCorners": Projection::FRAME_CORNERS}));
    let cropped = snapshot(&mut s);
    // Depth 0.1 of a frame 2 wide reaches 0.2 in front of and behind the frame through the sphere's middle
    let (beyond, within) = (facing_z(&s, 0.35, 1.1), facing_z(&s, -0.05, 0.05));
    assert!(!beyond.is_empty() && !within.is_empty());
    assert!(beyond.iter().all(|&i| cropped[i] == empty[i]), "past its Projection Depth the warp does not reach");
    assert!(within.iter().all(|&i| cropped[i] == unpulled[i]), "within it the warp covers as before");
    set(&mut s, layer, json!({"projectionDepth": 1}));
    let deep = snapshot(&mut s);
    assert!(beyond.iter().all(|&i| deep[i] == unpulled[i]), "a deeper warp reaches the whole sphere");

    set(&mut s, layer, json!({"warpCorners": pulled}));
    execute(&mut s, "layer.fill.resetFrame", &json!({"layer": layer.0})).unwrap();
    let Projection::Warp { frame: reset, corners, depth, .. } = projection(&s, layer) else { panic!("still a warp") };
    assert_eq!((reset, corners, depth), (frame, Projection::FRAME_CORNERS, 1.0), "the frame and corners go back; the depth stays");

    let inspect = execute(&mut s, "project.inspect", &Value::Null).unwrap();
    let row = inspect["layers"].as_array().unwrap().iter().find(|l| l["id"] == layer.0).unwrap().clone();
    assert_eq!(row["fill"]["projection"], "Warp projection");
    assert_eq!(row["fill"]["projectionSettings"]["Warp"]["corners"], json!(Projection::FRAME_CORNERS));
    set(&mut s, layer, json!({"projection": "planar"}));
    assert!(execute(&mut s, "layer.fill.resetFrame", &json!({"layer": layer.0})).is_err(), "only a warp has a frame to reset");
}

/// Depth culling keeps planar and warp projections off surfaces hidden behind others from the projector; new ones start with it on, older saves without it
#[test]
fn depth_culling_keeps_planar_and_warp_projections_off_hidden_surfaces() {
    let mut s = session();
    let layer = checker_fill(&mut s);
    execute(&mut s, "layer.visible", &json!({"layer": layer.0, "visible": false})).unwrap();
    let empty = snapshot(&mut s);
    execute(&mut s, "layer.visible", &json!({"layer": layer.0, "visible": true})).unwrap();
    set(&mut s, layer, json!({"projection": "planar", "axis": 2}));
    let Projection::Planar { projector, .. } = projection(&s, layer) else { panic!("{:?}", projection(&s, layer)) };
    assert_eq!(projector.depth, DepthCulling::NEW, "a new planar projection culls by depth");
    let culled = snapshot(&mut s);
    set(&mut s, layer, json!({"depthCulling": false}));
    let plain = snapshot(&mut s);

    // The sphere's far side sits behind its near side, seen from +Z
    let (far, near) = (facing_z(&s, -1.1, -0.3), facing_z(&s, 0.3, 1.1));
    assert!(!far.is_empty() && !near.is_empty());
    assert!(far.iter().any(|&i| plain[i] != empty[i]), "without depth culling the far side gets the projection");
    assert!(far.iter().all(|&i| culled[i] == empty[i]), "hidden behind the near side, the far side gets none of it");
    assert!(near.iter().all(|&i| culled[i] == plain[i]), "the near side keeps all of it");

    set(&mut s, layer, json!({"depthCulling": true, "depthCullingHardness": 0}));
    assert_ne!(snapshot(&mut s), culled, "a softer cull fades further behind the near side");
    let inspect = execute(&mut s, "project.inspect", &Value::Null).unwrap();
    let row = inspect["layers"].as_array().unwrap().iter().find(|l| l["id"] == layer.0).unwrap().clone();
    assert_eq!(row["fill"]["projectionSettings"]["Planar"]["projector"]["depth"], json!({"on": true, "hardness": 0.0}));

    set(&mut s, layer, json!({"projection": "warp"}));
    let Projection::Warp { projector, .. } = projection(&s, layer) else { panic!("{:?}", projection(&s, layer)) };
    assert_eq!(projector.depth, DepthCulling::NEW, "a new warp culls by depth");
    let warped = snapshot(&mut s);
    assert!(far.iter().all(|&i| warped[i] == empty[i]), "a warp from the front leaves the far side bare");

    let saved: Projector = serde_json::from_value(json!({"rotation": [0, 0, 0], "scale": [1, 1, 1]})).unwrap();
    assert_eq!(saved.depth, DepthCulling { on: false, hardness: 0.75 }, "an older save has no depth culling");
}

/// Texels of `far` that `values` gives something other than the bare `empty`
fn filled(far: &[usize], values: &[f32], empty: &[f32]) -> usize {
    far.iter().filter(|&&i| values[i] != empty[i]).count()
}

/// Mirror and radial symmetry copy a planar projection onto the sphere's far side and leave the near side as it was; Flip copy turns the copies' images over; UV projections and older saves have no copies
#[test]
fn fill_symmetry_mirrors_or_turns_copies_of_a_3d_projection() {
    let mut s = session();
    let layer = checker_fill(&mut s);
    execute(&mut s, "layer.visible", &json!({"layer": layer.0, "visible": false})).unwrap();
    let empty = snapshot(&mut s);
    execute(&mut s, "layer.visible", &json!({"layer": layer.0, "visible": true})).unwrap();
    // Depth culled from +Z, the planar projection reaches only the near side
    set(&mut s, layer, json!({"projection": "planar", "axis": 2}));
    let plain = snapshot(&mut s);
    let (far, near) = (facing_z(&s, -1.1, -0.3), facing_z(&s, 0.3, 1.1));
    assert_eq!(filled(&far, &plain, &empty), 0);

    set(&mut s, layer, json!({"symmetry": true, "symmetryType": "mirror", "symmetryAxis": 2}));
    let mirrored = snapshot(&mut s);
    assert!(filled(&far, &mirrored, &empty) > far.len() / 2, "the mirror copy reaches the far side");
    assert!(near.iter().all(|&i| mirrored[i] == plain[i]), "the original keeps its own half");

    set(&mut s, layer, json!({"symmetryType": "radial", "symmetryAxis": 1, "symmetryCount": 2}));
    let turned = snapshot(&mut s);
    assert!(filled(&far, &turned, &empty) > far.len() / 2, "the copy turned half round reaches the far side");
    assert!(near.iter().all(|&i| turned[i] == plain[i]), "the original keeps its own wedge");
    assert_ne!(turned, mirrored, "a turned copy is not a mirrored one");
    set(&mut s, layer, json!({"symmetry": false}));
    assert_eq!(snapshot(&mut s), plain, "switched off, the copies go");

    // A left to right ramp shows a copy's image turned over
    let ramp = s.import_resource_bytes("ramp.png", ramp_png()).unwrap();
    execute(&mut s, "layer.fill.set", &json!({"layer": layer.0, "channel": "baseColor", "image": ramp})).unwrap();
    set(&mut s, layer, json!({"symmetry": true}));
    let copied = snapshot(&mut s);
    set(&mut s, layer, json!({"flipCopy": [true, false]}));
    let flipped = snapshot(&mut s);
    assert!(far.iter().filter(|&&i| (flipped[i] - copied[i]).abs() > 0.1).count() > far.len() / 4, "Flip copy U turns the copy's ramp round");
    assert!(near.iter().all(|&i| flipped[i] == copied[i]), "the original is never flipped");

    set(&mut s, layer, json!({"projection": "uv"}));
    let uv = snapshot(&mut s);
    set(&mut s, layer, json!({"symmetry": false}));
    assert_eq!(snapshot(&mut s), uv, "a UV projection has no copies");

    assert!(execute(&mut s, "layer.fill.projection", &json!({"layer": layer.0, "symmetryType": "spiral"})).is_err());
    assert!(execute(&mut s, "layer.fill.projection", &json!({"layer": layer.0, "symmetryAxis": 3})).is_err());
    let saved: Sampling = serde_json::from_value(json!({"filtering": "BilinearHq", "wrap": "Repeat", "scale": "Tiling"})).unwrap();
    assert_eq!(saved.symmetry, FillSymmetry::OFF, "an older save has no symmetry");
}

/// Moving an axis planar on the model first places it as the frame it projects from: nothing moves until the frame does, and undo puts the axis planar back
#[test]
fn moving_an_axis_planar_places_it_as_its_frame_first() {
    let mut s = session();
    let layer = checker_fill(&mut s);
    let ramp = s.import_resource_bytes("ramp.png", ramp_png()).unwrap();
    execute(&mut s, "layer.fill.set", &json!({"layer": layer.0, "channel": "baseColor", "image": ramp})).unwrap();
    set(&mut s, layer, json!({"projection": "planar", "axis": 0, "scale": 1.5, "offset": [0.2, -0.1, 0.0], "projectorRotation": [0, 30, 10]}));
    let axis = projection(&s, layer);
    let before = snapshot(&mut s);

    execute(&mut s, "layer.decal.set", &json!({"layer": layer.0})).unwrap();
    let placed = projection(&s, layer);
    let Projection::Planar { placed: Some(frame), projector, .. } = &placed else { panic!("placed: {placed:?}") };
    assert_eq!((projector.rotation, projector.scale), ([0.0; 3], [1.0; 3]), "the turn is in the frame now");
    let after = snapshot(&mut s);
    let moved = before.iter().zip(&after).filter(|(a, b)| (*a - *b).abs() > 2.0 / 255.0).count();
    assert!(moved * 200 < before.len(), "the image stays where it was: {moved} of {} texels changed", before.len());

    execute(&mut s, "layer.decal.set", &json!({"layer": layer.0, "offset": [0.4, 0.0], "merge": true})).unwrap();
    assert_ne!(snapshot(&mut s), after, "then the frame moves the image");
    let Projection::Planar { placed: Some(slid), .. } = projection(&s, layer) else { panic!("still placed") };
    assert_eq!((slid.offset, slid.size), ([0.4, 0.0], frame.size));

    s.undo().unwrap();
    assert_eq!(projection(&s, layer), axis, "placing and moving is one step");

    set(&mut s, layer, json!({"projection": "warp", "warpCorners": [[-0.5, 0.5], [0.9, 0.9], [0.5, -0.5], [-0.5, -0.5]]}));
    execute(&mut s, "layer.decal.set", &json!({"layer": layer.0, "rotation": 30})).unwrap();
    let Projection::Warp { frame, corners, .. } = projection(&s, layer) else { panic!("still a warp") };
    assert_eq!((frame.rotation, corners[1]), (30.0, [0.9, 0.9]), "a warp's frame turns and keeps its corners");
    set(&mut s, layer, json!({"projection": "spherical"}));
    assert!(execute(&mut s, "layer.decal.set", &json!({"layer": layer.0, "rotation": 30})).is_err(), "a spherical projection has no frame");
}

/// A 64 × 8 PNG whose red climbs from left to right
fn ramp_png() -> Vec<u8> {
    let mut out = Vec::new();
    let ramp = image::RgbaImage::from_fn(64, 8, |x, _| image::Rgba([(x * 4) as u8, 0, 0, 255]));
    ramp.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).unwrap();
    out
}
