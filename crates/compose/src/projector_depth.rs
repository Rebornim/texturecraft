//! The depth map of a planar or warp projection: the surface nearest its projector across its view, for depth culling

use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::{Arc, Mutex};

use texturecraft_core::Projection;
use texturecraft_geom::Mesh;
use texturecraft_geom::glam::{Vec2, Vec3};

use crate::pattern::smoothstep;
use crate::projector_view::ProjectorView;

/// The nearest surface in each cell of a grid across a projector's view
pub struct ProjectorDepth {
    view: ProjectorView,
    /// The view point at the grid's lower corner
    origin: Vec2,
    /// One cell's size across and up in the view
    cell: Vec2,
    /// Per cell: the nearness of the nearest surface at the cell centre and its slope across and up (nearness -inf where no surface is)
    front: Vec<[f32; 3]>,
}

/// Built maps by their view and the mesh they come from
type CacheEntry = (ProjectorView, u64, Arc<ProjectorDepth>);

static CACHE: Mutex<Vec<CacheEntry>> = Mutex::new(Vec::new());

impl ProjectorDepth {
    /// The depth map of `proj` over `mesh` within `bounds`, built once per view and mesh; None unless `proj` culls by depth
    pub fn of(mesh: &Mesh, bounds: (Vec3, Vec3), proj: &Projection) -> Option<Arc<ProjectorDepth>> {
        let view = ProjectorView::of(proj, bounds)?;
        let mesh_key = fingerprint(mesh);

        // Built with the cache locked: the build runs no parallel work, and tiles asking for the same map wait for it instead of building it again
        let mut cache = CACHE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(at) = cache.iter().position(|(v, m, _)| *v == view && *m == mesh_key) {
            let entry = cache.remove(at);
            let depth = entry.2.clone();
            cache.insert(0, entry);
            return Some(depth);
        }

        let depth = Arc::new(ProjectorDepth::build(view, mesh)?);
        cache.insert(0, (view, mesh_key, depth.clone()));
        cache.truncate(CACHE_MAPS);
        Some(depth)
    }

    /// Rasterize every triangle of `mesh` in `view`, keeping the nearest per cell (None when no vertex lands in the view)
    pub fn build(view: ProjectorView, mesh: &Mesh) -> Option<ProjectorDepth> {
        let local: Vec<Option<Vec3>> = mesh.positions.iter().map(|p| view.local(*p)).collect();
        let (lo, hi) = local.iter().flatten().fold((Vec2::splat(f32::MAX), Vec2::splat(f32::MIN)), |(lo, hi), l| (lo.min(l.truncate()), hi.max(l.truncate())));
        if lo.x > hi.x || lo.y > hi.y {
            return None;
        }

        let cell = ((hi - lo) / SIZE as f32).max(Vec2::splat(1e-6));
        let mut depth = ProjectorDepth { view, origin: lo, cell, front: vec![[f32::NEG_INFINITY, 0.0, 0.0]; SIZE * SIZE] };
        let mut budget = CELL_BUDGET;
        for tri in mesh.indices.as_chunks::<3>().0 {
            let corner = |i: u32| local.get(i as usize).copied().flatten();
            let (Some(a), Some(b), Some(c)) = (corner(tri[0]), corner(tri[1]), corner(tri[2])) else { continue };
            if !depth.raster([a, b, c], &mut budget) {
                break;
            }
        }
        Some(depth)
    }

