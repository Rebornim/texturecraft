//! What the generator and pattern panels share: the attribute rows' key width, note size, the resolution factor's label and a random seed

use texturecraft_engine::core::OutputResolution;

use crate::i18n::tr;

/// Width of the names in the Attributes section, so their values line up
pub(crate) const ATTRIBUTE_KEY_WIDTH: f32 = 80.0;
/// Font size of the notes under a control
pub(crate) const NOTE_FONT_SIZE: f32 = 10.5;

/// Random seeds stay below this, short enough to read and type
const SEED_RANGE: u32 = 10_000;
/// Knuth's multiplicative hash constant: spreads the clock's nanoseconds over the seed range
const SEED_MIX: u32 = 2_654_435_761;
/// The most powers of two a resolution factor scales by, either way
const LARGEST_FACTOR_STEPS: u8 = OutputResolution::FACTORS.end().unsigned_abs();

/// "Same as context", or the fraction or multiple a resolution factor stands for
pub(crate) fn factor_label(factor: i8) -> String {
    match factor {
        0 => tr("Same as context").to_string(),
        negative if negative < 0 => format!("1/{}", 1u32 << negative.unsigned_abs().min(LARGEST_FACTOR_STEPS)),
        positive => format!("×{}", 1u32 << positive.unsigned_abs().min(LARGEST_FACTOR_STEPS)),
    }
}

/// A seed different from `current`, from the clock
pub(crate) fn random_seed(current: u32) -> u32 {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |elapsed| elapsed.subsec_nanos());
    let mixed = nanos.wrapping_mul(SEED_MIX) % SEED_RANGE;
    if mixed == current { (mixed + 1) % SEED_RANGE } else { mixed }
}
