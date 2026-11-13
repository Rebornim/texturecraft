//! The built-in environments (drawn in code, no image assets) and their diffuse light as spherical harmonics: the viewport
//! lights with them and Baked Lighting Environment bakes them into the base color
//!
//! Equirect convention: `u = φ / 2π`, `v = θ / π`, direction `(sin θ cos φ, cos θ, sin θ sin φ)`; row 0 is straight up.

use std::f32::consts::{PI, TAU};

use texturecraft_geom::glam::{Vec2, Vec3};

use crate::soft_box::SoftBox;

/// The direction at equirect place `u`, `v` (row 0 straight up)
pub fn dir_of(u: f32, v: f32) -> Vec3 {
    let (theta, phi) = (v * PI, u * TAU);
    Vec3::new(theta.sin() * phi.cos(), theta.cos(), theta.sin() * phi.sin())
}

/// The equirect place of direction `d`
pub fn uv_of(d: Vec3) -> Vec2 {
    let phi = d.z.atan2(d.x);
    Vec2::new((phi / TAU).rem_euclid(1.0), d.y.clamp(-1.0, 1.0).acos() / PI)
}

/// Smoothstep of `t` over 0..1
pub(crate) fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A sky blending `horizon` up to `zenith` and down to `ground`
fn sky(d: Vec3, zenith: Vec3, horizon: Vec3, ground: Vec3) -> Vec3 {
    if d.y >= 0.0 { horizon.lerp(zenith, d.y.powf(0.6)) } else { horizon.lerp(ground, (-d.y).powf(0.25)) }
}

/// The light a built-in environment `name` sends from direction `d` (unknown names give the default studio)
pub fn radiance(name: &str, d: Vec3) -> Vec3 {
    match name {
        "Studio Contrast" => {
            let base = sky(d, Vec3::splat(0.04), Vec3::splat(0.03), Vec3::splat(0.01));
            base + SoftBox { az: 35.0, el: 30.0, hw: 22.0, hh: 18.0, color: Vec3::new(14.0, 13.2, 12.0) }.eval(d)
                + SoftBox { az: -150.0, el: 40.0, hw: 6.0, hh: 30.0, color: Vec3::new(4.0, 4.4, 5.0) }.eval(d)
        }
        "Overcast" => {
            let base = sky(d, Vec3::new(1.2, 1.25, 1.35), Vec3::new(0.95, 0.95, 0.95), Vec3::new(0.18, 0.2, 0.15));
            base + SoftBox { az: 60.0, el: 55.0, hw: 40.0, hh: 25.0, color: Vec3::splat(0.6) }.eval(d)
        }
        "Sunset" => {
            let base = sky(d, Vec3::new(0.15, 0.25, 0.55), Vec3::new(1.4, 0.65, 0.3), Vec3::new(0.08, 0.06, 0.05));
            let sun_dir = dir_of(0.12, 0.47);
            let sun = smooth((d.dot(sun_dir) - 0.9985) / 0.0012) * 120.0;
            base + Vec3::new(1.0, 0.6, 0.3) * sun
        }
        "Warehouse" => {
            let mut c = sky(d, Vec3::splat(0.25), Vec3::splat(0.18), Vec3::splat(0.06));
            for k in 0..6 {
                c += SoftBox { az: -150.0 + k as f32 * 60.0, el: 50.0, hw: 6.0, hh: 3.0, color: Vec3::new(9.0, 8.5, 7.5) }.eval(d);
            }
            c + SoftBox { az: 90.0, el: 5.0, hw: 30.0, hh: 12.0, color: Vec3::new(1.6, 1.8, 2.2) }.eval(d)
        }
        "Night" => {
            let base = sky(d, Vec3::new(0.01, 0.015, 0.04), Vec3::new(0.03, 0.03, 0.06), Vec3::splat(0.005));
            base + SoftBox { az: 20.0, el: 15.0, hw: 10.0, hh: 10.0, color: Vec3::new(0.4, 1.6, 4.0) }.eval(d)
                + SoftBox { az: -110.0, el: 10.0, hw: 12.0, hh: 8.0, color: Vec3::new(4.0, 0.8, 1.6) }.eval(d)
        }
        // "Studio Soft" (default)
        _ => {
            let base = sky(d, Vec3::new(0.5, 0.52, 0.56), Vec3::new(0.32, 0.32, 0.33), Vec3::new(0.08, 0.075, 0.07));
            base + SoftBox { az: 40.0, el: 35.0, hw: 28.0, hh: 20.0, color: Vec3::new(5.5, 5.2, 4.8) }.eval(d)
                + SoftBox { az: -125.0, el: 20.0, hw: 35.0, hh: 22.0, color: Vec3::new(1.1, 1.25, 1.5) }.eval(d)
                + SoftBox { az: 175.0, el: 55.0, hw: 8.0, hh: 25.0, color: Vec3::new(3.0, 3.0, 3.0) }.eval(d)
        }
    }
}

/// The 9 spherical-harmonics basis functions at unit direction `d`
pub fn sh_basis(d: Vec3) -> [f32; 9] {
    let (x, y, z) = (d.x, d.y, d.z);
    [
        0.282_095,
        0.488_603 * y,
        0.488_603 * z,
        0.488_603 * x,
        1.092_548 * x * y,
        1.092_548 * y * z,
        0.315_392 * (3.0 * z * z - 1.0),
        1.092_548 * x * z,
        0.546_274 * (x * x - y * y),
    ]
}

/// Diffuse irradiance as 9 SH coefficients of the equirect image `width` x `height` whose texels `radiance` gives (already
/// convolved with the cosine lobe and divided by π, so `Σ c·Y(n)` times albedo is the diffuse radiance)
pub fn sh9(width: u32, height: u32, radiance: impl Fn(u32, u32) -> Vec3) -> [[f32; 3]; 9] {
    let (w, h) = (width.max(1), height.max(1));
    let mut c = [Vec3::ZERO; 9];
    for y in 0..h {
        let theta = PI * (y as f32 + 0.5) / h as f32;
        let dw = (TAU / w as f32) * (PI / h as f32) * theta.sin();
        for x in 0..w {
            let d = dir_of((x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32);
            // No firefly clamp: the projection integrates, so a bright sun lights diffuse fully.
            let l = radiance(x, y) * dw;
            for (k, ylm) in sh_basis(d).iter().enumerate() {
                c[k] += l * *ylm;
            }
        }
    }
    let a = [PI, 2.0 * PI / 3.0, 2.0 * PI / 3.0, 2.0 * PI / 3.0, PI / 4.0, PI / 4.0, PI / 4.0, PI / 4.0, PI / 4.0];
    std::array::from_fn(|k| (c[k] * a[k] / PI).to_array())
}