    /// Keep triangle `t` where it is nearer than what a cell holds; false once the cell budget runs out
    fn raster(&mut self, t: [Vec3; 3], budget: &mut usize) -> bool {
        let [a, b, c] = t;
        let (d1, d2) = (b - a, c - a);
        let det = d1.x * d2.y - d1.y * d2.x;
        if !det.is_finite() || det.abs() < MIN_SCREEN_AREA {
            // Seen edge on, it covers nothing
            return true;
        }

        let slope = Vec2::new((d1.z * d2.y - d2.z * d1.y) / det, (d1.x * d2.z - d2.x * d1.z) / det).clamp(Vec2::splat(-MAX_SLOPE), Vec2::splat(MAX_SLOPE));

        // Corners in cells: cell (i, j) has its centre at (i + 0.5, j + 0.5)
        let [pa, pb, pc] = t.map(|p| (p.truncate() - self.origin) / self.cell);
        let (lo, hi) = (pa.min(pb).min(pc), pa.max(pb).max(pc));
        let first = |v: f32| (v - 0.5).ceil().clamp(0.0, SIZE as f32) as usize;
        let last = |v: f32| ((v - 0.5).floor() + 1.0).clamp(0.0, SIZE as f32) as usize;
        let (x0, x1, y0, y1) = (first(lo.x), last(hi.x), first(lo.y), last(hi.y));
        let cells = x1.saturating_sub(x0).saturating_mul(y1.saturating_sub(y0));
        let Some(left) = budget.checked_sub(cells) else { return false };
        *budget = left;

        let area = edge(pa, pb, pc);
        for y in y0..y1 {
            for x in x0..x1 {
                let q = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
                let inside = [edge(pb, pc, q), edge(pc, pa, q), edge(pa, pb, q)].iter().all(|w| w * area.signum() >= -1e-6 * area.abs());
                if !inside {
                    continue;
                }

                let near = a.z + slope.dot(self.origin + q * self.cell - a.truncate());
                if let Some(cell) = self.front.get_mut(y * SIZE + x)
                    && near > cell[0]
                {
                    *cell = [near, slope.x, slope.y];
                }
            }
        }
        true
    }

    /// How much depth culling with `hardness` leaves of a projection at world point `p`: 0 behind the surface nearest the projector, 1 on it or where nothing is in front
    pub fn cover(&self, p: Vec3, hardness: f32) -> f32 {
        let Some(l) = self.view.local(p) else { return 1.0 };
        let at = (l.truncate() - self.origin) / self.cell;
        if !(at.x >= 0.0 && at.y >= 0.0 && at.x < SIZE as f32 && at.y < SIZE as f32) {
            return 1.0;
        }

        let (x, y) = (at.x as usize, at.y as usize);
        let Some(&[near, across, up]) = self.front.get(y * SIZE + x).filter(|cell| cell[0].is_finite()) else { return 1.0 };

        // The nearest surface's plane carried from the cell centre to the point, so a surface is never behind itself
        let centre = self.origin + (Vec2::new(x as f32, y as f32) + 0.5) * self.cell;
        let slope = Vec2::new(across, up);
        let behind = near + slope.dot(l.truncate() - centre) - l.z;
        let unit = self.view.unit();

        // Neighbouring triangles of a curved surface part from the carried plane by about one cell of its slope
        let bias = BIAS * unit + (slope.abs() * self.cell).element_sum().min(MAX_SLOPE_BIAS * unit);
        let width = (1.0 - hardness.clamp(0.0, 1.0)) * FADE * unit;
        if width <= f32::EPSILON {
            return f32::from(u8::from(behind <= bias));
        }

        1.0 - smoothstep(bias, bias + width, behind)
    }
}

/// Twice the signed area of triangle `a`, `b`, `q`
fn edge(a: Vec2, b: Vec2, q: Vec2) -> f32 {
    (b - a).perp_dot(q - a)
}

/// Which mesh a map was built for: its sizes and a spread of its vertices and indices
fn fingerprint(mesh: &Mesh) -> u64 {
    let mut h = DefaultHasher::new();
    (mesh.positions.len(), mesh.indices.len()).hash(&mut h);
    for p in mesh.positions.iter().step_by((mesh.positions.len() / FINGERPRINT_SAMPLES).max(1)) {
        p.to_array().map(f32::to_bits).hash(&mut h);
    }
    for i in mesh.indices.iter().step_by((mesh.indices.len() / FINGERPRINT_SAMPLES).max(1)) {
        i.hash(&mut h);
    }
    h.finish()
}

/// Cells across and up the grid
const SIZE: usize = 512;

/// A triangle flatter than this on screen is seen edge on
const MIN_SCREEN_AREA: f32 = 1e-12;

