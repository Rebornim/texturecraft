//! A wear generator's custom grunge on a sphere: read instead of the kind's own pattern, tiled by the grunge scale, the seed leaving it in place

use std::sync::Arc;

use texturecraft_compose::{BakedMaps, ImageRes, ImageStore, Inputs, PixelStore, mask_at};
use texturecraft_core::{
    BakedMap, Channel, Effect, EffectKind, Fill, Generator, GeneratorKind, Layer, LayerId, LayerKind, Mask, MaskBackground, Project, Source, TextureSet,
    WearParameters,
};
use texturecraft_geom::{Mesh, UvGBuffer, primitives};

const RES: u32 = 128;

/// Width of the split image: its left half black, its right half white
const SPLIT_WIDTH: u32 = 64;

const WEAR_KINDS: [GeneratorKind; 3] = [GeneratorKind::EdgeWear, GeneratorKind::FiberGlassEdgeWear, GeneratorKind::Dirt];

struct Fixture {
    project: Project,
    mesh: Mesh,
    gbuf: UvGBuffer,
    layer: LayerId,
    baked: BakedMaps,
    images: ImageStore,
}

/// A unit sphere with a white fill layer, no occlusion anywhere, no curvature map, and the images `black`, `white` and `split`
fn fixture() -> Fixture {
    let mesh = primitives::sphere(1.0, 32, 16);
    let gbuf = UvGBuffer::new(&mesh, 0, RES, RES);
    let mut project = Project::new("t", "builtin:sphere");
    project.texture_sets.push(TextureSet::new("A", 0, RES));
    let id = project.alloc_id();
    let mut fill = Fill::default();
    fill.sources.insert(Channel::BaseColor, Source::Uniform([1.0; 3]));
    let layer = project.insert_layer(0, Layer::new(id, "Fill", LayerKind::Fill(fill), &[Channel::BaseColor]), None, false).unwrap();
    let baked = BakedMaps { resolution: RES, maps: [(BakedMap::AmbientOcclusion, Arc::new(vec![1.0; (RES * RES) as usize]))].into() };

    let flat = |value: f32| Arc::new(ImageRes { width: 4, height: 4, rgba: vec![[value, value, value, 1.0]; 16], srgb: false });
    let split_texel = |index: u32| if index % SPLIT_WIDTH < SPLIT_WIDTH / 2 { [0.0, 0.0, 0.0, 1.0] } else { [1.0; 4] };
    let split = ImageRes { width: SPLIT_WIDTH, height: SPLIT_WIDTH, rgba: (0..SPLIT_WIDTH * SPLIT_WIDTH).map(split_texel).collect(), srgb: false };
    let images = [("black".to_string(), flat(0.0)), ("white".to_string(), flat(1.0)), ("split".to_string(), Arc::new(split))].into();
    Fixture { project, mesh, gbuf, layer, baked, images }
}

/// Every covered texel
fn covered(fixture: &Fixture) -> Vec<(u32, u32)> {
    (0..RES * RES).filter(|index| fixture.gbuf.surface(&fixture.mesh, *index as usize).is_some()).map(|index| (index % RES, index / RES)).collect()
}

/// The mask of `generator` alone over black at `texels`
fn mask(fixture: &mut Fixture, generator: Generator, texels: &[(u32, u32)]) -> Vec<f32> {
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
    mask_at(&inputs, fixture.layer, texels).unwrap()
}

/// `kind` on its wear controls at full grunge amount, reading `image` (None: an empty slot) as its grunge once `custom` is on
fn wear(kind: GeneratorKind, image: Option<&str>, custom: bool, edit: impl Fn(&mut WearParameters)) -> Generator {
    let mut wear = WearParameters { grunge_amount: 1.0, custom_grunge: custom, ..WearParameters::defaults(kind).unwrap() };
    edit(&mut wear);
    Generator { wear: Some(wear), grunge: image.map(Source::image), ..Generator::new(kind) }
}

