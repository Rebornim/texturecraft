//! The kind of copies a symmetric fill makes: mirrored or turned round an axis

use serde::{Deserialize, Serialize};

/// How a symmetric fill copies its projection
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SymmetryKind {
    /// One copy mirrored across the plane through the axis position, square to the axis
    #[default]
    Mirror,
    /// Copies turned round the axis through the axis position
    Radial,
}

impl SymmetryKind {
    pub const ALL: [SymmetryKind; 2] = [SymmetryKind::Mirror, SymmetryKind::Radial];

    /// The id commands use
    pub fn id(self) -> &'static str {
        match self {
            SymmetryKind::Mirror => "mirror",
            SymmetryKind::Radial => "radial",
        }
    }

    pub fn from_id(id: &str) -> Option<SymmetryKind> {
        SymmetryKind::ALL.into_iter().find(|kind| kind.id() == id)
    }
}
