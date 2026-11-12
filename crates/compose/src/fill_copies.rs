//! The copies a symmetric fill makes of its projection, as mirrors and turns of the surface point each copy samples

use texturecraft_core::{FillSymmetry, SymmetryKind};
use texturecraft_geom::glam::{Mat3, Vec3};

use crate::fill_copy::FillCopy;

/// The copies of a fill's projection one texel shows, the original among them: the one whose half (mirror) or wedge (radial) holds the texel comes first, so it lies on top where copies overlap
pub struct FillCopies {
    symmetry: FillSymmetry,
    pivot: Vec3,
    first: u32,
    count: u32,
    next: u32,
}

impl FillCopies {
    /// The copies `symmetry` makes for a texel at world point `p`, on a mesh whose bounds have centre `center` and radius `radius`
    pub fn new(symmetry: &FillSymmetry, p: Vec3, center: Vec3, radius: f32) -> FillCopies {
        let reach = FillSymmetry::REACH;
        let position = Vec3::from(symmetry.position.map(|v| if v.is_finite() { v.clamp(-reach, reach) } else { 0.0 }));
        let pivot = center + position * radius;
        let axis = usize::from(symmetry.axis.min(2));
        let count = symmetry.copies();
        let d = p - pivot;
        let first = match symmetry.kind {
            // The original keeps the half on the axis's positive side
            SymmetryKind::Mirror => u32::from(d[axis] < 0.0),
            SymmetryKind::Radial => {
                let turned = d[(axis + 2) % 3].atan2(d[(axis + 1) % 3]).to_degrees().rem_euclid(360.0);
                let step = symmetry.step().max(f32::EPSILON);
                let gap = |k: u32| {
                    let off = (turned - k as f32 * step).rem_euclid(360.0);
                    off.min(360.0 - off)
                };
                (0..count).min_by(|a, b| gap(*a).total_cmp(&gap(*b))).unwrap_or(0)
            }
        };
        FillCopies { symmetry: *symmetry, pivot, first, count, next: 0 }
    }

    /// Copy `k`: the original at 0, then the mirror or the radial copies in turn
    fn copy(&self, k: u32) -> FillCopy {
        let axis = usize::from(self.symmetry.axis.min(2));
        let flip = if k == 0 { [false; 2] } else { self.symmetry.flip };
        let turn = match self.symmetry.kind {
            _ if k == 0 => Mat3::IDENTITY,
            SymmetryKind::Mirror => {
                let mut scale = Vec3::ONE;
                scale[axis] = -1.0;
                Mat3::from_diagonal(scale)
            }
            // Copy `k` is the original turned by `k` steps, so a texel samples the original turned back
            SymmetryKind::Radial => Mat3::from_axis_angle(Vec3::AXES[axis], -(k as f32 * self.symmetry.step()).to_radians()),
        };
        FillCopy::new(turn, self.pivot, flip)
    }
}

impl Iterator for FillCopies {
    type Item = FillCopy;

    fn next(&mut self) -> Option<FillCopy> {
        if self.next >= self.count {
            return None;
        }
        let k = (self.first + self.next) % self.count;
        self.next += 1;
        Some(self.copy(k))
    }
}

#[cfg(test)]
mod tests {
    use texturecraft_geom::glam::Vec2;

    use super::*;

    fn symmetry(kind: SymmetryKind, axis: u8) -> FillSymmetry {
        FillSymmetry { on: true, kind, axis, ..FillSymmetry::OFF }
    }

    #[test]
    fn a_mirror_puts_the_texels_own_half_first_and_reflects_across_the_axis_position() {
        let mut mirror = symmetry(SymmetryKind::Mirror, 0);
        mirror.position = [0.5, 0.0, 0.0];
        let left: Vec<FillCopy> = FillCopies::new(&mirror, Vec3::new(-1.0, 0.0, 0.0), Vec3::ZERO, 2.0).collect();
        assert_eq!(left.len(), 2);
        // The plane passes at x = 1 (half a radius of 2): a point at -1 sees the original at 3 first
        assert!(left[0].point(Vec3::new(-1.0, 0.2, 0.0)).abs_diff_eq(Vec3::new(3.0, 0.2, 0.0), 1e-5));
        assert!(left[1].point(Vec3::new(-1.0, 0.2, 0.0)).abs_diff_eq(Vec3::new(-1.0, 0.2, 0.0), 1e-5));
        assert!(left[0].normal(Vec3::X).abs_diff_eq(-Vec3::X, 1e-5));
        let right: Vec<FillCopy> = FillCopies::new(&mirror, Vec3::new(2.0, 0.0, 0.0), Vec3::ZERO, 2.0).collect();
        assert!(right[0].point(Vec3::new(2.0, 0.3, 0.0)).abs_diff_eq(Vec3::new(2.0, 0.3, 0.0), 1e-5));
    }

    #[test]
    fn radial_copies_turn_by_their_step_and_flip_only_the_copies() {
        let mut radial = symmetry(SymmetryKind::Radial, 2);
        radial.count = 4;
        radial.flip = [true, false];
        let copies: Vec<FillCopy> = FillCopies::new(&radial, Vec3::X, Vec3::ZERO, 1.0).collect();
        assert_eq!(copies.len(), 4);
        assert_eq!(copies[0].uv(Vec2::new(0.25, 0.5)), Vec2::new(0.25, 0.5));
        assert_eq!(copies[1].uv(Vec2::new(0.25, 0.5)), Vec2::new(0.75, 0.5));
        // Copy 1 sits a quarter turn on: a texel on +Y samples the original on +X
        assert!(copies[1].point(Vec3::Y).abs_diff_eq(Vec3::X, 1e-5));
        // A texel on +Y lies in copy 1's wedge, so copy 1 comes first there
        let on_y: Vec<FillCopy> = FillCopies::new(&radial, Vec3::Y, Vec3::ZERO, 1.0).collect();
        assert!(on_y[0].point(Vec3::Y).abs_diff_eq(Vec3::X, 1e-5));
    }

    #[test]
    fn a_span_under_a_full_turn_puts_the_last_copy_at_its_end() {
        let mut radial = symmetry(SymmetryKind::Radial, 1);
        radial.count = 4;
        radial.span = 180.0;
        assert_eq!(radial.step(), 60.0);
        radial.span = 360.0;
        assert_eq!(radial.step(), 90.0);
    }
}
