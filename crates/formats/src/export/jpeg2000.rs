//! JPEG 2000, lossless (reversible 5/3 wavelet): a raw codestream (.j2k) or one in a JP2 file (.jp2)

use j2k::{
    J2kChannelAssociation, J2kChannelDefinition, J2kChannelType, J2kFileBoxMetadata, J2kFileColorSpec, J2kFileWrapOptions, J2kLosslessEncodeOptions,
    J2kLosslessSamples, encode_j2k_lossless, wrap_j2k_codestream,
};
use j2k_core::Colorspace;

/// Encode `samples` (8 or 16-bit L, LA, RGB or RGBA texels) of a `width`×`height` image; `jp2` wraps it in a JP2 file with `icc`
pub(super) fn write_jpeg2000(
    samples: &[u16],
    (width, height): (u32, u32),
    component_count: usize,
    depth: u8,
    jp2: bool,
    icc: Option<&[u8]>,
) -> Result<Vec<u8>, String> {
    let bytes: Vec<u8> =
        if depth == 16 { samples.iter().flat_map(|sample| sample.to_le_bytes()).collect() } else { samples.iter().map(|sample| *sample as u8).collect() };
    let components = u16::try_from(component_count).map_err(|_| format!("{component_count} components"))?;
    let input = J2kLosslessSamples { data: &bytes, width, height, components, bit_depth: depth, signed: false };
    let codestream = encode_j2k_lossless(input, &J2kLosslessEncodeOptions::default()).map_err(|error| error.to_string())?.codestream;
    if !jp2 {
        return Ok(codestream);
    }

    // The last component of LA and RGBA is opacity, for the whole image
    let color_count = if component_count >= 3 { 3 } else { 1 };
    let definitions: Vec<J2kChannelDefinition> = (0..components)
        .map(|channel_index| {
            if channel_index < color_count {
                J2kChannelDefinition {
                    channel_index,
                    channel_type: J2kChannelType::Color,
                    association: J2kChannelAssociation::Color { index: channel_index + 1 },
                }
            } else {
                J2kChannelDefinition { channel_index, channel_type: J2kChannelType::Opacity, association: J2kChannelAssociation::WholeImage }
            }
        })
        .collect();
    let mut options = J2kFileWrapOptions::jp2();
    options.color = match icc {
        Some(profile) if color_count == 3 => J2kFileColorSpec::IccProfile(profile),
        _ if color_count == 3 => J2kFileColorSpec::Enumerated(Colorspace::SRgb),
        _ => J2kFileColorSpec::Enumerated(Colorspace::Grayscale),
    };
    if component_count.is_multiple_of(2) {
        options.metadata = J2kFileBoxMetadata { palette: None, component_mappings: &[], channel_definitions: &definitions };
    }
    wrap_j2k_codestream(&codestream, options).map_err(|error| error.to_string())
}
