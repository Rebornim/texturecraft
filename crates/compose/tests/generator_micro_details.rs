//! A generator's micro details on a flat plane: a height bump reads as an edge on Curvature and occludes its foot on Ambient Occlusion; off or flat, they change nothing

use std::sync::Arc;

use texturecraft_compose::{BakedMaps, ImageRes, ImageStore, Inputs, PixelStore, mask_at};
use texturecraft_core::{
    BakedMap, Channel, CurvatureDetail, CurvatureMode, Effect, EffectKind, Fill, Generator, GeneratorKind, Layer, LayerId, LayerKind, Mask, MaskBackground,
    MicroCurvatureType, MicroDetails, Project, Source, TextureSet,
};
use texturecraft_geom::{Mesh, UvGBuffer, primitives};

const RES: u32 = 128;

/// Size of the bump image, and the bump's spread in its texels
const BUMP_SIZE: u32 = 64;
const BUMP_SIGMA: f32 = 4.0;

/// How steep the bump's normal image is drawn
const NORMAL_STRENGTH: f32 = 8.0;

struct Fixture {
    project: Project,
    mesh: Mesh,
    gbuf: UvGBuffer,
    layer: LayerId,
    baked: BakedMaps,
    images: ImageStore,
}

/// The bump's height at texel (`x`, `y`) of the bump image: 1 at its center, 0 far away
fn bump_height(x: f32, y: f32) -> f32 {
    let center = BUMP_SIZE as f32 / 2.0;
    let distance_squared = (x - center).powi(2) + (y - center).powi(2);
    (-distance_squared / (2.0 * BUMP_SIGMA * BUMP_SIGMA)).exp()
}

/// A plane with a white fill layer, flat curvature and no occlusion, and the images `bump` (a height), `bump_normal` and `flat`
fn fixture() -> Fixture {
    let mesh = primitives::plane(2.0, 1);
    let gbuf = UvGBuffer::new(&mesh, 0, RES, RES);
    let mut project = Project::new("t", "builtin:plane");
    project.texture_sets.push(TextureSet::new("A", 0, RES));
    let id = project.alloc_id();
    let mut fill = Fill::default();
    fill.sources.insert(Channel::BaseColor, Source::Uniform([1.0; 3]));
    let layer = project.insert_layer(0, Layer::new(id, "Fill", LayerKind::Fill(fill), &[Channel::BaseColor]), None, false).unwrap();
    let texels = (RES * RES) as usize;
    let maps = [(BakedMap::AmbientOcclusion, Arc::new(vec![1.0; texels])), (BakedMap::Curvature, Arc::new(vec![0.5; texels]))];
    let baked = BakedMaps { resolution: RES, maps: maps.into() };

    let texel_xy = |index: u32| ((index % BUMP_SIZE) as f32 + 0.5, (index / BUMP_SIZE) as f32 + 0.5);
    let height = |index: u32| {
        let (x, y) = texel_xy(index);
        let height = bump_height(x, y);
        [height, height, height, 1.0]
    };
    let normal = |index: u32| {
        let (x, y) = texel_xy(index);
        let slope = ((bump_height(x + 0.5, y) - bump_height(x - 0.5, y)), (bump_height(x, y + 0.5) - bump_height(x, y - 0.5)));
        let (normal_x, normal_y) = (-NORMAL_STRENGTH * slope.0, -NORMAL_STRENGTH * slope.1);
        let length = (normal_x * normal_x + normal_y * normal_y + 1.0).sqrt();
        [normal_x / length * 0.5 + 0.5, normal_y / length * 0.5 + 0.5, 0.5 / length + 0.5, 1.0]
    };
    let image = |texel: &dyn Fn(u32) -> [f32; 4]| {
        Arc::new(ImageRes { width: BUMP_SIZE, height: BUMP_SIZE, rgba: (0..BUMP_SIZE * BUMP_SIZE).map(texel).collect(), srgb: false })
    };
    let flat = Arc::new(ImageRes { width: 4, height: 4, rgba: vec![[0.3, 0.3, 0.3, 1.0]; 16], srgb: false });
    let images = [("bump".to_string(), image(&height)), ("bump_normal".to_string(), image(&normal)), ("flat".to_string(), flat)].into();
    Fixture { project, mesh, gbuf, layer, baked, images }
}

