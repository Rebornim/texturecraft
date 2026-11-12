//! The colour picker a colour bar opens: the space its values are shown in, the model its sliders edit, its memory between frames and what it hands back

mod color_model;
mod color_pick;
mod color_space;
mod picker_state;

pub(crate) use color_model::ColorModel;
pub(crate) use color_pick::ColorPick;
pub(crate) use color_space::ColorSpace;
pub(crate) use picker_state::PickerState;

/// `rgb` (0..1 each) as six hex digits, as the hex field shows it
pub(crate) fn hex(rgb: [f32; 3]) -> String {
    rgb.iter().map(|c| format!("{:02X}", (c.clamp(0.0, 1.0) * 255.0).round() as u8)).collect()
}

/// A typed hex code (`#` optional, 6 or 3 digits) as 0..1 components; None when it isn't one
pub(crate) fn parse_hex(text: &str) -> Option<[f32; 3]> {
    let digits = text.trim().trim_start_matches('#');
    let full: String = match digits.len() {
        3 => digits.chars().flat_map(|c| [c, c]).collect(),
        6 => digits.to_owned(),
        _ => return None,
    };
    let channel = |at: usize| full.get(at..at + 2).and_then(|pair| u8::from_str_radix(pair, 16).ok()).map(|byte| f32::from(byte) / 255.0);
    Some([channel(0)?, channel(2)?, channel(4)?])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_codes_read_back_what_they_show() {
        assert_eq!(hex([1.0, 0.5, 0.0]), "FF8000");
        assert_eq!(parse_hex("#ff8000"), Some([1.0, 128.0 / 255.0, 0.0]));
        assert_eq!(parse_hex("f80"), parse_hex("FF8800"));
        assert_eq!(parse_hex("12345"), None);
        assert_eq!(parse_hex("GG0000"), None);
        assert_eq!(parse_hex("€€€"), None, "multi-byte text is not a code");
    }

    #[test]
    fn each_space_shows_and_reads_back_the_same_linear_colour() {
        let linear = [0.2, 0.5, 0.9];
        for space in ColorSpace::ALL {
            let back = space.linear(space.shown(linear));
            assert!(back.iter().zip(linear).all(|(a, b)| (a - b).abs() < 1e-4), "{space:?}: {back:?}");
        }
        assert!(ColorSpace::Srgb.shown(linear)[0] > linear[0], "sRGB shows dark values brighter");
        assert_eq!(ColorSpace::Linear.shown(linear), linear);
    }
}
