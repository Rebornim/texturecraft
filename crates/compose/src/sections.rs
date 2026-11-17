//! The section controls of the Curvature, Ambient Occlusion, Position and World Space Normals generators

use texturecraft_core::{AxisBlend, AxisLayer, Generator, GeneratorGlobal, GeneratorKind, SectionParameters};

/// Gaussian sigma of the section blur per unit of its slider, in texels at [`GeneratorGlobal::BLUR_RESOLUTION`]
const BLUR_SIGMA_PER_UNIT: f32 = 4.5;

/// Base of the section balance's power curve: balance 0 raises the signal to 1/25, balance 1 to 25
const BALANCE_POWER_BASE: f32 = 25.0;

/// How far above the middle a signal must be to turn white at full contrast
const FULL_CONTRAST_STEP_OFFSET: f32 = 4.6e-5;

/// The order the axes combine in, as indices into [`SectionParameters::axes`]: top to bottom is the base, front to back goes last
const AXIS_ORDER: [usize; 3] = [SectionParameters::TOP_TO_BOTTOM, SectionParameters::RIGHT_TO_LEFT, SectionParameters::FRONT_TO_BACK];

/// A map's three channels (0..1) combined by `axes`: each axis read from its channel, inverted, blended over the axes before it and weighted
pub(crate) fn axes_signal(channels: [f32; 3], axes: &[AxisLayer; 3]) -> f32 {
    AXIS_ORDER.iter().fold(0.0, |base, &index| {
        let axis = axes[index];
        if axis.weight <= 0.0 {
            return base;
        }
        let layer = if axis.invert { 1.0 - channels[index] } else { channels[index] };
        (base + (blended(axis.blend, base, layer) - base) * axis.weight).clamp(0.0, 1.0)
    })
}

/// `layer` blended over `base` by `blend`, unclamped
pub(crate) fn blended(blend: AxisBlend, base: f32, layer: f32) -> f32 {
    match blend {
        AxisBlend::Normal => layer,
        AxisBlend::Add => base + layer,
        AxisBlend::Subtract => base - layer,
        AxisBlend::Multiply => base * layer,
        AxisBlend::AddSub => base + 2.0 * layer - 1.0,
        AxisBlend::Max => base.max(layer),
    }
}

/// Gaussian sigma of the section blur of a generator of `kind`, in texels at [`GeneratorGlobal::BLUR_RESOLUTION`] (0 for kinds without it)
pub(crate) fn blur_sigma(kind: GeneratorKind, section: &SectionParameters) -> f32 {
    if SectionParameters::has_blur(kind) {
        section.blur.clamp(*SectionParameters::BLUR_RANGE.start(), *SectionParameters::BLUR_RANGE.end()) * BLUR_SIGMA_PER_UNIT
    } else {
        0.0
    }
}

/// Gaussian sigma of `generator`'s section blur in texels at 2K (0 without one)
pub(crate) fn generator_blur_sigma(generator: &Generator) -> f32 {
    generator.section_controls().map_or(0.0, |section| blur_sigma(generator.kind, &section))
}

/// The radius of the three box passes that blur like the section blur at `resolution` texels per side
pub(crate) fn blur_radius_at(kind: GeneratorKind, section: &SectionParameters, resolution: u32) -> usize {
    let sigma = blur_sigma(kind, section) * resolution as f32 / GeneratorGlobal::BLUR_RESOLUTION;

    // Three boxes of radius r spread a texel with variance r (r + 1)
    (((1.0 + 4.0 * sigma * sigma).sqrt() - 1.0) * 0.5).round() as usize
}

