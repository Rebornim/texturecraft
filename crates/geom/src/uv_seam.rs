//! UV seams: mesh edges two triangles share in 3D but not in UV space

use std::collections::HashMap;

use crate::{Mesh, UvSeamSide};

/// Edge ends closer than this share a UV position
const SAME_UV: f32 = 1e-5;

/// A UV seam: the same 3D edge on two UV islands; both sides' starts are the same 3D point
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UvSeam {
    pub sides: [UvSeamSide; 2],
}

impl UvSeam {
    /// The seams between triangles of `material`
    pub fn of_material(mesh: &Mesh, material: usize) -> Vec<UvSeam> {
        // Corners within a millionth of the mesh size are the same 3D point
        let (minimum, maximum) = mesh.bounds();
        let tolerance = (maximum - minimum).length().max(1e-6) * 1e-6;
        let position_key = |index: usize| mesh.positions.get(index).map(|position| position.to_array().map(|value| (value / tolerance).round() as i64));
        let mut first_sides = HashMap::new();
        let mut seams = Vec::new();
        for triangle in mesh.triangles_of(material) {
            let Some(corners) = mesh.tri(triangle) else {
                continue;
            };

            for edge in 0..3 {
                let (first, second, third) = (corners[edge], corners[(edge + 1) % 3], corners[(edge + 2) % 3]);
                let (Some(first_key), Some(second_key)) = (position_key(first), position_key(second)) else {
                    continue;
                };

                // A pole's collapsed edges have no length to cross
                if first_key == second_key {
                    continue;
                }

                let (Some(&first_uv), Some(&second_uv), Some(&interior)) = (mesh.uvs.get(first), mesh.uvs.get(second), mesh.uvs.get(third)) else {
                    continue;
                };

                // Each side starts at the corner with the smaller key, so both sides run the same way
                let (key, side) = if first_key <= second_key {
                    ((first_key, second_key), UvSeamSide { start: first_uv, end: second_uv, interior })
                } else {
                    ((second_key, first_key), UvSeamSide { start: second_uv, end: first_uv, interior })
                };
                let Some(other) = first_sides.insert(key, side) else {
                    continue;
                };

                if side.start.distance(other.start) > SAME_UV || side.end.distance(other.end) > SAME_UV {
                    seams.push(UvSeam { sides: [other, side] });
                }
            }
        }

        seams
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_closed_mesh_has_seams_and_a_single_island_has_none() {
        let sphere = crate::primitives::sphere(1.0, 16, 8);
        let plane = crate::primitives::plane(1.0, 4);
        let seams = UvSeam::of_material(&sphere, 0);

        assert!(!seams.is_empty());
        assert!(seams.iter().all(|seam| seam.sides[0].start != seam.sides[1].start || seam.sides[0].end != seam.sides[1].end));
        assert!(UvSeam::of_material(&plane, 0).is_empty());
    }
}
