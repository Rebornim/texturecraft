//! The Grayscale Conversion generator on a plane filling its UV tile: its image turned gray by the chosen type, tiled, turned and balanced

use std::sync::Arc;

use texturecraft_compose::{BakedMaps, ImageRes, ImageStore, Inputs, PixelStore, mask_at};
use texturecraft_core::{
    Channel, Effect, EffectKind, Fill, Generator, GeneratorKind, GrayscaleConversion, GrayscaleConversionParameters, KindControls, Layer, LayerId, LayerKind,
    Mask, MaskBackground, Project, Source, TextureSet,
};
use texturecraft_geom::{Mesh, UvGBuffer, primitives};

const RES: u32 = 64;

/// Width of the image: its left half red, its right half blue
const IMAGE_WIDTH: u32 = 16;

struct Fixture {
    project: Project,
    mesh: Mesh,
    gbuf: UvGBuffer,
    layer: LayerId,
    images: ImageStore,
}

fn fixture() -> Fixture {
    let mesh = primitives::plane(2.0, 1);
    let gbuf = UvGBuffer::new(&mesh, 0, RES, RES);
    let mut project = Project::new("t", "builtin:plane");
    project.texture_sets.push(TextureSet::new("A", 0, RES));
    let id = project.alloc_id();
    let mut fill = Fill::default();
    fill.sources.insert(Channel::BaseColor, Source::Uniform([1.0; 3]));
    let layer = project.insert_layer(0, Layer::new(id, "Fill", LayerKind::Fill(fill), &[Channel::BaseColor]), None, false).unwrap();
    let texel = |index: u32| if index % IMAGE_WIDTH < IMAGE_WIDTH / 2 { [1.0, 0.0, 0.0, 1.0] } else { [0.0, 0.0, 1.0, 1.0] };
    let image = ImageRes { width: IMAGE_WIDTH, height: IMAGE_WIDTH, rgba: (0..IMAGE_WIDTH * IMAGE_WIDTH).map(texel).collect(), srgb: false };
    Fixture { project, mesh, gbuf, layer, images: [("split".to_string(), Arc::new(image))].into() }
}

/// The values along a row a quarter of the way down, left to right
fn row(fixture: &mut Fixture, grayscale: GrayscaleConversionParameters) -> Vec<f32> {
    let generator = Generator {
        controls: Some(Box::new(KindControls::Grayscale(GrayscaleConversionParameters { source: Some(Source::image("split")), ..grayscale }))),
        ..Generator::new(GeneratorKind::GrayscaleConversion)
    };
    let mut mask = Mask::new(MaskBackground::Black);
    mask.effects.push(Effect::new(fixture.project.alloc_id(), EffectKind::Generator(generator)));
    fixture.project.layer_mut(fixture.layer).unwrap().mask = Some(mask);
    let baked = BakedMaps::default();
    let inputs = Inputs {
        project: &fixture.project,
        ts: 0,
        mesh: &fixture.mesh,
        gbuf: &fixture.gbuf,
        pixels: &PixelStore::default(),
        baked: &baked,
        block_scales: fixture.project.texture_sets[0].block_scales(),
        preview: None,
        images: &fixture.images,
        bounds: fixture.mesh.bounds(),
    };
    let texels: Vec<(u32, u32)> = (0..RES).map(|x| (x, RES / 4)).collect();
    mask_at(&inputs, fixture.layer, &texels).unwrap()
}

fn red_channel() -> GrayscaleConversionParameters {
    GrayscaleConversionParameters { grayscale: GrayscaleConversion::Red, ..GrayscaleConversionParameters::default() }
}

/// How many times the row changes between dark and light
fn edges(values: &[f32]) -> usize {
    values.windows(2).filter(|pair| (pair[0] > 0.5) != (pair[1] > 0.5)).count()
}

#[test]
fn the_type_picks_what_turns_white() {
    let mut fixture = fixture();
    let red = row(&mut fixture, red_channel());
    let blue = row(&mut fixture, GrayscaleConversionParameters { grayscale: GrayscaleConversion::Blue, ..GrayscaleConversionParameters::default() });

    assert!(red[2] > 0.9 && red[RES as usize - 3] < 0.1, "red on the left: {red:?}");
    assert!(blue[2] < 0.1 && blue[RES as usize - 3] > 0.9, "blue on the right");
}

#[test]
fn tile_repeats_the_image_and_a_half_turn_mirrors_it() {
    let mut fixture = fixture();
    let once = row(&mut fixture, red_channel());
    let four = row(&mut fixture, GrayscaleConversionParameters { tile: 4.0, ..red_channel() });
    let turned = row(&mut fixture, GrayscaleConversionParameters { rotation: 180.0, ..red_channel() });

    assert!(edges(&four) > edges(&once) + 4, "{} vs {}", edges(&four), edges(&once));
    assert!(turned[2] < 0.1 && turned[RES as usize - 3] > 0.9, "the red half swaps sides");
}

#[test]
fn safe_rotation_turns_by_quarter_turns_only() {
    let mut fixture = fixture();
    let snapped = row(&mut fixture, GrayscaleConversionParameters { rotation: 170.0, ..red_channel() });
    let half = row(&mut fixture, GrayscaleConversionParameters { rotation: 180.0, ..red_channel() });

    assert_eq!(snapped, half);
}
