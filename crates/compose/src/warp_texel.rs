//! What a Warp filter reads for one texel: its source, then the moves its mode makes

use texturecraft_core::{SourceWarp, WarpBlending, WarpMode, WarpSource, WarpSourceMode, lum};

use crate::eval::{direction, texel_noise};

/// Bilinear sample at an absolute texel position into an accumulator (components, then weight), as [`crate::eval`]'s gather reads
pub(crate) type Sample<'a> = &'a dyn Fn(f32, f32, &mut [f32; 5]);

/// The filter's Custom Noise image input at an absolute texel position; None where the values it filters cover nothing
pub(crate) type Input<'a> = &'a dyn Fn(f32, f32) -> Option<f32>;

/// Warp `warp` at texel `(x, y)` (`res` texels across a UV tile, `comps` values a texel) into `acc`: where its source sends it, or those reads blended
pub(crate) fn warp_texel(warp: &SourceWarp, (x, y): (f32, f32), (res, comps): (u32, usize), acc: &mut [f32; 5], (sample, input): (Sample<'_>, Input<'_>)) {
    let source = Source { source: &warp.source, seed: warp.seed, res, comps, sample, input };
    let reach = warp.move_texels(res);
    if warp.mode == WarpMode::Warp {
        let probe = SourceWarp::probe_texels(res);
        let rise = [source.at(x + probe, y) - source.at(x - probe, y), source.at(x, y + probe) - source.at(x, y - probe)];
        let [dx, dy] = warp.slope_move(rise, res);
        sample(x + dx, y + dy, acc);
        return;
    }

    let directions: Vec<[f32; 2]> = warp.angles().into_iter().map(direction).collect();
    let moved = |(at_x, at_y): (f32, f32), [dx, dy]: [f32; 2]| {
        let along = SourceWarp::amount(source.at(at_x, at_y)) * reach;
        (at_x + dx * along, at_y + dy * along)
    };
    match (warp.mode, warp.blending) {
        (WarpMode::MultiDirectional, WarpBlending::Chain) => {
            let (end_x, end_y) = directions.iter().fold((x, y), |at, direction| moved(at, *direction));
            sample(end_x, end_y, acc);
        }
        (WarpMode::MultiDirectional, WarpBlending::Max | WarpBlending::Min) => {
            let larger = warp.blending == WarpBlending::Max;
            let mut picked: Option<[f32; 5]> = None;
            for direction in &directions {
                let (at_x, at_y) = moved((x, y), *direction);
                let mut read = [0.0f32; 5];
                sample(at_x, at_y, &mut read);
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
        // One direction, or the average of several: the reads add up in `acc`
        _ => {
            for direction in &directions {
                let (at_x, at_y) = moved((x, y), *direction);
                sample(at_x, at_y, acc);
            }
        }
    }
}

/// A warp's or slope blur's source at one texel's neighbourhood
pub(crate) struct Source<'a> {
    pub(crate) source: &'a WarpSource,
    /// The default noise's seed
    pub(crate) seed: u32,
    pub(crate) res: u32,
    pub(crate) comps: usize,
    pub(crate) sample: Sample<'a>,
    pub(crate) input: Input<'a>,
}

impl Source<'_> {
    /// The source at texel position `(x, y)` after its blur, Balance and Contrast
    pub(crate) fn at(&self, x: f32, y: f32) -> f32 {
        let half = 0.5 * self.source.blur_texels(self.res);
        let value = if half > 0.0 {
            let corners = [(-half, -half), (half, -half), (-half, half), (half, half)];
            let read: Vec<f32> = corners.iter().filter_map(|(dx, dy)| self.raw(x + dx, y + dy)).collect();
            if read.is_empty() { 0.5 } else { read.iter().sum::<f32>() / read.len() as f32 }
        } else {
            self.raw(x, y).unwrap_or(0.5)
        };
        self.source.shape(value)
    }

    /// The source at texel position `(x, y)` before it is shaped; None where the values it filters cover nothing
    fn raw(&self, x: f32, y: f32) -> Option<f32> {
        match self.source.mode {
            WarpSourceMode::DefaultNoise => Some(WarpSource::stretch_noise(texel_noise(x, y, [0.0; 2], self.res, self.source.noise_scale(), self.seed))),
            WarpSourceMode::PreviousInput => {
                let mut read = [0.0f32; 5];
                (self.sample)(x, y, &mut read);
                match (self.comps, read) {
                    // Content stays premultiplied by its own coverage
                    (4, [r, g, b, alpha, _]) if alpha > 1e-5 => Some(lum([r / alpha, g / alpha, b / alpha])),
                    (1, [value, .., weight]) if weight > 1e-6 => Some(value / weight),
                    _ => None,
                }
            }
            // Mid gray (its default) moves nothing
            WarpSourceMode::CustomNoise => (self.input)(x, y),
        }
    }
}
