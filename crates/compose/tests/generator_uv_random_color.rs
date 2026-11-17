//! The UV Random Color generator on a cube: one value per UV island, reshuffled by the seed, or picked along a gradient

use texturecraft_compose::{BakedMaps, Inputs, PixelStore, mask_at};
use texturecraft_core::{
    Channel, Effect, EffectKind, Fill, Generator, GeneratorKind, KindControls, Layer, LayerId, LayerKind, Mask, MaskBackground, Project, Source, TextureSet,
    UvRandomColorParameters,
};
use texturecraft_geom::{Mesh, TrianglePartition, UvGBuffer, primitives};

const RES: u32 = 128;

struct Fixture {
    project: Project,
    mesh: Mesh,
    gbuf: UvGBuffer,
    layer: LayerId,
}

fn fixture() -> Fixture {
    let mesh = primitives::cube(1.0);
    let gbuf = UvGBuffer::new(&mesh, 0, RES, RES);
    let mut project = Project::new("t", "builtin:cube");
    project.texture_sets.push(TextureSet::new("A", 0, RES));
    let id = project.alloc_id();
    let mut fill = Fill::default();
    fill.sources.insert(Channel::BaseColor, Source::Uniform([1.0; 3]));
    let layer = project.insert_layer(0, Layer::new(id, "Fill", LayerKind::Fill(fill), &[Channel::BaseColor]), None, false).unwrap();
    Fixture { project, mesh, gbuf, layer }
}

/// Every covered texel and its UV island
fn covered(fixture: &Fixture) -> Vec<((u32, u32), usize)> {
    let islands = TrianglePartition::uv_islands(&fixture.mesh, 0);
    (0..RES * RES)
        .filter_map(|index| {
            let triangle = *fixture.gbuf.tri.get(index as usize)?;
            islands.index_of(triangle).map(|island| ((index % RES, index / RES), island))
        })
        .collect()
}

fn mask(fixture: &mut Fixture, random: UvRandomColorParameters, seed: u32, texels: &[(u32, u32)]) -> Vec<f32> {
    let generator = Generator { controls: Some(Box::new(KindControls::UvRandomColor(random))), seed, ..Generator::new(GeneratorKind::UvRandomColor) };
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
    mask_at(&inputs, fixture.layer, texels).unwrap()
}

/// Each island's values, by island
fn by_island(texels: &[((u32, u32), usize)], values: &[f32]) -> std::collections::BTreeMap<usize, Vec<f32>> {
    let mut islands = std::collections::BTreeMap::<usize, Vec<f32>>::new();
    for ((_, island), value) in texels.iter().zip(values) {
        islands.entry(*island).or_default().push(*value);
    }
    islands
}

#[test]
fn each_island_is_one_value_and_the_islands_differ() {
    let mut fixture = fixture();
    let texels = covered(&fixture);
    let at: Vec<(u32, u32)> = texels.iter().map(|(texel, _)| *texel).collect();

    let islands = by_island(&texels, &mask(&mut fixture, UvRandomColorParameters::default(), 0, &at));
    assert!(islands.len() > 1, "the cube has several UV islands");
    assert!(islands.values().all(|values| values.iter().all(|value| (value - values[0]).abs() < 1e-6)));
    let firsts: Vec<f32> = islands.values().map(|values| values[0]).collect();
    assert!(firsts.iter().any(|value| (value - firsts[0]).abs() > 0.01));
}

#[test]
fn the_seed_reshuffles_the_values() {
    let mut fixture = fixture();
    let at: Vec<(u32, u32)> = covered(&fixture).iter().map(|(texel, _)| *texel).collect();

    let first = mask(&mut fixture, UvRandomColorParameters::default(), 0, &at);
    let second = mask(&mut fixture, UvRandomColorParameters::default(), 2, &at);
    assert_ne!(first, second);
}

#[test]
fn a_one_color_gradient_colors_every_island_alike() {
    let mut fixture = fixture();
    let at: Vec<(u32, u32)> = covered(&fixture).iter().map(|(texel, _)| *texel).collect();

    let values = mask(&mut fixture, UvRandomColorParameters { custom_gradient: true, gradient: vec![[0.1, 0.2, 0.3]] }, 0, &at);
    assert!(values.iter().all(|value| (value - 0.2).abs() < 1e-5), "the mean of its channels");
}
