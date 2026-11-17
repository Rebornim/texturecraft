use serde::{Deserialize, Serialize};

use super::{BorderDistanceParameters, DistanceParameters, GeneratorKind, GrayscaleConversionParameters, LinearGradientParameters};

/// The controls of a kind that has only the reference's own controls, no earlier model: one variant per kind
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum KindControls {
    Distance(DistanceParameters),
    LinearGradient(LinearGradientParameters),
    BorderDistance(BorderDistanceParameters),
    Grayscale(GrayscaleConversionParameters),
}

impl KindControls {
    /// The reference app's defaults for `kind`, or None for kinds with an earlier model or section, wear or light controls
    pub fn defaults(kind: GeneratorKind) -> Option<KindControls> {
        match kind {
            GeneratorKind::ThreeDDistance => Some(KindControls::Distance(DistanceParameters::default())),
            GeneratorKind::ThreeDLinearGradient => Some(KindControls::LinearGradient(LinearGradientParameters::default())),
            GeneratorKind::UvBorderDistance => Some(KindControls::BorderDistance(BorderDistanceParameters::default())),
            GeneratorKind::GrayscaleConversion => Some(KindControls::Grayscale(GrayscaleConversionParameters::default())),
            GeneratorKind::EdgeWear
            | GeneratorKind::Dirt
            | GeneratorKind::ThinAreas
            | GeneratorKind::Curvature
            | GeneratorKind::AmbientOcclusion
            | GeneratorKind::Position
            | GeneratorKind::Direction
            | GeneratorKind::FiberGlassEdgeWear
            | GeneratorKind::WorldSpaceNormals => None,
        }
    }

    /// The kind these controls belong to
    pub fn kind(&self) -> GeneratorKind {
        match self {
            KindControls::Distance(_) => GeneratorKind::ThreeDDistance,
            KindControls::LinearGradient(_) => GeneratorKind::ThreeDLinearGradient,
            KindControls::BorderDistance(_) => GeneratorKind::UvBorderDistance,
            KindControls::Grayscale(_) => GeneratorKind::GrayscaleConversion,
        }
    }
}