/// `signal` after the section's balance, contrast, brightness and invert, as far as a generator of `kind` has them
pub(crate) fn adjusted(kind: GeneratorKind, section: &SectionParameters, signal: f32) -> f32 {
    let mut value = signal.clamp(0.0, 1.0);
    if SectionParameters::has_balance(kind) {
        let balance = section.balance.clamp(0.0, 1.0);
        value = if kind == GeneratorKind::AmbientOcclusion {
            GeneratorGlobal { balance, ..GeneratorGlobal::default() }.apply(value)
        } else {
            value.powf(BALANCE_POWER_BASE.powf(2.0 * balance - 1.0))
        };
    }
    value = contrasted(value, section.contrast.clamp(0.0, 1.0));
    if SectionParameters::has_brightness(kind) {
        let brightness = section.brightness.clamp(*SectionParameters::BRIGHTNESS_RANGE.start(), *SectionParameters::BRIGHTNESS_RANGE.end());
        value = if brightness > 0.0 { value + brightness * (1.0 - value) } else { value * (1.0 + brightness) };
    }
    if section.invert { 1.0 - value } else { value }
}

/// `value` with its range narrowed around the middle by `contrast` (1 a step just above the middle)
fn contrasted(value: f32, contrast: f32) -> f32 {
    if contrast <= 0.0 {
        return value;
    }
    let half_range = 0.5 * (1.0 - contrast);
    if 2.0 * half_range < GeneratorGlobal::MIN_RANGE {
        return if value > 0.5 + FULL_CONTRAST_STEP_OFFSET { 1.0 } else { 0.0 };
    }
    ((value - (0.5 - half_range)) / (2.0 * half_range)).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn axis(blend: AxisBlend, weight: f32) -> AxisLayer {
        AxisLayer { invert: false, blend, weight }
    }

    fn defaults(kind: GeneratorKind) -> SectionParameters {
        SectionParameters::defaults(kind).unwrap()
    }

    #[test]
    fn each_axis_reads_its_own_channel() {
        let channels = [0.2, 0.5, 0.9];
        for (index, expected) in channels.iter().enumerate() {
            let mut axes = [axis(AxisBlend::Add, 0.0); 3];
            axes[index].weight = 1.0;
            assert!((axes_signal(channels, &axes) - expected).abs() < 1e-6, "axis {index}");
        }
        let mut inverted = [axis(AxisBlend::Add, 0.0); 3];
        inverted[SectionParameters::TOP_TO_BOTTOM] = AxisLayer { invert: true, ..axis(AxisBlend::Add, 1.0) };
        assert!((axes_signal(channels, &inverted) - 0.5).abs() < 1e-6);
        inverted[SectionParameters::RIGHT_TO_LEFT] = AxisLayer { invert: true, ..axis(AxisBlend::Normal, 1.0) };
        assert!((axes_signal(channels, &inverted) - 0.8).abs() < 1e-6, "inverted red replaces the base");
    }

    #[test]
    fn right_to_left_blends_over_top_to_bottom_and_front_to_back_goes_last() {
        let channels = [0.3, 0.8, 0.6];
        let mut axes = [axis(AxisBlend::Add, 0.0), axis(AxisBlend::Add, 1.0), axis(AxisBlend::Add, 0.0)];
        for (blend, expected) in [
            (AxisBlend::Normal, 0.3),
            (AxisBlend::Add, 1.0),
            (AxisBlend::Subtract, 0.5),
            (AxisBlend::Multiply, 0.24),
            (AxisBlend::AddSub, 0.4),
            (AxisBlend::Max, 0.8),
        ] {
            axes[SectionParameters::RIGHT_TO_LEFT] = axis(blend, 1.0);
            assert!((axes_signal(channels, &axes) - expected).abs() < 1e-6, "{blend:?}");
        }

        axes[SectionParameters::RIGHT_TO_LEFT] = axis(AxisBlend::Normal, 1.0);
        axes[SectionParameters::FRONT_TO_BACK] = axis(AxisBlend::Normal, 1.0);
        assert!((axes_signal(channels, &axes) - 0.6).abs() < 1e-6, "front to back replaces both");
    }

    #[test]
    fn a_weight_lerps_the_unclamped_blend_and_clamps_once() {
        let channels = [0.9, 0.8, 0.0];
        let axes = [axis(AxisBlend::Add, 0.5), axis(AxisBlend::Add, 1.0), axis(AxisBlend::Add, 0.0)];
        assert!((axes_signal(channels, &axes) - 1.0).abs() < 1e-6, "0.8 + 0.45 clamps to 1");
        let axes = [axis(AxisBlend::Subtract, 0.5), axis(AxisBlend::Add, 1.0), axis(AxisBlend::Add, 0.0)];
        assert!((axes_signal(channels, &axes) - 0.35).abs() < 1e-6);
    }

    #[test]
    fn position_balance_is_a_power_curve_and_occlusion_balance_is_linear() {
        let position = SectionParameters { balance: 0.25, ..defaults(GeneratorKind::Position) };
        assert!((adjusted(GeneratorKind::Position, &position, 0.5) - 0.5f32.powf(0.2)).abs() < 1e-5);
        let position = SectionParameters { balance: 0.75, ..position };
        assert!((adjusted(GeneratorKind::Position, &position, 0.5) - 0.5f32.powf(5.0)).abs() < 1e-5);

        let occlusion = SectionParameters { balance: 0.25, ..defaults(GeneratorKind::AmbientOcclusion) };
        assert_eq!(adjusted(GeneratorKind::AmbientOcclusion, &occlusion, 0.4), 0.0, "the lower half goes black");
        assert!((adjusted(GeneratorKind::AmbientOcclusion, &occlusion, 0.75) - 0.5).abs() < 1e-6);

        let curvature = SectionParameters { balance: 0.0, ..defaults(GeneratorKind::Curvature) };
        assert_eq!(adjusted(GeneratorKind::Curvature, &curvature, 0.4), 0.4, "curvature has no balance");
    }

    #[test]
    fn contrast_narrows_around_the_middle_and_full_contrast_steps_just_above_it() {
        let half = SectionParameters { contrast: 0.5, ..defaults(GeneratorKind::Curvature) };
        assert_eq!(adjusted(GeneratorKind::Curvature, &half, 0.25), 0.0);
        assert!((adjusted(GeneratorKind::Curvature, &half, 0.6) - 0.7).abs() < 1e-5);

        let full = SectionParameters { contrast: 1.0, ..half };
        assert_eq!(adjusted(GeneratorKind::Curvature, &full, 0.5), 0.0, "the middle itself stays black");
        assert_eq!(adjusted(GeneratorKind::Curvature, &full, 0.5001), 1.0);
    }

    #[test]
    fn brightness_lerps_towards_white_or_black_then_invert_flips() {
        let brighter = SectionParameters { brightness: 0.5, ..defaults(GeneratorKind::WorldSpaceNormals) };
        assert!((adjusted(GeneratorKind::WorldSpaceNormals, &brighter, 0.4) - 0.7).abs() < 1e-6);
        let darker = SectionParameters { brightness: -0.5, ..brighter };
        assert!((adjusted(GeneratorKind::WorldSpaceNormals, &darker, 0.4) - 0.2).abs() < 1e-6);
        let inverted = SectionParameters { invert: true, ..darker };
        assert!((adjusted(GeneratorKind::WorldSpaceNormals, &inverted, 0.4) - 0.8).abs() < 1e-6);

        let occlusion = SectionParameters { brightness: 0.5, ..defaults(GeneratorKind::AmbientOcclusion) };
        assert_eq!(adjusted(GeneratorKind::AmbientOcclusion, &occlusion, 0.4), 0.4, "ambient occlusion has no brightness");
    }

    #[test]
    fn only_ambient_occlusion_and_world_space_normals_blur() {
        let section = SectionParameters { blur: 16.0, ..defaults(GeneratorKind::Position) };
        assert_eq!(blur_sigma(GeneratorKind::Position, &section), 0.0);
        assert_eq!(blur_sigma(GeneratorKind::WorldSpaceNormals, &section), 72.0);
        assert_eq!(blur_radius_at(GeneratorKind::AmbientOcclusion, &SectionParameters { blur: 0.0, ..section }, 2048), 0);
        let radius = blur_radius_at(GeneratorKind::AmbientOcclusion, &SectionParameters { blur: 1.0, ..section }, 2048) as f32;
        assert!((radius * (radius + 1.0)).sqrt() - 4.5 < 0.6, "three boxes of radius {radius} spread like sigma 4.5");
    }
}