fn mean(values: &[f32]) -> f32 {
    values.iter().sum::<f32>() / values.len() as f32
}

/// The share of texels where `first` and `second` differ
fn differing(first: &[f32], second: &[f32]) -> f32 {
    first.iter().zip(second).filter(|(a, b)| (*a - *b).abs() > 0.05).count() as f32 / first.len() as f32
}

#[test]
fn a_grunge_image_changes_nothing_until_use_custom_grunge_is_on() {
    let mut fixture = fixture();
    let texels = covered(&fixture);
    for kind in WEAR_KINDS {
        let own = mask(&mut fixture, wear(kind, None, false, |_| {}), &texels);
        assert_eq!(mask(&mut fixture, wear(kind, Some("white"), false, |_| {}), &texels), own, "{kind:?}");
        assert!(differing(&mask(&mut fixture, wear(kind, Some("white"), true, |_| {}), &texels), &own) > 0.1, "{kind:?}: on, the image replaces the pattern");
    }
}

#[test]
fn a_brighter_custom_grunge_wears_edges_more_and_dirties_less_in_uv_and_triplanar() {
    let mut fixture = fixture();
    let texels = covered(&fixture);
    for kind in WEAR_KINDS {
        for triplanar in [false, true] {
            let read = |fixture: &mut Fixture, image| mean(&mask(fixture, wear(kind, Some(image), true, |wear| wear.triplanar = triplanar), &texels));
            let (white, black) = (read(&mut fixture, "white"), read(&mut fixture, "black"));
            let (more, less) = if kind == GeneratorKind::Dirt { (black, white) } else { (white, black) };
            assert!(more > less + 0.3, "{kind:?} triplanar {triplanar}: white {white}, black {black}");
        }
    }
}

#[test]
fn an_empty_grunge_slot_reads_black() {
    let mut fixture = fixture();
    let texels = covered(&fixture);
    for kind in WEAR_KINDS {
        assert_eq!(
            mask(&mut fixture, wear(kind, None, true, |_| {}), &texels),
            mask(&mut fixture, wear(kind, Some("black"), true, |_| {}), &texels),
            "{kind:?}"
        );
    }
}

#[test]
fn the_grunge_scale_repeats_a_custom_grunge_across_the_uv_square() {
    let mut fixture = fixture();
    // Texel pairs half the UV square apart, both covered
    let covered = covered(&fixture);
    let pairs: Vec<((u32, u32), (u32, u32))> =
        covered.iter().filter(|(x, _)| *x < RES / 2).map(|(x, y)| ((*x, *y), (x + RES / 2, *y))).filter(|(_, right)| covered.contains(right)).collect();
    let (left, right): (Vec<_>, Vec<_>) = pairs.into_iter().unzip();
    let halves = |fixture: &mut Fixture, scale: f32| {
        let generator = wear(GeneratorKind::EdgeWear, Some("split"), true, |wear| wear.grunge_scale = scale);
        (mask(fixture, generator.clone(), &left), mask(fixture, generator, &right))
    };

    let (first, second) = halves(&mut fixture, 2.0);
    assert!(differing(&first, &second) < 0.02, "twice across the square, half of it apart reads the same");
    let (first, second) = halves(&mut fixture, 1.0);
    assert!(differing(&first, &second) > 0.5, "once across the square, half of it apart reads the other side of the image");
}

#[test]
fn the_seed_leaves_a_custom_grunge_in_place() {
    let mut fixture = fixture();
    let texels = covered(&fixture);
    for kind in WEAR_KINDS {
        for triplanar in [false, true] {
            let seeded = |seed| Generator { seed, ..wear(kind, Some("split"), true, |wear| wear.triplanar = triplanar) };
            assert_eq!(mask(&mut fixture, seeded(1), &texels), mask(&mut fixture, seeded(2), &texels), "{kind:?} triplanar {triplanar}");
        }
    }
}