/// Every texel, row by row
fn texels() -> Vec<(u32, u32)> {
    (0..RES * RES).map(|index| (index % RES, index / RES)).collect()
}

/// The texel under the bump's center
fn bump_center() -> usize {
    (RES / 2 * RES + RES / 2) as usize
}

/// The mask of `generator` alone over black at every texel
fn mask(fixture: &mut Fixture, generator: Generator) -> Vec<f32> {
    let mut mask = Mask::new(MaskBackground::Black);
    mask.effects.push(Effect::new(fixture.project.alloc_id(), EffectKind::Generator(generator)));
    fixture.project.layer_mut(fixture.layer).unwrap().mask = Some(mask);
    let inputs = Inputs {
        project: &fixture.project,
        ts: 0,
        mesh: &fixture.mesh,
        gbuf: &fixture.gbuf,
        pixels: &PixelStore::default(),
        baked: &fixture.baked,
        block_scales: fixture.project.texture_sets[0].block_scales(),
        preview: None,
        images: &fixture.images,
        bounds: fixture.mesh.bounds(),
    };
    mask_at(&inputs, fixture.layer, &texels()).unwrap()
}

/// `kind` with micro details reading `height` (and `normal` when given), edited by `edit`
fn with_micro(kind: GeneratorKind, height: Option<&str>, normal: Option<&str>, edit: impl Fn(&mut MicroDetails)) -> Generator {
    let mut micro = MicroDetails {
        enabled: true,
        height: height.is_some(),
        normal: normal.is_some(),
        height_source: height.map(Source::image),
        normal_source: normal.map(Source::image),
        ..MicroDetails::defaults(kind).unwrap()
    };
    edit(&mut micro);
    let mut generator = Generator::new(kind);
    if generator.wear.is_none() && kind != GeneratorKind::FiberGlassEdgeWear && generator.section.is_none() {
        generator.wear = texturecraft_core::WearParameters::defaults(kind);
    }
    generator.micro = Some(Box::new(micro));
    generator
}

/// Curvature in Raw mode, so a lift and a drop both show
fn raw_curvature(mut generator: Generator) -> Generator {
    generator.detail = Some(CurvatureDetail { mode: CurvatureMode::Raw, ..CurvatureDetail::default() });
    generator
}

fn strong(micro: &mut MicroDetails) {
    (micro.curvature_intensity, micro.height_intensity) = (5.0, 10.0);
}

fn mean(values: &[f32]) -> f32 {
    values.iter().sum::<f32>() / values.len() as f32
}

#[test]
fn micro_details_switched_off_change_nothing() {
    let mut fixture = fixture();
    for kind in [GeneratorKind::Curvature, GeneratorKind::AmbientOcclusion, GeneratorKind::EdgeWear, GeneratorKind::Dirt, GeneratorKind::FiberGlassEdgeWear] {
        let plain = mask(&mut fixture, with_micro(kind, None, None, |_| {}));
        // Curvature and Ambient Occlusion switch them off whole, the wear kinds by both their images
        let switch = MicroDetails::has_switch(kind);
        let off = |micro: &mut MicroDetails| (micro.enabled, micro.height, micro.normal) = (!switch, switch, switch);

        let switched_off = mask(&mut fixture, with_micro(kind, Some("bump"), Some("bump_normal"), off));
        assert_eq!(plain, switched_off, "{kind:?}");
    }
}

#[test]
fn a_micro_height_bump_reads_as_an_edge_on_curvature() {
    let mut fixture = fixture();
    let off = mask(&mut fixture, Generator::new(GeneratorKind::Curvature));

    let on = mask(&mut fixture, with_micro(GeneratorKind::Curvature, Some("bump"), None, strong));
    assert!(on[bump_center()] > off[bump_center()] + 0.01, "the bump's top is convex: {} vs {}", on[bump_center()], off[bump_center()]);
    assert!((on[0] - off[0]).abs() < 1e-4, "far from the bump the surface stays flat");
}

