//! The precomputed maps one generator reads: its curvature detail sizes and its micro details

use std::sync::Arc;

use texturecraft_core::{BakedMap, Generator, KindControls, MicroDetails};

use crate::curvature_sizes::CurvatureSizes;
use crate::dripping_rust::rust_field;
use crate::eval::Inputs;
use crate::micro_details::{LayoutMap, MicroCurvature, micro_occlusion};
use crate::uv_border_distance::border_gradient;

/// What a generator reads beyond its texel: built once per map and settings, shared by every tile
#[derive(Clone, Default)]
pub(crate) struct GeneratorMaps {
    /// The curvature detail sizes it reads (of its micro curvature when it has some)
    pub sizes: Option<Arc<CurvatureSizes>>,
    /// The curvature its micro details add
    pub micro_curvature: Option<Arc<MicroCurvature>>,
    /// What its micro details multiply its occlusion by
    pub micro_occlusion: Option<Arc<LayoutMap>>,
    /// The UV Border Distance gradient at every working texel
    pub border_gradient: Option<Arc<Vec<f32>>>,
    /// The Dripping Rust field at every working texel
    pub rust_field: Option<Arc<Vec<f32>>>,
    /// The controls of a kind with only its own controls, brought into range
    pub controls: Option<Arc<KindControls>>,
}

impl GeneratorMaps {
    /// The maps `generator` reads, with the detail sizes `needed` marks (one flag per blurred size)
    pub fn of(inp: &Inputs, generator: &Generator, needed: &[bool]) -> GeneratorMaps {
        let micro = generator.micro_in_use().map(|micro| micro.clone().clamped(generator.kind));
        let micro_curvature =
            micro.as_ref().filter(|_| MicroDetails::has_curvature(generator.kind)).and_then(|micro| MicroCurvature::of(inp, generator.kind, micro));
        let micro_occlusion = micro.as_ref().filter(|_| MicroDetails::has_occlusion(generator.kind)).and_then(|micro| micro_occlusion(inp, micro));
        let sizes = match &micro_curvature {
            Some(micro) => CurvatureSizes::of_map(inp, &micro.curvature, needed),
            None => inp.baked.maps.get(&BakedMap::Curvature).and_then(|curvature| CurvatureSizes::of_map(inp, curvature, needed)),
        };
        let controls = generator.kind_controls();
        let border_gradient = match &controls {
            Some(KindControls::BorderDistance(border)) => Some(border_gradient(inp, border)),
            _ => None,
        };
        let rust_field = match &controls {
            Some(KindControls::DrippingRust(rust)) => Some(rust_field(inp, rust, generator.seed)),
            _ => None,
        };
        GeneratorMaps { sizes, micro_curvature, micro_occlusion, border_gradient, rust_field, controls: controls.map(Arc::new) }
    }
}
