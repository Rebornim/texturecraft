//! A soft rectangular light of the built-in environments

use texturecraft_geom::glam::Vec3;

/// A soft rectangular light: azimuth/elevation center (degrees), half sizes (degrees).
pub(crate) struct SoftBox {
    pub az: f32,
    pub el: f32,
    pub hw: f32,
    pub hh: f32,
    pub color: Vec3,
}

impl SoftBox {
    pub fn eval(&self, d: Vec3) -> Vec3 {
        let el = d.y.clamp(-1.0, 1.0).asin().to_degrees();
        let az = d.z.atan2(d.x).to_degrees();
        let daz = ((az - self.az + 540.0).rem_euclid(360.0)) - 180.0;
        let (x, y) = (daz.abs() / self.hw, (el - self.el).abs() / self.hh);
        let e = |t: f32| 1.0 - crate::environment_sky::smooth((t - 0.8) / 0.4);
        self.color * e(x) * e(y)
    }
}
