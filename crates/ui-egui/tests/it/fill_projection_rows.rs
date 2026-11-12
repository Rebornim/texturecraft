//! A fill's Fill section shows the rows its projection has, and each new row changes the projection

use egui::{Event, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::json;
use texturecraft_engine::core::{LayerId, LayerKind, Projection, ShapeCrop};
use texturecraft_ui_egui::TextureCraftApp;

/// Tall enough that Properties shows the whole fill without scrolling
const WINDOW_SIZE: egui::Vec2 = egui::vec2(1600.0, 1400.0);

/// Small texture and bake sizes keep the sample session fast; the panel doesn't depend on them
const TEXTURE_RESOLUTION: u32 = 256;
const BAKE_RESOLUTION: u32 = 64;

/// Frames for the first layout and fonts to settle
const STARTUP_FRAMES: usize = 4;

/// Frames for a click or selection change to reach the panel
const SETTLE_FRAMES: usize = 3;

/// The app with a new fill layer (projected as `projection`) selected in Properties, and its layer id
fn app_with_fill(projection: &str) -> (Harness<'static, TextureCraftApp>, u64) {
    let mut session = texturecraft_engine::sample_session(TEXTURE_RESOLUTION, BAKE_RESOLUTION).unwrap();
    let added = texturecraft_engine::execute(&mut session, "layer.add.fill", &json!({})).unwrap();
    let layer = added["layer"].as_u64().unwrap();
    texturecraft_engine::execute(&mut session, "layer.fill.projection", &json!({"layer": layer, "projection": projection})).unwrap();

    let mut harness = Harness::builder().with_size(WINDOW_SIZE).build_eframe(|cc| TextureCraftApp::new(cc, session));
    // The labels below are English; a first run would speak the system's language
    harness.state_mut().ui.prefs.language = "en".into();
    harness.run_steps(STARTUP_FRAMES);
    (harness, layer)
}

/// The fill layer's projection as the document holds it
fn projection(harness: &Harness<'_, TextureCraftApp>, layer: u64) -> Projection {
    match &harness.state().session.doc.project.layer(LayerId(layer)).unwrap().kind {
        LayerKind::Fill(fill) => fill.projection.clone(),
        other => panic!("not a fill: {other:?}"),
    }
}

fn click(harness: &mut Harness<'_, TextureCraftApp>, label: &str) {
    harness.get_by_label(label).click();
    harness.run_steps(SETTLE_FRAMES);
}

/// Press and release the left button at `at` without moving
fn click_at(harness: &mut Harness<'_, TextureCraftApp>, at: Pos2) {
    harness.hover_at(at);
    harness.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    harness.run_steps(1);
    harness.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    harness.run_steps(SETTLE_FRAMES);
}

#[test]
fn each_projection_shows_its_own_rows() {
    let shows = |harness: &Harness<'_, TextureCraftApp>, label: &str| harness.query_by_label(label).is_some();

    let (harness, _) = app_with_fill("planar");
    for label in ["Depth culling", "Backface culling", "Angle", "Projection Extends Outside Shape", "Offset X", "Rotation Y", "Scale Z"] {
        assert!(shows(&harness, label), "planar: {label}");
    }
    assert!(!shows(&harness, "Reset frame"), "planar has no frame to reset");

    let (harness, _) = app_with_fill("warp");
    // A warp's 3D projection settings are its frame's: where its centre is, its turn and its size
    for label in ["Projection Depth", "Reset frame", "Depth culling", "Offset X", "Rotation X", "Scale X"] {
        assert!(shows(&harness, label), "warp: {label}");
    }
    assert!(!shows(&harness, "Backface culling"), "a warp has no backface culling");

    let (harness, _) = app_with_fill("cylindrical");
    for label in ["Angle", "Backface culling", "Projection Extends Outside Shape", "Offset X"] {
        assert!(shows(&harness, label), "cylindrical: {label}");
    }
    assert!(!shows(&harness, "Depth culling"));

    let (harness, _) = app_with_fill("triplanar");
    assert!(shows(&harness, "Hardness") && !shows(&harness, "UV Wrap") && !shows(&harness, "Backface culling"));

    let (harness, _) = app_with_fill("uvTile");
    assert!(shows(&harness, "Fill (Match per UV Tile)"));
    assert!(!shows(&harness, "Rotation") && !shows(&harness, "Filtering and edges"), "per UV tile there is nothing to place");
}

#[test]
fn culling_toggles_change_the_projection() {
    let (mut harness, layer) = app_with_fill("planar");
    click(&mut harness, "Depth culling");
    click(&mut harness, "Backface culling");
    let Projection::Planar { projector, .. } = projection(&harness, layer) else { panic!("still planar") };
    assert!(!projector.depth.on, "a new planar starts depth culled; the toggle turns it off");
    assert!(projector.backface.on);
}

#[test]
fn shape_crop_is_picked_from_its_list() {
    // Tri-planar's Shape crop sits highest in the panel, in view
    let (mut harness, layer) = app_with_fill("triplanar");
    click(&mut harness, "Projection Extends Outside Shape");
    click(&mut harness, "Projection Cropped to Shape");
    let Projection::Triplanar { projector, .. } = projection(&harness, layer) else { panic!("still tri-planar") };
    assert_eq!(projector.crop, ShapeCrop::Cropped);
}

#[test]
fn triplanar_hardness_runs_from_soft_to_the_hardest_blend() {
    let (mut harness, layer) = app_with_fill("triplanar");
    let track = harness.get_by_label("Hardness").rect();
    click_at(&mut harness, Pos2::new(track.right() - 1.0, track.center().y));
    let Projection::Triplanar { sharpness, .. } = projection(&harness, layer) else { panic!("still tri-planar") };
    assert!(sharpness > 48.0, "Hardness 1 is the hardest blend, 64: {sharpness}");

    click_at(&mut harness, Pos2::new(track.left() + 1.0, track.center().y));
    let Projection::Triplanar { sharpness, .. } = projection(&harness, layer) else { panic!("still tri-planar") };
    assert!(sharpness < 1.5, "Hardness 0 is the softest, 1: {sharpness}");
}

#[test]
fn reset_frame_puts_a_warps_corners_back() {
    let (mut harness, layer) = app_with_fill("warp");
    let pulled = json!({"layer": layer, "warpCorners": [[-0.7, 0.6], [0.5, 0.5], [0.5, -0.5], [-0.5, -0.5]]});
    texturecraft_engine::execute(&mut harness.state_mut().session, "layer.fill.projection", &pulled).unwrap();
    harness.run_steps(SETTLE_FRAMES);

    click(&mut harness, "Reset frame");
    let Projection::Warp { corners, .. } = projection(&harness, layer) else { panic!("still a warp") };
    assert_eq!(corners, Projection::FRAME_CORNERS);
}

#[test]
fn the_3d_projection_settings_fold_away() {
    let (mut harness, _) = app_with_fill("spherical");
    assert!(harness.query_by_label("Scale X").is_some(), "the band starts open");
    click(&mut harness, "3D projection settings");
    assert!(harness.query_by_label("Scale X").is_none());
    assert!(harness.query_by_label("Spherical projection").is_some(), "the rows above it stay");
}

/// The fill layer as the document holds it
fn fill(harness: &Harness<'_, TextureCraftApp>, layer: u64) -> texturecraft_engine::core::Fill {
    match &harness.state().session.doc.project.layer(LayerId(layer)).unwrap().kind {
        LayerKind::Fill(fill) => fill.clone(),
        other => panic!("not a fill: {other:?}"),
    }
}

#[test]
fn the_fill_caption_resets_the_whole_fill_section_in_one_undo_step() {
    use texturecraft_engine::core::{Filtering, Sampling};
    let (mut harness, layer) = app_with_fill("planar");
    harness.state_mut().run("layer.fill.projection", json!({"layer": layer, "tiling": 3, "filtering": "nearest", "shapeCrop": "cropped", "symmetry": true}));
    harness.run_steps(SETTLE_FRAMES);
    let changed = fill(&harness, layer);
    let reset = harness.get_by_label("Fill: Reset to defaults").rect().center();
    let edges = harness.get_by_label("Restore defaults").rect().center();

    // The band's reset puts back only Filtering and edges, through a real pointer click
    click_at(&mut harness, edges);
    let after_band = fill(&harness, layer);
    assert_eq!((after_band.sampling.filtering, crop(&after_band.projection)), (Filtering::default(), Some(ShapeCrop::default())));
    assert!(matches!(after_band.projection, Projection::Planar { .. }), "the projection stays planar");
    assert!(harness.get_by_label("Restore defaults").accesskit_node().is_disabled(), "at its defaults the band's reset is inert");

    // The caption's reset puts back the whole section
    click_at(&mut harness, reset);
    let after = fill(&harness, layer);
    assert_eq!(after.projection, Projection::default(), "a UV projection at tiling 1");
    assert_eq!(after.sampling, Sampling { symmetry: changed.sampling.symmetry, ..Sampling::default() }, "the symmetry stays");
    assert!(harness.get_by_label("Fill: Reset to defaults").accesskit_node().is_disabled(), "at the defaults it is faint and inert");
    click_at(&mut harness, reset);
    assert_eq!(fill(&harness, layer), after, "a click at the defaults changes nothing");

    harness.state_mut().run("edit.undo", json!({}));
    assert_eq!(fill(&harness, layer), after_band, "one undo step brings the whole section back");
}

/// A projection's Shape crop, if it has a projector
fn crop(projection: &Projection) -> Option<ShapeCrop> {
    match projection {
        Projection::Planar { projector, .. } | Projection::Triplanar { projector, .. } => Some(projector.crop),
        _ => None,
    }
}
