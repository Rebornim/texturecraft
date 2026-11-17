//! The UV Checker generator on a plane filling its UV tile: a checker with grid lines, its border along the tile's edge

use texturecraft_compose::{BakedMaps, Inputs, PixelStore, mask_at};
use texturecraft_core::{
    Channel, Effect, EffectKind, Fill, Generator, GeneratorKind, KindControls, Layer, LayerId, LayerKind, Mask, MaskBackground, Project, Source, TextureSet,
    UvCheckerParameters,
};
use texturecraft_geom::{Mesh, UvGBuffer, primitives};

const RES: u32 = 256;

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

/// The middle row, left to right
fn row(fixture: &mut Fixture, checker: UvCheckerParameters) -> Vec<f32> {
    let generator = Generator { controls: Some(Box::new(KindControls::UvChecker(checker))), ..Generator::new(GeneratorKind::UvChecker) };
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
    let texels: Vec<(u32, u32)> = (0..RES).map(|x| (x, RES / 2 + 3)).collect();
    mask_at(&inputs, fixture.layer, &texels).unwrap()
}

/// The distinct values of `values`, to 0.01
fn tones(values: &[f32]) -> std::collections::BTreeSet<u32> {
    values.iter().map(|value| (value * 100.0).round() as u32).collect()
}

#[test]
fn without_grid_lines_the_checker_has_two_tones_and_a_border() {
    let mut fixture = fixture();
    let values = row(&mut fixture, UvCheckerParameters { grid_opacity: 0.0, grid_tile: 1, ..UvCheckerParameters::default() });

    assert_eq!(tones(&values[8..RES as usize - 8]).len(), 2, "two checker tones inside: {:?}", tones(&values));
    assert!((values[0] - 1.0 / 3.0).abs() < 0.01, "the red border reads a third: {}", values[0]);
}

#[test]
fn grid_lines_only_darken() {
    let mut fixture = fixture();
    let plain = row(&mut fixture, UvCheckerParameters { grid_opacity: 0.0, ..UvCheckerParameters::default() });
    let lined = row(&mut fixture, UvCheckerParameters::default());

    assert!(lined.iter().zip(&plain).all(|(lined, plain)| lined <= plain));
    assert!(lined.iter().zip(&plain).any(|(lined, plain)| lined < plain));
}
