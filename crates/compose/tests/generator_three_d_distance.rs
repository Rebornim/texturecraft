//! The 3D Distance generator on a sphere: white at its point, falling off over its radius, slid by its offset

use std::sync::Arc;

use texturecraft_compose::{BakedMaps, Inputs, PixelStore, mask_at};
use texturecraft_core::{
    BakedMap, Channel, DistanceParameters, Effect, EffectKind, Fill, Generator, GeneratorKind, Layer, LayerId, LayerKind, Mask, MaskBackground, Project,
    Source, TextureSet,
};
use texturecraft_geom::{Mesh, UvGBuffer, primitives};

const RES: u32 = 64;

struct Fixture {
    project: Project,
    mesh: Mesh,
    gbuf: UvGBuffer,
    layer: LayerId,
    baked: BakedMaps,
}

/// A unit sphere with a white fill layer
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
    Fixture { project, mesh, gbuf, layer, baked }
}

/// Every covered texel and its height on the sphere
fn covered(fixture: &Fixture) -> Vec<((u32, u32), f32)> {
    (0..RES * RES).filter_map(|index| fixture.gbuf.surface(&fixture.mesh, index as usize).map(|surface| ((index % RES, index / RES), surface.pos.y))).collect()
}

/// The mask of a 3D Distance generator with `distance` alone over black at `texels`
fn mask(fixture: &mut Fixture, distance: DistanceParameters, texels: &[(u32, u32)]) -> Vec<f32> {
    let generator = Generator { distance: Some(Box::new(distance)), ..Generator::new(GeneratorKind::ThreeDDistance) };
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
        images: &Default::default(),
        bounds: fixture.mesh.bounds(),
    };
    mask_at(&inputs, fixture.layer, texels).unwrap()
}

#[test]
fn a_point_at_the_top_lights_the_top() {
    let mut fixture = fixture();
    let texels = covered(&fixture);
    let at: Vec<(u32, u32)> = texels.iter().map(|(texel, _)| *texel).collect();

    let values = mask(&mut fixture, DistanceParameters { point: [0.5, 1.0, 0.5], radius: 0.8, contrast: 0.0, ..DistanceParameters::default() }, &at);
    let mean_where = |above: bool| {
        let picked: Vec<f32> = texels.iter().zip(&values).filter(|((_, height), _)| (*height > 0.5) == above).map(|(_, value)| *value).collect();
        picked.iter().sum::<f32>() / picked.len() as f32
    };
    assert!(mean_where(true) > mean_where(false) + 0.3, "top {} bottom {}", mean_where(true), mean_where(false));
}

#[test]
fn the_offset_slides_the_mask_from_black_to_white() {
    let mut fixture = fixture();
    let at: Vec<(u32, u32)> = covered(&fixture).iter().map(|(texel, _)| *texel).collect();

    let black = mask(&mut fixture, DistanceParameters { offset: 0.0, ..DistanceParameters::default() }, &at);
    let white = mask(&mut fixture, DistanceParameters { offset: 1.0, ..DistanceParameters::default() }, &at);
    assert!(black.iter().all(|value| *value == 0.0));
    assert!(white.iter().all(|value| *value == 1.0));
}

#[test]
fn a_larger_radius_reaches_further() {
    let mut fixture = fixture();
    let at: Vec<(u32, u32)> = covered(&fixture).iter().map(|(texel, _)| *texel).collect();
    let total = |values: Vec<f32>| values.iter().sum::<f32>();

    let small = total(mask(&mut fixture, DistanceParameters { radius: 0.3, ..DistanceParameters::default() }, &at));
    let large = total(mask(&mut fixture, DistanceParameters { radius: 1.2, ..DistanceParameters::default() }, &at));
    assert!(large > small);
}
