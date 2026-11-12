//! The projections a fill's Projection dropdown offers

use texturecraft_engine::core::Projection;

/// The projections a fill's Projection dropdown offers, with their command ids
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProjectionKind {
    UvTile,
    Uv,
    Triplanar,
    Planar,
    Spherical,
    Cylindrical,
}

impl ProjectionKind {
    pub(crate) const ALL: [ProjectionKind; 6] =
        [ProjectionKind::UvTile, ProjectionKind::Uv, ProjectionKind::Triplanar, ProjectionKind::Planar, ProjectionKind::Spherical, ProjectionKind::Cylindrical];

    /// The `projection` param of `layer.fill.projection`
    pub(crate) fn id(self) -> &'static str {
        match self {
            ProjectionKind::UvTile => "uvTile",
            ProjectionKind::Uv => "uv",
            ProjectionKind::Triplanar => "triplanar",
            ProjectionKind::Planar => "planar",
            ProjectionKind::Spherical => "spherical",
            ProjectionKind::Cylindrical => "cylindrical",
        }
    }

    pub(crate) fn of(projection: &Projection) -> Option<ProjectionKind> {
        match projection {
            Projection::UvTile => Some(ProjectionKind::UvTile),
            Projection::Uv { .. } => Some(ProjectionKind::Uv),
            Projection::Triplanar { .. } => Some(ProjectionKind::Triplanar),
            Projection::Planar { .. } => Some(ProjectionKind::Planar),
            Projection::Spherical { .. } => Some(ProjectionKind::Spherical),
            Projection::Cylindrical { .. } => Some(ProjectionKind::Cylindrical),
            // Not in the dropdown until the panel's warp rows land
            Projection::Warp { .. } => None,
        }
    }

    /// The dropdown's name for it
    pub(crate) fn label(self) -> &'static str {
        use crate::i18n::n_;
        match self {
            ProjectionKind::UvTile => n_("Fill (Match per UV Tile)"),
            ProjectionKind::Uv => n_("UV projection"),
            ProjectionKind::Triplanar => n_("Tri-planar projection"),
            ProjectionKind::Planar => n_("Planar projection"),
            ProjectionKind::Spherical => n_("Spherical projection"),
            ProjectionKind::Cylindrical => n_("Cylindrical projection"),
        }
    }
}
