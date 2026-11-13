//! What a Blur Slope filter reads for one texel: its source's slope, then the values downhill of it

use texturecraft_core::{SlopeBlending, SourceSlopeBlur, SourceWarp};

use crate::eval::LINE_SAMPLES;
use crate::warp_texel::{Input, Sample, Source};

/// Blur Slope `slope` at texel `(x, y)` (`res` texels across a UV tile, `comps` values a texel) into `acc`: the reads downhill of it, blended
pub(crate) fn slope_texel(
    slope: &SourceSlopeBlur,
    (x, y): (f32, f32),
    (res, comps): (u32, usize),
    acc: &mut [f32; 5],
    (sample, input): (Sample<'_>, Input<'_>),
) {
    let source = Source { source: &slope.source, seed: slope.seed, res, comps, sample, input };
    let probe = SourceWarp::probe_texels(res);
    let downhill = [source.at(x - probe, y) - source.at(x + probe, y), source.at(x, y - probe) - source.at(x, y + probe)];
    let steepness = downhill[0].hypot(downhill[1]);
    // A flat spot keeps its value
    if steepness.is_nan() || steepness <= 1e-6 {
        sample(x, y, acc);
        return;
    }
    let [dx, dy] = downhill.map(|rise| rise / steepness);
    let reach = slope.move_texels(res);
    let reads = slope.samples((reach.ceil() as u32).clamp(1, LINE_SAMPLES.unsigned_abs()));
    let step = reach / reads as f32;
    let along = (0..=reads).map(|read| read as f32 * step);
    if slope.blending == SlopeBlending::Blur {
        for t in along {
            sample(x + dx * t, y + dy * t, acc);
        }
        return;
    }
    let larger = slope.blending == SlopeBlending::Max;
    let mut picked: Option<[f32; 5]> = None;
    for t in along {
        let mut read = [0.0f32; 5];
        sample(x + dx * t, y + dy * t, &mut read);
        if read[4] <= 1e-6 {
            continue;
        }
        let read = read.map(|component| component / read[4]);
        picked = Some(match picked {
            Some(so_far) => std::array::from_fn(|k| if larger { so_far[k].max(read[k]) } else { so_far[k].min(read[k]) }),
            None => read,
        });
    }
    if let Some(picked) = picked {
        acc[..comps.min(4)].copy_from_slice(&picked[..comps.min(4)]);
        acc[4] = 1.0;
    }
}
