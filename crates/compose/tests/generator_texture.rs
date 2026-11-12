//! A generator's texture (Use Texture) on a sphere: read in UV or triplanar, turned gray, blended over the signal before the global controls

use std::sync::Arc;

use texturecraft_compose::{BakedMaps, ImageRes, ImageStore, Inputs, PixelStore, mask_at};
use texturecraft_core::{
    BakedMap, Channel, Effect, EffectKind, Fill, Generator, GeneratorKind, GrayscaleConversion, Layer, LayerId, LayerKind, Mask, MaskBackground, Project,
    SectionParameters, Source, TextureBlend, TextureLayer, TextureSet,
};
use texturecraft_geom::glam::Vec3;
use texturecraft_geom::{Mesh, UvGBuffer, primitives};

const RES: u32 = 128;

/// How far a mask value may sit from the expected one
const TOLERANCE: f32 = 1e-3;

/// The ambient occlusion every texel of the sphere reads
const OCCLUSION: f32 = 0.3;

/// Width of the split image: its left half black, its right half white
const SPLIT_WIDTH: u32 = 64;

/// How far from a split's edge (in UV) a texel must sit to read one side only
const SPLIT_MARGIN: f32 = 0.05;

struct Fixture {
    project: Project,
    mesh: Mesh,
    gbuf: UvGBuffer,
    layer: LayerId,
    baked: BakedMaps,
    images: ImageStore,
}

/// A unit sphere with a white fill layer, uniform occlusion and the images `flat` (one color) and `split` (black left, white right)
fn fixture(flat: [f32; 3]) -> Fixture {
    let mesh = primitives::sphere(1.0, 32, 16);
    let gbuf = UvGBuffer::new(&mesh, 0, RES, RES);
    let mut project = Project::new("t", "builtin:sphere");
    project.texture_sets.push(TextureSet::new("A", 0, RES));
    let id = project.alloc_id();
    let mut fill = Fill::default();
    fill.sources.insert(Channel::BaseColor, Source::Uniform([1.0; 3]));
    let layer = project.insert_layer(0, Layer::new(id, "Fill", LayerKind::Fill(fill), &[Channel::BaseColor]), None, false).unwrap();
    let baked = BakedMaps { resolution: RES, maps: [(BakedMap::AmbientOcclusion, Arc::new(vec![OCCLUSION; (RES * RES) as usize]))].into() };

    let [red, green, blue] = flat;
    let flat = ImageRes { width: 4, height: 4, rgba: vec![[red, green, blue, 1.0]; 16], srgb: false };
    let split_texel = |index: u32| if index % SPLIT_WIDTH < SPLIT_WIDTH / 2 { [0.0, 0.0, 0.0, 1.0] } else { [1.0; 4] };
    let split = ImageRes { width: SPLIT_WIDTH, height: SPLIT_WIDTH, rgba: (0..SPLIT_WIDTH * SPLIT_WIDTH).map(split_texel).collect(), srgb: false };
    let images = [("flat".to_string(), Arc::new(flat)), ("split".to_string(), Arc::new(split))].into();
    Fixture { project, mesh, gbuf, layer, baked, images }
}

/// A covered texel with its UV, world position and normal
struct SurfacePoint {
    texel: (u32, u32),
    uv: [f32; 2],
    position: Vec3,
    normal: Vec3,
}