#[test]
fn a_micro_normal_bump_reads_as_an_edge_on_curvature() {
    let mut fixture = fixture();
    let off = mask(&mut fixture, Generator::new(GeneratorKind::Curvature));

    let on = mask(&mut fixture, with_micro(GeneratorKind::Curvature, None, Some("bump_normal"), strong));
    assert!(on[bump_center()] > off[bump_center()] + 0.01, "{} vs {}", on[bump_center()], off[bump_center()]);
}

#[test]
fn standard_lifts_a_flat_micro_height_and_smooth_lowers_it() {
    let mut fixture = fixture();
    let typed = |curvature_type| move |micro: &mut MicroDetails| micro.curvature_type = curvature_type;
    let sobel = mean(&mask(&mut fixture, raw_curvature(with_micro(GeneratorKind::Curvature, Some("flat"), None, typed(MicroCurvatureType::Sobel)))));
    let off = mean(&mask(&mut fixture, raw_curvature(Generator::new(GeneratorKind::Curvature))));

    let standard = mean(&mask(&mut fixture, raw_curvature(with_micro(GeneratorKind::Curvature, Some("flat"), None, typed(MicroCurvatureType::Standard)))));
    let smooth = mean(&mask(&mut fixture, raw_curvature(with_micro(GeneratorKind::Curvature, Some("flat"), None, typed(MicroCurvatureType::Smooth)))));
    assert!((sobel - off).abs() < 1e-4, "Sobel adds nothing on a flat height");
    assert!(standard > off + 1e-3 && smooth < off - 1e-4, "standard {standard}, off {off}, smooth {smooth}");
}

#[test]
fn a_micro_height_bump_occludes_around_it_on_ambient_occlusion() {
    let mut fixture = fixture();
    let deep = |micro: &mut MicroDetails| (micro.ao_radius, micro.ao_depth) = (1.0, 1.0);
    let off = mask(&mut fixture, Generator::new(GeneratorKind::AmbientOcclusion));
    let flat = mask(&mut fixture, with_micro(GeneratorKind::AmbientOcclusion, Some("flat"), None, deep));

    let on = mask(&mut fixture, with_micro(GeneratorKind::AmbientOcclusion, Some("bump"), None, deep));
    assert_eq!(off, flat, "a flat micro height occludes nothing");
    assert!(on.iter().zip(&off).all(|(on, off)| on <= off), "micro occlusion only darkens");
    assert!(mean(&on) < mean(&off) - 1e-3, "{} vs {}", mean(&on), mean(&off));
}

#[test]
fn a_tiny_ao_radius_occludes_nothing() {
    let mut fixture = fixture();
    let off = mask(&mut fixture, Generator::new(GeneratorKind::AmbientOcclusion));

    let tiny = mask(&mut fixture, with_micro(GeneratorKind::AmbientOcclusion, Some("bump"), None, |micro| micro.ao_radius = 0.05));
    assert_eq!(off, tiny);
}

#[test]
fn a_micro_height_bump_moves_the_wear_kinds_around_it() {
    let mut fixture = fixture();
    for kind in [GeneratorKind::EdgeWear, GeneratorKind::Dirt, GeneratorKind::FiberGlassEdgeWear] {
        let off = mask(&mut fixture, with_micro(kind, None, None, |_| {}));

        let on = mask(&mut fixture, with_micro(kind, Some("bump"), None, |micro| micro.curvature_intensity = 1.0));
        let changed = on.iter().zip(&off).filter(|(on, off)| (*on - *off).abs() > 1e-3).count();
        assert!(changed > 0, "{kind:?}");
    }
}

#[test]
fn saved_micro_values_out_of_range_render_as_their_nearest_valid_ones() {
    let mut fixture = fixture();
    let hostile =
        mask(&mut fixture, with_micro(GeneratorKind::AmbientOcclusion, Some("bump"), None, |micro| (micro.ao_radius, micro.ao_depth) = (1e30, f32::NAN)));
    let valid = mask(&mut fixture, with_micro(GeneratorKind::AmbientOcclusion, Some("bump"), None, |micro| micro.ao_radius = 1.0));

    assert_eq!(hostile, valid);
}
