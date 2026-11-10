//! Dilation + diffusion padding: the texels beyond the dilation take a blur that spreads out from the islands (pull-push)

/// Most components a texel has (RGBA)
const MAX_COMPONENTS: usize = 4;

/// One level of the pull pyramid: per texel, `n` colour components and how much of it is known (0 to 1)
struct Level {
    width: usize,
    height: usize,
    color: Vec<f32>,
    weight: Vec<f32>,
}

/// Fill the texels of a `width` × `height` image of `n` components where `valid` is false; valid texels keep their values
pub(super) fn diffuse(data: &mut [f32], valid: &[bool], (width, height): (u32, u32), n: usize) {
    let (width, height) = (width as usize, height as usize);
    if n == 0
        || n > MAX_COMPONENTS
        || data.len() != width * height * n
        || valid.len() != width * height
        || valid.iter().all(|v| *v)
        || !valid.iter().any(|v| *v)
    {
        return;
    }

    // Pull: average the known texels into ever coarser levels
    let mut levels = vec![Level { width, height, color: data.to_vec(), weight: valid.iter().map(|v| f32::from(u8::from(*v))).collect() }];
    while let Some(fine) = levels.last()
        && (fine.width > 1 || fine.height > 1)
    {
        let coarse = pull(fine, n);
        levels.push(coarse);
    }

    // Push: from the coarsest level down, what a texel doesn't know comes from the level above it
    for index in (0..levels.len().saturating_sub(1)).rev() {
        let (fine_levels, coarse_levels) = levels.split_at_mut(index + 1);
        if let (Some(fine), Some(coarse)) = (fine_levels.last_mut(), coarse_levels.first()) {
            push(fine, coarse, n);
        }
    }

    if let Some(finest) = levels.first() {
        for (texel, known) in valid.iter().enumerate() {
            if !known {
                let range = texel * n..texel * n + n;
                if let (Some(target), Some(source)) = (data.get_mut(range.clone()), finest.color.get(range)) {
                    target.copy_from_slice(source);
                }
            }
        }
    }
}

/// The next coarser level: each texel is the weighted average of up to 2×2 finer texels
fn pull(fine: &Level, n: usize) -> Level {
    let (width, height) = (fine.width.div_ceil(2), fine.height.div_ceil(2));
    let mut color = vec![0.0; width * height * n];
    let mut weight = vec![0.0; width * height];
    for y in 0..height {
        for x in 0..width {
            let mut total = 0.0;
            let mut sum = [0.0f32; MAX_COMPONENTS];
            for (fx, fy) in [(2 * x, 2 * y), (2 * x + 1, 2 * y), (2 * x, 2 * y + 1), (2 * x + 1, 2 * y + 1)] {
                if fx >= fine.width || fy >= fine.height {
                    continue;
                }
                let texel = fy * fine.width + fx;
                let w = fine.weight.get(texel).copied().unwrap_or(0.0);
                total += w;
                for (k, s) in sum.iter_mut().take(n).enumerate() {
                    *s += w * fine.color.get(texel * n + k).copied().unwrap_or(0.0);
                }
            }
            if total > 0.0 {
                let texel = y * width + x;
                for (k, s) in sum.into_iter().take(n).enumerate() {
                    if let Some(c) = color.get_mut(texel * n + k) {
                        *c = s / total;
                    }
                }
                if let Some(w) = weight.get_mut(texel) {
                    *w = total.min(1.0);
                }
            }
        }
    }

    Level { width, height, color, weight }
}

/// Blend each fine texel toward the coarse level (sampled bilinearly) by how little of it is known
fn push(fine: &mut Level, coarse: &Level, n: usize) {
    for y in 0..fine.height {
        for x in 0..fine.width {
            let texel = y * fine.width + x;
            let known = fine.weight.get(texel).copied().unwrap_or(0.0);
            if known >= 1.0 {
                continue;
            }
            let from_coarse = bilinear(coarse, (x as f32 + 0.5) / 2.0 - 0.5, (y as f32 + 0.5) / 2.0 - 0.5, n);
            for (k, c) in from_coarse.into_iter().take(n).enumerate() {
                if let Some(value) = fine.color.get_mut(texel * n + k) {
                    *value = known * *value + (1.0 - known) * c;
                }
            }
            if let Some(w) = fine.weight.get_mut(texel) {
                *w = 1.0;
            }
        }
    }
}

/// `level`'s colour at (x, y) in texel units, clamped to its edges; the first `n` components are set
fn bilinear(level: &Level, x: f32, y: f32, n: usize) -> [f32; MAX_COMPONENTS] {
    let clamp = |v: f32, size: usize| v.clamp(0.0, size.saturating_sub(1) as f32);
    let (x, y) = (clamp(x, level.width), clamp(y, level.height));
    let (x0, y0) = (x.floor() as usize, y.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(level.width.saturating_sub(1)), (y0 + 1).min(level.height.saturating_sub(1)));
    let (tx, ty) = (x - x0 as f32, y - y0 as f32);
    let at = |x: usize, y: usize, k: usize| level.color.get((y * level.width + x) * n + k).copied().unwrap_or(0.0);

    let mut color = [0.0; MAX_COMPONENTS];
    for (k, c) in color.iter_mut().take(n).enumerate() {
        let top = at(x0, y0, k) * (1.0 - tx) + at(x1, y0, k) * tx;
        let bottom = at(x0, y1, k) * (1.0 - tx) + at(x1, y1, k) * tx;
        *c = top * (1.0 - ty) + bottom * ty;
    }

    color
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_texels_stay_and_gaps_blend_from_both_sides() {
        let width = 16;
        let mut data: Vec<f32> = (0..width)
            .map(|x| {
                if x < 4 {
                    0.0
                } else if x >= 12 {
                    1.0
                } else {
                    -1.0
                }
            })
            .collect();
        let valid: Vec<bool> = (0..width).map(|x| !(4..12).contains(&x)).collect();
        diffuse(&mut data, &valid, (width as u32, 1), 1);

        assert!(data[..4].iter().all(|v| *v == 0.0) && data[12..].iter().all(|v| *v == 1.0), "{data:?}");
        let gap = &data[4..12];
        assert!(gap.windows(2).all(|pair| pair[0] <= pair[1]), "the gap ramps from one side to the other: {gap:?}");
        assert!(gap.iter().all(|v| (0.0..=1.0).contains(v)));
    }

    #[test]
    fn nothing_to_fill_or_nothing_known_leaves_the_image() {
        let mut data = vec![0.5; 4];
        diffuse(&mut data, &[false; 4], (2, 2), 1);
        assert_eq!(data, [0.5; 4]);
        diffuse(&mut data, &[true; 3], (2, 2), 1);
        assert_eq!(data, [0.5; 4], "a mask of the wrong size is ignored");
    }
}