/// Most cells all triangles together may visit before the rest are left out (hostile meshes of huge overlapping triangles)
const CELL_BUDGET: usize = 256 * SIZE * SIZE;

/// Steepest surface slope kept, in nearness per view unit
const MAX_SLOPE: f32 = 1e4;

/// How far behind the nearest surface a point may sit and keep everything, in mesh radii
const BIAS: f32 = 0.002;

/// Most the slope adds to the bias, in mesh radii
const MAX_SLOPE_BIAS: f32 = 0.05;

/// How far behind the nearest surface culling fades out at hardness 0, in mesh radii
const FADE: f32 = 0.1;

/// Maps kept built (each is about 3 MB)
const CACHE_MAPS: usize = 8;

/// Vertices and indices the mesh fingerprint reads
const FINGERPRINT_SAMPLES: usize = 64;

#[cfg(test)]
mod tests {
    use super::*;
    use texturecraft_core::{Projector, UvTransform};

    /// Two squares facing +Z, one at z = `front` covering x 0..1, one at z = `back` covering x -1..1
    fn two_squares(front: f32, back: f32) -> Mesh {
        let positions = vec![
            Vec3::new(0.0, -1.0, front),
            Vec3::new(1.0, -1.0, front),
            Vec3::new(1.0, 1.0, front),
            Vec3::new(0.0, 1.0, front),
            Vec3::new(-1.0, -1.0, back),
            Vec3::new(1.0, -1.0, back),
            Vec3::new(1.0, 1.0, back),
            Vec3::new(-1.0, 1.0, back),
        ];
        Mesh { positions, indices: vec![0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7], ..Mesh::default() }
    }

    fn planar_z(on: bool, hardness: f32) -> Projection {
        let mut projector = Projector::depth_culled();
        projector.depth.on = on;
        projector.depth.hardness = hardness;
        Projection::Planar { scale: 1.0, axis: 2, offset: [0.0; 3], uv: UvTransform::default(), projector, placed: None }
    }

    #[test]
    fn surfaces_behind_others_are_culled_and_the_nearest_keeps_everything() {
        let mesh = two_squares(0.5, -0.5);
        let bounds = mesh.bounds();
        let depth = ProjectorDepth::build(ProjectorView::of(&planar_z(true, 1.0), bounds).unwrap(), &mesh).unwrap();
        assert_eq!(depth.cover(Vec3::new(0.5, 0.3, 0.5), 1.0), 1.0, "the front square");
        assert_eq!(depth.cover(Vec3::new(0.5, 0.3, -0.5), 1.0), 0.0, "the back square behind it");
        assert_eq!(depth.cover(Vec3::new(-0.5, 0.3, -0.5), 1.0), 1.0, "the back square where nothing is in front");
        let soft = depth.cover(Vec3::new(0.5, 0.3, 0.5 - 0.01), 0.0);
        assert!(soft > 0.0 && soft < 1.0, "a soft cull fades just behind the front: {soft}");
        assert!(ProjectorView::of(&planar_z(false, 1.0), bounds).is_none(), "off, there is nothing to build");
    }

    #[test]
    fn a_slanted_surface_is_never_behind_itself() {
        let positions = vec![Vec3::new(-1.0, -1.0, -0.9), Vec3::new(1.0, -1.0, 0.9), Vec3::new(1.0, 1.0, 0.9), Vec3::new(-1.0, 1.0, -0.9)];
        let mesh = Mesh { positions, indices: vec![0, 1, 2, 0, 2, 3], ..Mesh::default() };
        let depth = ProjectorDepth::build(ProjectorView::of(&planar_z(true, 1.0), mesh.bounds()).unwrap(), &mesh).unwrap();
        for i in 0..1000 {
            let (x, y) = ((i as f32 * 0.618).fract() * 1.98 - 0.99, (i as f32 * 0.377).fract() * 1.98 - 0.99);
            assert_eq!(depth.cover(Vec3::new(x, y, x * 0.9), 1.0), 1.0, "{x} {y}");
        }
    }
}
