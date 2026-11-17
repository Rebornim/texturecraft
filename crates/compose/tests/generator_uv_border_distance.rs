//! The UV Border Distance generator on a plane filling its UV tile: white at the tile's edge, fading inwards over its distance, softened by its smoothness

use texturecraft_compose::{BakedMaps, Inputs, PixelStore, mask_at};
use texturecraft_core::{
    BorderDistanceParameters, Channel, Effect, EffectKind, Fill, Generator, GeneratorKind, KindControls, Layer, LayerId, LayerKind, Mask, MaskBackground,
    Project, Source, TextureSet,
};
use texturecraft_geom::{Mesh, UvGBuffer, primitives};

const RES: u32 = 64;

struct Fixture {
    project: Project,
    mesh: Mesh,
    gbuf: UvGBuffer,
    layer: LayerId,
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
    Fixture { project, mesh, gbuf, layer }
}

/// The middle row's values, left edge to centre
fn row(fixture: &mut Fixture, border: BorderDistanceParameters) -> Vec<f32> {
    let generator = Generator { controls: Some(Box::new(KindControls::BorderDistance(border))), ..Generator::new(GeneratorKind::UvBorderDistance) };
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
        images: &Default::default(),
        bounds: fixture.mesh.bounds(),
    };
    let texels: Vec<(u32, u32)> = (0..RES / 2).map(|x| (x, RES / 2)).collect();
    mask_at(&inputs, fixture.layer, &texels).unwrap()
}

#[test]
fn the_border_is_white_and_the_inside_fades_to_black() {
    let mut fixture = fixture();
    let values = row(&mut fixture, BorderDistanceParameters { distance: 0.5, ..BorderDistanceParameters::default() });

    assert!(values[0] > 0.9, "the edge, a texel in: {}", values[0]);
    assert!(values.windows(2).all(|pair| pair[1] <= pair[0]), "falling inwards: {values:?}");
    assert!(*values.last().unwrap() < 0.05, "half a tile in is black at distance 0.5");
}

#[test]
fn a_larger_distance_reaches_further_in() {
    let mut fixture = fixture();
    let near = row(&mut fixture, BorderDistanceParameters { distance: 0.1, ..BorderDistanceParameters::default() });
    let far = row(&mut fixture, BorderDistanceParameters { distance: 0.6, ..BorderDistanceParameters::default() });

    assert!(far.iter().sum::<f32>() > near.iter().sum::<f32>() + 1.0);
}

#[test]
fn smoothness_darkens_the_edge_with_the_space_around_the_island() {
    let mut fixture = fixture();
    let sharp = row(&mut fixture, BorderDistanceParameters::default());
    let smooth = row(&mut fixture, BorderDistanceParameters { smoothness: 1.0, ..BorderDistanceParameters::default() });

    assert!(smooth[0] < sharp[0] - 0.2, "{} vs {}", smooth[0], sharp[0]);
}
