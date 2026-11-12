//! Reading saved projections, including decals saved before they were planar projections placed on the surface

use serde::{Deserialize, Deserializer};

use crate::{Decal, Projection};

/// A projection as saved: today's, or a decal from before decals were planar projections
#[derive(Deserialize)]
enum SavedProjection {
    Decal(Decal),
    #[serde(untagged)]
    Current(Projection),
}

/// Serde hook for projection fields: an older decal loads as the planar projection that looks the same
pub(crate) fn load<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Projection, D::Error> {
    Ok(match SavedProjection::deserialize(deserializer)? {
        SavedProjection::Decal(frame) => Projection::decal(frame),
        SavedProjection::Current(projection) => projection,
    })
}