/// Every covered texel
fn surface(fixture: &Fixture) -> Vec<SurfacePoint> {
    (0..RES * RES)
        .filter_map(|index| {
            let texel = (index % RES, index / RES);
            let uv = [(texel.0 as f32 + 0.5) / RES as f32, (texel.1 as f32 + 0.5) / RES as f32];
            fixture.gbuf.surface(&fixture.mesh, index as usize).map(|surface| SurfacePoint { texel, uv, position: surface.pos, normal: surface.nrm })
        })
        .collect()
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

/// Ambient Occlusion on its section controls with `texture` reading image `image`
fn occlusion_with(image: &str, texture: TextureLayer) -> Generator {
    Generator { texture: Some(TextureLayer { source: Some(Source::image(image)), ..texture }), ..Generator::new(GeneratorKind::AmbientOcclusion) }
}

/// A texture turned on at full opacity, replacing the signal
fn replacing() -> TextureLayer {
    TextureLayer { enabled: true, opacity: 1.0, blend: TextureBlend::Normal, ..TextureLayer::default() }
}

fn assert_all(actual: &[f32], expected: f32, what: &str) {
    for (index, actual) in actual.iter().enumerate() {
        assert!((actual - expected).abs() < TOLERANCE, "{what}: texel {index} is {actual}, expected {expected}");
    }
}

#[test]
fn a_texture_turned_off_or_at_opacity_zero_leaves_the_signal() {
    let mut fixture = fixture([0.6; 3]);
    let texels: Vec<_> = surface(&fixture).iter().map(|point| point.texel).collect();

    assert_all(&mask(&mut fixture, occlusion_with("flat", TextureLayer { enabled: false, ..replacing() }), &texels), OCCLUSION, "turned off");
    assert_all(&mask(&mut fixture, occlusion_with("flat", TextureLayer { opacity: 0.0, ..replacing() }), &texels), OCCLUSION, "opacity 0");
    assert_all(&mask(&mut fixture, occlusion_with("flat", TextureLayer::default()), &texels), OCCLUSION, "the reference's defaults");
}

#[test]
fn an_empty_texture_slot_blends_black() {
    let mut fixture = fixture([0.6; 3]);
    let texels: Vec<_> = surface(&fixture).iter().map(|point| point.texel).collect();
    let empty = Generator { texture: Some(TextureLayer { opacity: 0.5, ..replacing() }), ..Generator::new(GeneratorKind::AmbientOcclusion) };
    assert_all(&mask(&mut fixture, empty, &texels), OCCLUSION * 0.5, "half way to black");
}

#[test]
fn a_texture_needs_the_section_controls() {
    let mut fixture = fixture([0.6; 3]);
    let texels: Vec<_> = surface(&fixture).iter().map(|point| point.texel).collect();
    let earlier = Generator { section: None, global: Default::default(), ..occlusion_with("flat", replacing()) };
    let without = Generator { texture: None, ..earlier.clone() };
    assert_eq!(mask(&mut fixture, earlier, &texels), mask(&mut fixture, without, &texels));
}

#[test]
fn the_texture_turns_gray_by_its_conversion() {
    let mut fixture = fixture([1.0, 0.25, 0.0]);
    let texels: Vec<_> = surface(&fixture).iter().map(|point| point.texel).collect();
    for (grayscale, expected) in [
        (GrayscaleConversion::Average, 1.25 / 3.0),
        (GrayscaleConversion::Red, 1.0),
        (GrayscaleConversion::Green, 0.25),
        (GrayscaleConversion::Desaturation, 0.5),
        (GrayscaleConversion::Luma, 0.299 + 0.587 * 0.25),
    ] {
        let generator = occlusion_with("flat", TextureLayer { grayscale, ..replacing() });
        assert_all(&mask(&mut fixture, generator, &texels), expected, &format!("{grayscale:?}"));
    }
}

#[test]
fn overlay_with_a_mid_gray_texture_doubles_a_dark_signal_times_the_gray() {
    let mut fixture = fixture([0.4; 3]);
    let texels: Vec<_> = surface(&fixture).iter().map(|point| point.texel).collect();
    let generator = occlusion_with("flat", TextureLayer { blend: TextureBlend::Overlay, opacity: 0.5, ..replacing() });
    let overlay = 2.0 * OCCLUSION * 0.4;
    assert_all(&mask(&mut fixture, generator, &texels), OCCLUSION + (overlay - OCCLUSION) * 0.5, "overlay at half opacity");
}

#[test]
fn the_texture_blends_after_the_section_controls_and_before_the_global_ones() {
    let mut fixture = fixture([0.6; 3]);
    let texels: Vec<_> = surface(&fixture).iter().map(|point| point.texel).collect();

    let mut section_inverted = occlusion_with("flat", replacing());
    section_inverted.section = Some(SectionParameters { invert: true, ..section_inverted.section.unwrap() });
    assert_all(&mask(&mut fixture, section_inverted, &texels), 0.6, "the texture replaces the inverted section");

    let globally_inverted = Generator { invert: true, ..occlusion_with("flat", replacing()) };
    assert_all(&mask(&mut fixture, globally_inverted, &texels), 0.4, "the global invert flips the texture");

    let mut blurred = occlusion_with("flat", replacing());
    blurred.section = Some(SectionParameters { blur: 2.0, ..blurred.section.unwrap() });
    assert_all(&mask(&mut fixture, blurred, &texels), 0.6, "with the section blur (computed over a region)");
}

#[test]
fn uv_scale_repeats_the_texture_about_the_centre() {
    let mut fixture = fixture([0.0; 3]);
    let points = surface(&fixture);
    let side = |u: f32| (u.rem_euclid(1.0) >= 0.5) as u8 as f32;
    for scale in [1.0, 2.0, 3.0] {
        let read_at = |u: f32| (u - 0.5) * scale + 0.5;
        let clear: Vec<_> = points
            .iter()
            .filter(|point| {
                let u = read_at(point.uv[0]).rem_euclid(1.0);
                [0.0, 0.5, 1.0].iter().all(|edge| (u - edge).abs() > SPLIT_MARGIN * scale)
            })
            .collect();
        let texels: Vec<_> = clear.iter().map(|point| point.texel).collect();
        let values = mask(&mut fixture, occlusion_with("split", TextureLayer { scale, ..replacing() }), &texels);
        for (point, value) in clear.iter().zip(values) {
            let expected = side(read_at(point.uv[0]));
            assert!((value - expected).abs() < TOLERANCE, "scale {scale} at u {}: {value}, expected {expected}", point.uv[0]);
        }
    }
}

#[test]
fn triplanar_reads_the_top_from_the_plane_facing_up() {
    let mut fixture = fixture([0.0; 3]);
    let (low, high) = fixture.mesh.bounds();
    let across = |position: Vec3| (position.x - low.x) / (high.x - low.x);
    let top: Vec<_> = surface(&fixture).into_iter().filter(|point| point.normal.y > 0.95 && (across(point.position) - 0.5).abs() > SPLIT_MARGIN).collect();
    assert!(!top.is_empty());

    let texels: Vec<_> = top.iter().map(|point| point.texel).collect();
    let sharp = TextureLayer { triplanar: true, triplanar_contrast: 1.0, ..replacing() };
    let values = mask(&mut fixture, occlusion_with("split", sharp), &texels);
    for (point, value) in top.iter().zip(values) {
        let expected = if across(point.position) >= 0.5 { 1.0 } else { 0.0 };
        assert!((value - expected).abs() < TOLERANCE, "at x {}: {value}, expected {expected}", across(point.position));
    }
}
