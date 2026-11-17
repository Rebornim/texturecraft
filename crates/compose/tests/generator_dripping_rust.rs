//! The Dripping Rust generator on a sphere with an occluded band: rust grows in the occlusion, spreads from none to everywhere, and drips run down from it

use std::sync::Arc;

use texturecraft_compose::{BakedMaps, Inputs, PixelStore, mask_at};
use texturecraft_core::{
    BakedMap, Channel, DrippingRustParameters, Effect, EffectKind, Fill, Generator, GeneratorKind, KindControls, Layer, LayerId, LayerKind, Mask,
    MaskBackground, Project, Source, TextureSet,
};
use texturecraft_geom::{Mesh, UvGBuffer, primitives};

const RES: u32 = 128;

/// The occluded band's heights on the unit sphere, and its occlusion
const BAND: std::ops::Range<f32> = 0.2..0.5;
const BAND_OCCLUSION: f32 = 0.2;

struct Fixture {
    project: Project,
    mesh: Mesh,
    gbuf: UvGBuffer,
    layer: LayerId,
    baked: BakedMaps,
}

/// A unit sphere with a white fill layer, occluded only in [`BAND`]
fn fixture() -> Fixture {
    let mesh = primitives::sphere(1.0, 48, 24);
    let gbuf = UvGBuffer::new(&mesh, 0, RES, RES);
    let mut project = Project::new("t", "builtin:sphere");
    project.texture_sets.push(TextureSet::new("A", 0, RES));
    let id = project.alloc_id();
    let mut fill = Fill::default();
    fill.sources.insert(Channel::BaseColor, Source::Uniform([1.0; 3]));
    let layer = project.insert_layer(0, Layer::new(id, "Fill", LayerKind::Fill(fill), &[Channel::BaseColor]), None, false).unwrap();
    let occlusion: Vec<f32> = (0..RES * RES)
        .map(|texel| gbuf.surface(&mesh, texel as usize).map_or(1.0, |surface| if BAND.contains(&surface.pos.y) { BAND_OCCLUSION } else { 1.0 }))
        .collect();
    let baked = BakedMaps { resolution: RES, maps: [(BakedMap::AmbientOcclusion, Arc::new(occlusion))].into() };
    Fixture { project, mesh, gbuf, layer, baked }
}

/// Every covered texel and its height
fn covered(fixture: &Fixture) -> Vec<((u32, u32), f32)> {
    (0..RES * RES).filter_map(|index| fixture.gbuf.surface(&fixture.mesh, index as usize).map(|surface| ((index % RES, index / RES), surface.pos.y))).collect()
}

fn mask(fixture: &mut Fixture, rust: DrippingRustParameters, texels: &[(u32, u32)]) -> Vec<f32> {
    let generator = Generator { controls: Some(Box::new(KindControls::DrippingRust(rust))), ..Generator::new(GeneratorKind::DrippingRust) };
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

/// The mean of `values` over the texels whose height lies in `heights`
fn mean_within(texels: &[((u32, u32), f32)], values: &[f32], heights: std::ops::Range<f32>) -> f32 {
    let picked: Vec<f32> = texels.iter().zip(values).filter(|((_, height), _)| heights.contains(height)).map(|(_, value)| *value).collect();
    picked.iter().sum::<f32>() / picked.len().max(1) as f32
}

#[test]
fn spreading_runs_from_none_to_everywhere() {
    let mut fixture = fixture();
    let at: Vec<(u32, u32)> = covered(&fixture).iter().map(|(texel, _)| *texel).collect();

    let none = mask(&mut fixture, DrippingRustParameters { spreading: 0.0, ..DrippingRustParameters::default() }, &at);
    let all = mask(&mut fixture, DrippingRustParameters { spreading: 1.0, ..DrippingRustParameters::default() }, &at);
    assert!(none.iter().all(|value| *value == 0.0));
    assert!(all.iter().all(|value| *value == 1.0));
}

#[test]
fn the_occluded_band_rusts_most() {
    let mut fixture = fixture();
    let texels = covered(&fixture);
    let at: Vec<(u32, u32)> = texels.iter().map(|(texel, _)| *texel).collect();

    let values = mask(&mut fixture, DrippingRustParameters { drips_intensity: 0.0, ..DrippingRustParameters::default() }, &at);
    assert!(mean_within(&texels, &values, BAND) > mean_within(&texels, &values, -1.0..-0.2) + 0.3);
}

#[test]
fn drips_only_add_rust_and_run_down_from_the_band() {
    let mut fixture = fixture();
    let texels = covered(&fixture);
    let at: Vec<(u32, u32)> = texels.iter().map(|(texel, _)| *texel).collect();
    let rust = DrippingRustParameters { contrast: 0.0, drips_smoothness: 0.0, ..DrippingRustParameters::default() };

    let dry = mask(&mut fixture, DrippingRustParameters { drips_intensity: 0.0, ..rust }, &at);
    let dripping = mask(&mut fixture, DrippingRustParameters { drips_intensity: 1.0, ..rust }, &at);
    assert!(dripping.iter().zip(&dry).all(|(wet, dry)| wet >= dry), "a drip takes the most rust above, never less than its own");
    let gain = |heights: std::ops::Range<f32>| mean_within(&texels, &dripping, heights.clone()) - mean_within(&texels, &dry, heights);
    assert!(gain(0.05..0.2) > gain(0.5..0.65) + 0.05, "below the band {} above it {}", gain(0.05..0.2), gain(0.5..0.65));
}
