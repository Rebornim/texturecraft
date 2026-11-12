use texturecraft_geom::glam::{Mat3, Vec2, Vec3};

/// One copy of a fill's projection: a texel at `p` shows what the projection gives at [`FillCopy::point`]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FillCopy {
    turn: Mat3,
    pivot: Vec3,
    flip: [bool; 2],
}

impl FillCopy {
    /// A copy sampling the projection at `pivot + turn * (p - pivot)`, its image turned over across and down as `flip` says
    pub fn new(turn: Mat3, pivot: Vec3, flip: [bool; 2]) -> FillCopy {
        FillCopy { turn, pivot, flip }
    }

    /// The surface point the projection is sampled at for a texel at world point `p`
    pub fn point(&self, p: Vec3) -> Vec3 {
        self.pivot + self.turn * (p - self.pivot)
    }

    /// The normal the projection sees for a texel whose normal is `n`
    pub fn normal(&self, n: Vec3) -> Vec3 {
        self.turn * n
    }

    /// `uv` in the copy's image, turned over across and down as its Flip copy says
    pub fn uv(&self, uv: Vec2) -> Vec2 {
        let [across, down] = self.flip;
        Vec2::new(if across { 1.0 - uv.x } else { uv.x }, if down { 1.0 - uv.y } else { uv.y })
    }
}
