use serde::{Deserialize, Serialize};

/// How a color texture becomes one gray value
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GrayscaleConversion {
    /// Halfway between the brightest and the darkest channel
    Desaturation,
    /// The channels weighted by how bright the eye sees them
    Luma,
    /// The mean of the three channels
    #[default]
    Average,
    Max,
    Min,
    Red,
    Green,
    Blue,
}

impl GrayscaleConversion {
    pub const ALL: [GrayscaleConversion; 8] = [
        GrayscaleConversion::Desaturation,
        GrayscaleConversion::Luma,
        GrayscaleConversion::Average,
        GrayscaleConversion::Max,
        GrayscaleConversion::Min,
        GrayscaleConversion::Red,
        GrayscaleConversion::Green,
        GrayscaleConversion::Blue,
    ];

    /// The name the panel shows
    pub fn label(self) -> &'static str {
        match self {
            GrayscaleConversion::Desaturation => "Desaturation",
            GrayscaleConversion::Luma => "Luma",
            GrayscaleConversion::Average => "Average",
            GrayscaleConversion::Max => "Max",
            GrayscaleConversion::Min => "Min",
            GrayscaleConversion::Red => "Red channel",
            GrayscaleConversion::Green => "Green channel",
            GrayscaleConversion::Blue => "Blue channel",
        }
    }

    /// The name commands and saved projects use
    pub fn id(self) -> &'static str {
        match self {
            GrayscaleConversion::Desaturation => "desaturation",
            GrayscaleConversion::Luma => "luma",
            GrayscaleConversion::Average => "average",
            GrayscaleConversion::Max => "max",
            GrayscaleConversion::Min => "min",
            GrayscaleConversion::Red => "red",
            GrayscaleConversion::Green => "green",
            GrayscaleConversion::Blue => "blue",
        }
    }

    /// The conversion named by its id or its label, in any case
    pub fn parse(name: &str) -> Option<GrayscaleConversion> {
        GrayscaleConversion::ALL.into_iter().find(|mode| mode.id().eq_ignore_ascii_case(name) || mode.label().eq_ignore_ascii_case(name))
    }

    /// The gray of `rgb`
    pub fn gray(self, [red, green, blue]: [f32; 3]) -> f32 {
        // ITU-R BT.601 weights, the reference's Luma
        const LUMA: [f32; 3] = [0.299, 0.587, 0.114];
        match self {
            GrayscaleConversion::Desaturation => (red.max(green).max(blue) + red.min(green).min(blue)) * 0.5,
            GrayscaleConversion::Luma => red * LUMA[0] + green * LUMA[1] + blue * LUMA[2],
            GrayscaleConversion::Average => (red + green + blue) / 3.0,
            GrayscaleConversion::Max => red.max(green).max(blue),
            GrayscaleConversion::Min => red.min(green).min(blue),
            GrayscaleConversion::Red => red,
            GrayscaleConversion::Green => green,
            GrayscaleConversion::Blue => blue,
        }
    }
}
