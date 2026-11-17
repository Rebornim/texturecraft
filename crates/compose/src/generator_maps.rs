//! The precomputed maps one generator reads: its curvature detail sizes and its micro details

use std::sync::Arc;

use texturecraft_core::{BakedMap, Generator, MicroDetails};

use crate::curvature_sizes::CurvatureSizes;
use crate::eval::Inputs;
use crate::micro_details::{LayoutMap, MicroCurvature, micro_occlusion};

/// What a generator reads beyond its texel: built once per map and settings, shared by every tile
#[derive(Clone, Default)]
pub(crate) struct GeneratorMaps {
    /// The curvature detail sizes it reads (of its micro curvature when it has some)
    pub sizes: Option<Arc<CurvatureSizes>>,
    /// The curvature its micro details add
    pub micro_curvature: Option<Arc<MicroCurvature>>,
    /// What its micro details multiply its occlusion by
    pub micro_occlusion: Option<Arc<LayoutMap>>,
}

impl GeneratorMaps {
    /// The maps `generator` reads, with the detail sizes `needed` marks (one flag per blurred size)
    pub fn of(inp: &Inputs, generator: &Generator, needed: &[bool]) -> GeneratorMaps {
        let micro = generator.micro_in_use();
        let micro_curvature = micro.filter(|_| MicroDetails::has_curvature(generator.kind)).and_then(|micro| MicroCurvature::of(inp, generator.kind, micro));
        let micro_occlusion = micro.filter(|_| MicroDetails::has_occlusion(generator.kind)).and_then(|micro| micro_occlusion(inp, micro));
        let sizes = match &micro_curvature {
            Some(micro) => CurvatureSizes::of_map(inp, &micro.curvature, needed),
            None => inp.baked.maps.get(&BakedMap::Curvature).and_then(|curvature| CurvatureSizes::of_map(inp, curvature, needed)),
        };
        GeneratorMaps { sizes, micro_curvature, micro_occlusion }
    }
}
