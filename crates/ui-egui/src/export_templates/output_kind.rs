use texturecraft_engine::formats::{Dest, FileFormat, MapOptions, MapSpec, SourceRef};

use crate::i18n::n_;

/// What a Create button adds: one grey channel, one colour, three separate channels, either with alpha
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum OutputKind {
    Gray,
    Rgb,
    SeparateRgb,
    RgbAlpha,
    SeparateRgbAlpha,
}

impl OutputKind {
    pub(super) const ALL: [OutputKind; 5] = [OutputKind::Gray, OutputKind::Rgb, OutputKind::SeparateRgb, OutputKind::RgbAlpha, OutputKind::SeparateRgbAlpha];

    pub(super) fn label(self) -> &'static str {
        match self {
            OutputKind::Gray => n_("Gray"),
            OutputKind::Rgb => "RGB",
            OutputKind::SeparateRgb => "R+G+B",
            OutputKind::RgbAlpha => "RGB+A",
            OutputKind::SeparateRgbAlpha => "R+G+B+A",
        }
    }

    /// A new output of this kind, every channel empty (black, alpha white)
    pub(super) fn new_map(self) -> MapSpec {
        let rgb = [(Dest::R, SourceRef::Constant(0.0)), (Dest::G, SourceRef::Constant(0.0)), (Dest::B, SourceRef::Constant(0.0))];
        let alpha = (Dest::A, SourceRef::Constant(1.0));
        let channels = match self {
            OutputKind::Gray => vec![(Dest::L, SourceRef::Constant(0.0))],
            OutputKind::Rgb | OutputKind::SeparateRgb => rgb.to_vec(),
            OutputKind::RgbAlpha | OutputKind::SeparateRgbAlpha => rgb.into_iter().chain([alpha]).collect(),
        };
        let separate_rgb = matches!(self, OutputKind::SeparateRgb | OutputKind::SeparateRgbAlpha);

        MapSpec {
            file: "$mesh_$textureSet_Output".into(),
            format: FileFormat::Png,
            bit_depth: 8,
            channels,
            options: MapOptions { separate_rgb, ..Default::default() },
        }
    }
}
