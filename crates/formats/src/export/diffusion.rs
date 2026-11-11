//! Dilation + diffusion padding: the texels beyond the dilation take a blur that spreads out from the islands (pull-push)

mod level;

use level::Level;

/// Most components a texel has (RGBA)
const MAX_COMPONENTS: usize = 4;

/// Fill the texels of a `width` × `height` image of `component_count` components where `valid` is false; valid texels keep their values
pub(super) fn diffuse(data: &mut [f32], valid: &[bool], (width, height): (u32, u32), component_count: usize) {
    let (width, height) = (width as usize, height as usize);
    if component_count == 0
        || component_count > MAX_COMPONENTS
        || data.len() != width * height * component_count
        || valid.len() != width * height
        || valid.iter().all(|is_valid| *is_valid)
        || !valid.iter().any(|is_valid| *is_valid)
    {
        return;
    }

    // Pull: average the known texels into ever coarser levels
    let mut levels = vec![Level { width, height, color: data.to_vec(), weight: valid.iter().map(|is_valid| f32::from(u8::from(*is_valid))).collect() }];
    while let Some(fine) = levels.last()
        && (fine.width > 1 || fine.height > 1)
    {
        let coarse = pull(fine, component_count);
        levels.push(coarse);
    }

    // Push: from the coarsest level down, what a texel doesn't know comes from the level above it
    for index in (0..levels.len().saturating_sub(1)).rev() {
        let (fine_levels, coarse_levels) = levels.split_at_mut(index + 1);
        if let (Some(fine), Some(coarse)) = (fine_levels.last_mut(), coarse_levels.first()) {
            push(fine, coarse, component_count);
        }
    }

    if let Some(finest) = levels.first() {
        for (texel, known) in valid.iter().enumerate() {
            if !known {
                let range = texel * component_count..texel * component_count + component_count;
                if let (Some(target), Some(source)) = (data.get_mut(range.clone()), finest.color.get(range)) {
                    target.copy_from_slice(source);
                }
            }
        }
    }
}

/// The next coarser level: each texel is the weighted average of up to 2×2 finer texels
fn pull(fine: &Level, component_count: usize) -> Level {
    let (width, height) = (fine.width.div_ceil(2), fine.height.div_ceil(2));
    let mut color = vec![0.0; width * height * component_count];
    let mut weight = vec![0.0; width * height];
    for y in 0..height {
        for x in 0..width {
            let mut total = 0.0;
            let mut component_sums = [0.0f32; MAX_COMPONENTS];
            for (fine_x, fine_y) in [(2 * x, 2 * y), (2 * x + 1, 2 * y), (2 * x, 2 * y + 1), (2 * x + 1, 2 * y + 1)] {
                if fine_x >= fine.width || fine_y >= fine.height {
                    continue;
                }

                let texel = fine_y * fine.width + fine_x;
                let texel_weight = fine.weight.get(texel).copied().unwrap_or(0.0);
                total += texel_weight;
                for (component, component_sum) in component_sums.iter_mut().take(component_count).enumerate() {
                    *component_sum += texel_weight * fine.color.get(texel * component_count + component).copied().unwrap_or(0.0);
                }
            }
            if total > 0.0 {
                let texel = y * width + x;
                for (component, component_sum) in component_sums.into_iter().take(component_count).enumerate() {
                    if let Some(value) = color.get_mut(texel * component_count + component) {
                        *value = component_sum / total;
                    }
                }
                if let Some(texel_weight) = weight.get_mut(texel) {
                    *texel_weight = total.min(1.0);
                }
            }
        }
    }

    Level { width, height, color, weight }
}

/// Blend each fine texel toward the coarse level (sampled bilinearly) by how little of it is known
fn push(fine: &mut Level, coarse: &Level, component_count: usize) {
    for y in 0..fine.height {
        for x in 0..fine.width {
            let texel = y * fine.width + x;
            let known = fine.weight.get(texel).copied().unwrap_or(0.0);
            if known >= 1.0 {
                continue;
            }
            let from_coarse = bilinear(coarse, (x as f32 + 0.5) / 2.0 - 0.5, (y as f32 + 0.5) / 2.0 - 0.5, component_count);
            for (component, coarse_value) in from_coarse.into_iter().take(component_count).enumerate() {
                if let Some(value) = fine.color.get_mut(texel * component_count + component) {
                    *value = known * *value + (1.0 - known) * coarse_value;
                }
            }
            if let Some(texel_weight) = fine.weight.get_mut(texel) {
                *texel_weight = 1.0;
            }
        }
    }
}

/// `level`'s colour at (x, y) in texel units, clamped to its edges; the first `component_count` components are set
fn bilinear(level: &Level, x: f32, y: f32, component_count: usize) -> [f32; MAX_COMPONENTS] {
    let clamp = |coordinate: f32, size: usize| coordinate.clamp(0.0, size.saturating_sub(1) as f32);
    let (x, y) = (clamp(x, level.width), clamp(y, level.height));
    let (x0, y0) = (x.floor() as usize, y.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(level.width.saturating_sub(1)), (y0 + 1).min(level.height.saturating_sub(1)));
    let (fraction_x, fraction_y) = (x - x0 as f32, y - y0 as f32);
    let at = |x: usize, y: usize, component: usize| level.color.get((y * level.width + x) * component_count + component).copied().unwrap_or(0.0);

    let mut color = [0.0; MAX_COMPONENTS];
    for (component, value) in color.iter_mut().take(component_count).enumerate() {
        let top = at(x0, y0, component) * (1.0 - fraction_x) + at(x1, y0, component) * fraction_x;
        let bottom = at(x0, y1, component) * (1.0 - fraction_x) + at(x1, y1, component) * fraction_x;
        *value = top * (1.0 - fraction_y) + bottom * fraction_y;
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
