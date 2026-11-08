//! The Path tool: control points placed on the surface (3D view) or in texture space (2D view)
//! and a smooth curve through them, painted with the brush, or, when closed and filled, as an
//! antialiased lasso fill of the visible surface (3D) or of the UV region (2D).
//!
//! Paths are non-destructive: a committed path is stored in a path sublayer
//! ([`EffectKind::Path`]) that is repainted from its paths whenever one is added, edited
//! (`path.edit`, live while the points, brush or material change) or removed (`path.remove`).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use texturecraft_compose::{ImageRes, PaintTarget, PixelStore};
use texturecraft_core::{Channel, Effect, EffectKind, LayerId, Mask, MaskBackground, PathShape};
use texturecraft_geom::glam::{Vec2, Vec3};
use texturecraft_geom::{OrbitCamera, surface_at};
use texturecraft_paint::{BrushSettings, Lasso, PickCtx, fill_coverage, pick, spline, spline_segments};
use texturecraft_texel::{TILE_AREA, TileImage};

use crate::commands::{CommandSpec, always, bad, boolp, has_layer, u64p};
use crate::path_draft::PathDraft;
use crate::session::{StackTarget, Tool};
use crate::tool_cmds::{points_param, set_tool, viewport_of};
use crate::{Document, EngineError, Result, Session, cmd};

/// Control points kept at most.
pub const MAX_PATH_POINTS: usize = 4096;
/// Bytes of tiles the path cache's snapshots may hold (plus the edited path's); the oldest go
/// first.
const CACHE_BYTES: usize = 256 << 20;

/// The undo step that adds a new path
pub(crate) const NEW_PATH_LABEL: &str = "Path";
/// Curve sampling step in viewport pixels (3D view) and texture units (2D view).
const STEP_PX: f32 = 2.0;
const STEP_UV: f32 = 0.001;
/// A stored path's viewport is clamped to this range (pixels per side) before painting from it
const MIN_PAINT_VIEWPORT_PX: f32 = 16.0;
const MAX_PAINT_VIEWPORT_PX: f32 = 16384.0;
/// Sampling step of a 3D path's curve on the mesh for the views, in its paint viewport pixels
const SURFACE_STEP_PX: f32 = 4.0;
const MIN_PROJECTIVE_W: f32 = 1e-6;
// Allow ray hits to differ slightly from transformed texel positions
const RAY_HIT_RELATIVE_TOLERANCE: f32 = 0.001;

/// The curve being edited with the Path tool
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PathState {
    /// Control points: surface positions (3D view) or texture coordinates with z = 0 (2D view)
    pub points: Vec<Vec3>,
    /// The points are texture coordinates (placed in the 2D view)
    pub uv: bool,
    /// The curve loops back to its first point
    pub closed: bool,
    /// A closed path fills its interior instead of stroking the curve
    pub fill: bool,
    /// The stored path being edited: (path sublayer, index). Changes repaint it live
    #[serde(default)]
    pub editing: Option<(LayerId, usize)>,
    /// This edit has already repainted: later changes join its undo step, not an earlier edit's
    #[serde(default)]
    pub edited: bool,
    /// The selected control point (drawn in the accent color; Backspace or Remove point removes it)
    #[serde(default)]
    pub selected: Option<usize>,
}

impl PathState {
    /// Append a control point; a point placed in the other view starts a new path.
    pub fn push(&mut self, p: Vec3, uv: bool) {
        if uv != self.uv {
            self.points.clear();
            self.uv = uv;
        }
        if self.points.len() < MAX_PATH_POINTS {
            self.points.push(p);
        }
    }

    /// Insert a control point before point `at` (on the segment ending there) and select it; None when it can't
    pub fn insert(&mut self, at: usize, p: Vec3) -> Option<usize> {
        if at > self.points.len() || self.points.len() >= MAX_PATH_POINTS {
            return None;
        }

        self.points.insert(at, p);
        self.selected = Some(at);
        self.selected
    }

    /// The same curve (points, open or closed, filled, view) as `other`
    fn same_curve(&self, other: &PathState) -> bool {
        (&self.points, self.closed, self.fill, self.uv) == (&other.points, other.closed, other.fill, other.uv)
    }

    /// Empty the tool for the next path: no points, open, nothing selected or edited
    pub fn clear(&mut self) {
        self.points.clear();
        self.closed = false;
        self.selected = None;
        self.editing = None;
        self.edited = false;
    }

    /// Remove the selected control point, or the last one when none is selected
    pub fn remove_selected(&mut self) {
        let at = self.selected.take().filter(|i| *i < self.points.len()).or(self.points.len().checked_sub(1));
        if let Some(i) = at {
            self.points.remove(i);
        }
        if self.closed && self.points.len() < 3 {
            self.closed = false;
        }
    }
}

/// Mesh id, texture size, owner layer channels, texture set channels.
type CacheKey = (u64, Option<(u32, u32)>, Vec<Channel>, Vec<Channel>);
/// A path sublayer's images: its mask (`None`) and each channel.
type Snap = BTreeMap<Option<Channel>, TileImage>;
/// A stored path and the brush alpha image it was painted with.
type Painted = (PathShape, Option<Arc<ImageRes>>);

/// A path sublayer as last painted for `key`: its paths, its images after each path
/// (`snaps[k]` after the first k; tiles are shared, so a snapshot costs only the tiles that
/// path changed; None once [`evict`]ed) and the tiles each path painted.
#[derive(Clone)]
pub(crate) struct PathCache {
    eid: LayerId,
    key: CacheKey,
    shapes: Vec<Painted>,
    snaps: Vec<Option<Snap>>,
    tiles: Vec<BTreeSet<(u32, u32)>>,
}

/// Drop the snapshots past `budget` bytes, oldest first, so adding a path and editing recent
/// ones stay cheap, but keep snapshot `edited` (below the path being edited, so its next live
/// repaint starts there). Snapshot k holds at most path k-1's tiles in each image; the first
/// (empty) and the last (the live pixels) cost nothing.
fn evict(snaps: &mut [Option<Snap>], tiles: &[BTreeSet<(u32, u32)>], budget: usize, edited: usize) {
    let mut bytes = 0;
    for k in (1..snaps.len().saturating_sub(1)).rev().filter(|&k| k != edited) {
        if let Some(snap) = &snaps[k] {
            bytes += tiles[k - 1].len() * snap.values().map(|i| TILE_AREA * i.comps() * 2).sum::<usize>();
            if bytes > budget {
                snaps[k] = None;
            }
        }
    }
}

fn same_handle<T>(a: Option<&Arc<T>>, b: Option<&Arc<T>>) -> bool {
    match (a, b) {
        (Some(x), Some(y)) => Arc::ptr_eq(x, y),
        (x, y) => x.is_none() && y.is_none(),
    }
}

fn same_path(a: &Painted, b: &Painted) -> bool {
    a.0 == b.0 && same_handle(a.1.as_ref(), b.1.as_ref())
}

/// Whether `a` and `b` hold the same tiles at `tiles`, by shared handle (painting a tile always
/// gives it a new one); a missing image reads as empty.
fn same_tiles(a: &Snap, b: &Snap, tiles: &BTreeSet<(u32, u32)>) -> bool {
    a.keys().chain(b.keys()).all(|k| {
        let (x, y) = (a.get(k), b.get(k));
        let dims = |i: &TileImage| (i.width(), i.height(), i.comps());
        x.zip(y).is_none_or(|(x, y)| dims(x) == dims(y))
            && tiles.iter().all(|&(tx, ty)| same_handle(x.and_then(|i| i.tile_handle(tx, ty)), y.and_then(|i| i.tile_handle(tx, ty))))
    })
}

/// Copy `from`'s tiles at `tiles` into `to`.
fn copy_tiles(to: &mut Snap, from: &Snap, tiles: &BTreeSet<(u32, u32)>) {
    for (k, src) in from {
        to.entry(*k).or_insert_with(|| TileImage::new(src.width(), src.height(), src.comps()));
    }
    for (k, dst) in to.iter_mut() {
        let src = from.get(k);
        for &(x, y) in tiles {
            dst.set_tile(x, y, src.and_then(|i| i.tile_handle(x, y)).cloned());
        }
    }
}

/// What a stored path is painted with ([`PathShape::paint`]).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct PathPaint {
    brush: BrushSettings,
    values: BTreeMap<Channel, [f32; 3]>,
    enabled: BTreeSet<Channel>,
    mask_value: f32,
    camera: OrbitCamera,
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "tool.path",
            "Path",
            ["Tools"],
            Some("7"),
            "{points?: [[x,y],...] viewport pixels on the mesh | uvPoints?: [[u,v],...], viewport?: [w,h], closed?, fill?, commit?: bool, clear?: bool} — selects the Path tool and edits its curve; commit adds the path to the selected path sublayer (or a new one), painted with the current brush or filled when closed and filled. While a stored path is edited (path.edit), changes repaint it at once",
            always,
            path
        ),
        cmd!(
            "path.edit",
            "Edit path",
            ["Tools"],
            None,
            "{effect: path sublayer id, index: path index} — loads a stored path into the Path tool (with its brush and material); changes to it repaint the sublayer live until commit or clear",
            has_layer,
            edit_path
        ),
        cmd!(
            "path.remove",
            "Remove path",
            ["Tools"],
            None,
            "{effect: path sublayer id, index: path index} — removes a stored path and repaints its sublayer",
            has_layer,
            remove_path
        ),
    ]
}

fn path(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "tool.path";
    if s.tool.tool != Tool::Path {
        set_tool(s, Tool::Path)?;
    }
    let viewport = viewport_of(s, p);
    if boolp(p, "clear") == Some(true) {
        s.cancel_path();
    }

    let placed = match (points_param(CMD, p, "points")?, points_param(CMD, p, "uvPoints")?) {
        (Some(pixels), _) => Some((
            pixels
                .iter()
                .enumerate()
                .map(|(i, pixel)| s.pick(*pixel, viewport).map(|hit| hit.pos).ok_or_else(|| bad(CMD, format!("point {i} misses the mesh"))))
                .collect::<Result<Vec<_>>>()?,
            false,
        )),
        (None, Some(uvs)) => Some((uvs.iter().map(|uv| uv.clamp(Vec2::ZERO, Vec2::ONE).extend(0.0)).collect(), true)),
        (None, None) => None,
    };
    if let Some((points, uv)) = placed {
        if points.len() > MAX_PATH_POINTS {
            return Err(bad(CMD, format!("at most {MAX_PATH_POINTS} control points")));
        }
        s.tool.path.points = points;
        s.tool.path.uv = uv;
    }
    if let Some(v) = boolp(p, "closed") {
        s.tool.path.closed = v;
    }
    if let Some(v) = boolp(p, "fill") {
        s.tool.path.fill = v;
    }
    s.revision = s.revision.wrapping_add(1);

    let committed = if boolp(p, "commit") == Some(true) {
        let report = s.commit_path(viewport)?;
        s.update();
        report
    } else {
        if s.path_live()? {
            s.update();
        }
        Value::Null
    };

    let path = &s.tool.path;
    let editing = path.editing.map(|(e, i)| json!({ "effect": e.0, "index": i }));
    Ok(json!({ "points": path.points.len(), "uv": path.uv, "closed": path.closed, "fill": path.fill, "editing": editing, "committed": committed }))
}

/// The `effect` and `index` params naming a stored path, with the path.
fn path_ref(s: &Session, cmd: &str, p: &Value) -> Result<(LayerId, usize, PathShape)> {
    let eid = LayerId(u64p(p, "effect").ok_or_else(|| bad(cmd, "`effect` (a path sublayer id) is required"))?);
    let index = u64p(p, "index").ok_or_else(|| bad(cmd, "`index` is required"))?;
    let shapes = s.paths(eid).ok_or_else(|| bad(cmd, format!("{} is not a path sublayer", eid.0)))?;
    let i =
        usize::try_from(index).ok().filter(|i| *i < shapes.len()).ok_or_else(|| bad(cmd, format!("no path {index} (the sublayer has {})", shapes.len())))?;
    let shape = shapes.get(i).cloned().ok_or_else(|| bad(cmd, format!("no path {index}")))?;
    Ok((eid, i, shape))
}

fn edit_path(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "path.edit";
    let (eid, index, shape) = path_ref(s, CMD, p)?;
    let paint = read_paint(&shape)?;

    s.finish_path()?;
    set_tool(s, Tool::Path)?;
    s.tool.brush = paint.brush;
    s.tool.values = paint.values;
    s.tool.enabled = paint.enabled;
    s.tool.mask_value = paint.mask_value;
    s.tool.path = PathState { editing: Some((eid, index)), ..state_of(&shape) };
    s.path_steps = Default::default();
    s.target = StackTarget::Effect(eid);
    Ok(json!({ "effect": eid.0, "index": index, "points": s.tool.path.points.len() }))
}

fn remove_path(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "path.remove";
    let (eid, index, _) = path_ref(s, CMD, p)?;
    s.edit("Remove path", |s| {
        if let Some(Effect { kind: EffectKind::Path(shapes), .. }) = s.doc.project.effect_mut(eid)
            && index < shapes.len()
        {
            shapes.remove(index);
        }
        s.render_paths(eid)
    })?;
    if s.tool.path.editing.is_some_and(|(e, _)| e == eid) {
        s.stop_path_edit();
    }
    s.update();
    Ok(json!({ "effect": eid.0, "paths": s.paths(eid).map_or(0, <[_]>::len) }))
}

/// A stored path's settings, with the brush clamped like the tool's own (they come from the file).
fn read_paint(shape: &PathShape) -> Result<PathPaint> {
    let mut paint: PathPaint = serde_json::from_value(shape.paint.clone()).map_err(|e| EngineError::Other(format!("unreadable path settings: {e}")))?;
    paint.brush = paint.brush.sanitized();
    Ok(paint)
}

/// The Path tool state showing a stored path (finite points only, at most [`MAX_PATH_POINTS`])
fn state_of(shape: &PathShape) -> PathState {
    let points = shape.points.iter().filter(|p| p.iter().all(|v| v.is_finite())).take(MAX_PATH_POINTS).map(|p| Vec3::from_array(*p)).collect();
    PathState { points, uv: shape.uv, closed: shape.closed, fill: shape.fill, ..PathState::default() }
}

/// Tiles (x, y) where sublayer `eid`'s pixels differ between `before` and `after` (copy-on-write: an unchanged tile is the same allocation)
fn changed_tiles(before: &PixelStore, after: &PixelStore, eid: LayerId) -> BTreeSet<(u32, u32)> {
    let (before_images, after_images) = (sublayer_images(before, eid), sublayer_images(after, eid));
    let channels: BTreeSet<Option<Channel>> = before_images.keys().chain(after_images.keys()).copied().collect();

    let mut tiles = BTreeSet::new();
    for channel in channels {
        let (before_image, after_image) = (before_images.get(&channel), after_images.get(&channel));
        let Some(image) = before_image.or(after_image) else { continue };
        for ty in 0..image.tiles_y() {
            for tx in 0..image.tiles_x() {
                let same = match (before_image.and_then(|i| i.tile_handle(tx, ty)), after_image.and_then(|i| i.tile_handle(tx, ty))) {
                    (None, None) => true,
                    (Some(before_tile), Some(after_tile)) => Arc::ptr_eq(before_tile, after_tile),
                    _ => false,
                };
                if !same {
                    tiles.insert((tx, ty));
                }
            }
        }
    }
    tiles
}

/// Sublayer `eid`'s images by channel (None: its mask image)
fn sublayer_images(pixels: &PixelStore, eid: LayerId) -> BTreeMap<Option<Channel>, &TileImage> {
    let layers = pixels.layers.iter().filter(|((id, _), _)| *id == eid).map(|((_, channel), image)| (Some(*channel), image));
    layers.chain(pixels.masks.get(&eid).map(|mask| (None, mask))).collect()
}

/// The camera and viewport a stored path paints through (the views draw its curve the same way)
fn paint_view(shape: &PathShape, paint: &PathPaint) -> (OrbitCamera, Vec2) {
    (paint.camera, Vec2::from_array(shape.viewport).max(Vec2::splat(MIN_PAINT_VIEWPORT_PX)).min(Vec2::splat(MAX_PAINT_VIEWPORT_PX)))
}

/// NDC (y up) to viewport pixels (origin top-left).
fn to_pixels(ndc: Vec2, viewport: Vec2) -> Vec2 {
    Vec2::new((ndc.x * 0.5 + 0.5) * viewport.x, (0.5 - ndc.y * 0.5) * viewport.y)
}

impl Session {
    /// The path's control points in view space: viewport pixels for a 3D path (None behind the
    /// camera), texture coordinates for a 2D one.
    pub fn path_view_points(&self, viewport: Vec2) -> Vec<Option<Vec2>> {
        let path = &self.tool.path;
        if path.uv {
            return path.points.iter().map(|p| Some(p.truncate())).collect();
        }
        let aspect = viewport.x.max(1.0) / viewport.y.max(1.0);
        path.points.iter().map(|p| self.camera.project(*p, aspect).map(|n| to_pixels(n.truncate(), viewport))).collect()
    }

    /// The stored paths of path sublayer `eid` as Path tool states, for the views to draw
    pub fn stored_path_states(&self, eid: LayerId) -> Vec<PathState> {
        self.paths(eid).unwrap_or_default().iter().map(state_of).collect()
    }

    /// The stored paths of path sublayer `eid` (None if it isn't one).
    pub fn paths(&self, eid: LayerId) -> Option<&[PathShape]> {
        match self.doc.project.effect(eid) {
            Some((_, Effect { kind: EffectKind::Path(shapes), .. })) => Some(shapes),
            _ => None,
        }
    }

    /// The Path tool's curve with the current brush and material, as painted from `camera`.
    fn path_shape(&self, camera: OrbitCamera, viewport: Vec2) -> PathShape {
        let (t, path) = (&self.tool, &self.tool.path);
        let paint = PathPaint { brush: t.brush.clone(), values: t.values.clone(), enabled: t.enabled.clone(), mask_value: t.mask_value, camera };
        PathShape {
            points: path.points.iter().map(|p| p.to_array()).collect(),
            uv: path.uv,
            closed: path.closed,
            fill: path.fill,
            viewport: viewport.to_array(),
            paint: serde_json::to_value(paint).unwrap_or_default(),
        }
    }

    /// Store the Path tool's curve (new, or the path being edited) and repaint its sublayer as one undo step, then clear the tool
    pub fn commit_path(&mut self, viewport: Vec2) -> Result<Value> {
        let shape = self.path_shape(self.camera, viewport);
        if let Some(draft) = self.path_draft.take() {
            match self.tool.path.editing {
                // The preview is what applying paints: keep its pixels
                Some((eid, index)) if self.paths(eid).and_then(|v| v.get(index)) == Some(&shape) => {
                    self.push_step(NEW_PATH_LABEL, draft.base, false);
                    self.target = StackTarget::Effect(eid);
                    self.tool.path.clear();
                    let mut out = draft.painted;
                    out["effect"] = json!(eid.0);
                    out["index"] = json!(index);
                    return Ok(out);
                }
                // Otherwise it paints again from the document before the preview
                _ => {
                    let preview = std::mem::replace(&mut self.doc, draft.base);
                    self.changed_without_history(&preview);
                    self.tool.path.editing = None;
                }
            }
        }

        let editing = self.tool.path.editing;
        let label = editing.map_or(NEW_PATH_LABEL.to_string(), |(_, i)| format!("Edit path {}", i + 1));
        let out = self.edit_merging(&label, editing.is_some() && self.tool.path.edited, |s| {
            let (eid, index) = match editing {
                Some(at) => at,
                None => (s.path_effect()?, usize::MAX),
            };
            s.put_path(eid, index, shape)
        })?;

        if let Some(eid) = out.get("effect").and_then(Value::as_u64) {
            self.target = StackTarget::Effect(LayerId(eid));
        }
        self.tool.path.clear();
        Ok(out)
    }

    /// Repaint the edited path (one merged undo step) or a new path's preview (no step) when its points, brush or material changed; called every frame
    pub fn path_live(&mut self) -> Result<bool> {
        let stored = match self.tool.path.editing {
            Some((eid, index)) => match self.paths(eid).and_then(|v| v.get(index)).cloned() {
                Some(shape) => Some((eid, index, shape)),
                None => {
                    self.stop_path_edit();
                    return Ok(false);
                }
            },
            None => None,
        };

        let path = &self.tool.path;
        if self.tool.tool != Tool::Path || self.stroke.is_some() || path.points.is_empty() || (path.closed && path.fill && path.points.len() < 3) {
            return Ok(false);
        }
        let Some((eid, index, stored)) = stored else { return self.start_path_draft() };

        // Typed comparison: the stored settings went through JSON (f64), maybe a save and load
        let unchanged = read_paint(&stored).is_ok_and(|p| {
            let now = self.path_shape(p.camera, Vec2::from_array(stored.viewport));
            (&now.points, now.uv, now.closed, now.fill) == (&stored.points, stored.uv, stored.closed, stored.fill) && read_paint(&now).is_ok_and(|n| n == p)
        });
        if unchanged {
            return Ok(false);
        }

        let shape = self.path_shape(self.camera, self.viewport);
        let (painted, before) = self.repaint_path(eid, index, shape)?;
        match &mut self.path_draft {
            Some(draft) => draft.painted = painted,
            None => {
                // The path's own repaint keeps its point changes undoable, unlike other steps
                let steps = std::mem::take(&mut self.path_steps);
                self.push_step(&format!("Edit path {}", index + 1), before, self.tool.path.edited);
                self.path_steps = steps;
                self.tool.path.edited = true;
            }
        }
        Ok(true)
    }

    /// Paint a new path's first preview where [`Session::commit_path`] would add it, keeping the document before it for apply and cancel
    fn start_path_draft(&mut self) -> Result<bool> {
        let base = self.doc.clone();
        let shape = self.path_shape(self.camera, self.viewport);
        let placed = self.path_effect().and_then(|eid| {
            let index = self.paths(eid).map_or(0, <[_]>::len);
            self.put_path(eid, index, shape).map(|painted| ((eid, index), painted))
        });
        match placed {
            Ok((at, painted)) => {
                // Placing a path is a new action: what was undone before can't be redone over it
                self.redo.clear();
                self.tool.path.editing = Some(at);
                self.changed_without_history(&base);
                self.path_draft = Some(PathDraft { base, painted });
                Ok(true)
            }
            Err(e) => {
                self.doc = base;
                Err(e)
            }
        }
    }

    /// Remember the curve before a point change from a view, so undo steps back through those changes while the path is edited
    pub fn path_checkpoint(&mut self) {
        if !self.path_steps.0.last().is_some_and(|s| s.same_curve(&self.tool.path)) {
            self.path_steps.0.push(self.tool.path.clone());
            self.path_steps.1.clear();
        }
    }

    /// Whether [`Session::step_path`] would undo (`back`) or redo a point change
    pub(crate) fn path_step_pending(&self, back: bool) -> bool {
        let steps = if back { &self.path_steps.0 } else { &self.path_steps.1 };
        self.tool.tool == Tool::Path && steps.iter().any(|s| !s.same_curve(&self.tool.path))
    }

    /// Undo (`back`) or redo the edited path's last point change, false when there is none; with no points left the path is gone
    pub(crate) fn step_path(&mut self, back: bool) -> bool {
        if self.tool.tool != Tool::Path {
            return false;
        }

        let (from, to) = if back { (&mut self.path_steps.0, &mut self.path_steps.1) } else { (&mut self.path_steps.1, &mut self.path_steps.0) };
        // A checkpoint taken by a press that changed nothing isn't a step
        while from.last().is_some_and(|s| s.same_curve(&self.tool.path)) {
            from.pop();
        }
        let Some(state) = from.pop() else { return false };

        let path = &mut self.tool.path;
        let now = std::mem::replace(path, PathState { editing: path.editing, edited: path.edited, ..state });
        to.push(now);

        if self.tool.path.points.is_empty() {
            self.stop_path_edit();
        }
        true
    }

    /// Apply a new path being placed (as Enter would) and end the edit of a stored one, e.g. before the active texture set changes
    pub fn finish_path(&mut self) -> Result<()> {
        if self.path_draft.is_some() {
            let viewport = self.viewport;
            self.commit_path(viewport)?;
        }
        self.stop_path_edit();
        Ok(())
    }

    /// Cancel the Path tool's path: a new one's preview is removed, a stored one keeps its applied changes
    pub fn cancel_path(&mut self) {
        self.stop_path_edit();
        self.tool.path.clear();
    }

    /// Stop editing a stored path (its changes are already applied) and clear the tool; a new path's preview is removed
    pub fn stop_path_edit(&mut self) {
        if let Some(draft) = self.path_draft.take() {
            let preview = std::mem::replace(&mut self.doc, draft.base);
            self.changed_without_history(&preview);
        }
        if self.tool.path.editing.is_some() {
            self.tool.path.clear();
        }
        self.path_steps = Default::default();
    }

    /// The selected path sublayer, or a new one on the selected layer's content or mask
    /// (whichever is selected).
    fn path_effect(&mut self) -> Result<LayerId> {
        let id = self.selected_layer().ok_or_else(|| EngineError::Other("select a layer to add the path to".into()))?;
        let p = &self.doc.project;
        let mask = match self.target {
            StackTarget::Effect(e) => match p.effect(e) {
                Some((_, x)) if matches!(x.kind, EffectKind::Path(_)) => return Ok(e),
                Some((owner, _)) => !p.layer(owner)?.content_effects.iter().any(|x| x.id == e),
                None => false,
            },
            StackTarget::Mask => true,
            StackTarget::Content => false,
        };
        // An instance's content and (unless it has its own) mask belong to its source.
        let owner = if mask { p.mask_owner(id) } else { p.content_layer(id).map_or(id, |l| l.id) };
        let eid = self.doc.project.alloc_id();
        let l = self.doc.project.layer_mut(owner)?;
        let e = Effect::new(eid, EffectKind::Path(Vec::new()));
        if mask {
            l.mask.get_or_insert_with(|| Mask::new(MaskBackground::Black)).effects.push(e);
        } else if l.is_group() {
            return Err(EngineError::Other(format!("\"{}\" is a folder: add the path to a layer or a mask", l.name)));
        } else {
            l.content_effects.push(e);
        }
        Ok(eid)
    }

    /// [`Session::put_path`] recompositing only the tiles it changed, with no undo step: returns the paint report and the document before
    fn repaint_path(&mut self, eid: LayerId, index: usize, shape: PathShape) -> Result<(Value, Document)> {
        let before = self.doc.clone();
        let painted = match self.put_path(eid, index, shape) {
            Ok(v) => v,
            Err(e) => {
                self.doc = before;
                return Err(e);
            }
        };

        // Paths paint into the active set; a sublayer in another set recomposites in full
        let project = &self.doc.project;
        let set = project.effect(eid).and_then(|(owner, _)| project.locate(owner)).map(|(ts, _, _)| ts);
        if set == Some(self.ts()) {
            // Content or mask, the sublayer's owner decides which sets redraw
            let tiles = changed_tiles(&before.pixels, &self.doc.pixels, eid);
            self.mark_painted(self.ts(), Some(PaintTarget::Layer(eid)), tiles);
        } else {
            self.invalidate_changed(&before);
        }
        self.revision = self.revision.wrapping_add(1);
        self.modified = true;
        Ok((painted, before))
    }

    /// Replace path `index` of sublayer `eid` (append when out of range) and repaint the
    /// sublayer. Returns what painting that path reported, with the sublayer and index.
    fn put_path(&mut self, eid: LayerId, index: usize, shape: PathShape) -> Result<Value> {
        let index = match self.doc.project.effect_mut(eid) {
            Some(Effect { kind: EffectKind::Path(shapes), .. }) => match shapes.get_mut(index) {
                Some(slot) => {
                    *slot = shape;
                    index
                }
                None => {
                    shapes.push(shape);
                    shapes.len() - 1
                }
            },
            _ => return Err(EngineError::Other(format!("{} is not a path sublayer", eid.0))),
        };
        let mut out = self.render_paths(eid)?.get(index).cloned().unwrap_or_else(|| json!({}));
        out["effect"] = json!(eid.0);
        out["index"] = json!(index);
        Ok(out)
    }

    /// Repaint path sublayer `eid` from its stored paths, bottom to top. From [`PathCache`]: the
    /// paths before the first changed one keep their pixels, and an unchanged path above it
    /// whose tiles the change didn't reach keeps its painted tiles; the rest repaint. Runs
    /// inside the caller's undo step. Returns each painted path's report (null for the cached
    /// ones).
    fn render_paths(&mut self, eid: LayerId) -> Result<Vec<Value>> {
        let shapes = self.paths(eid).ok_or_else(|| EngineError::Other(format!("{} is not a path sublayer", eid.0)))?.to_vec();
        let images = &self.images;
        let alpha = |sh: &PathShape| read_paint(sh).ok().and_then(|p| p.brush.alpha_image).and_then(|n| images.get(&n).cloned());
        let new: Vec<Painted> = shapes
            .into_iter()
            .map(|sh| {
                let a = alpha(&sh);
                (sh, a)
            })
            .collect();
        let project = &self.doc.project;
        let owner = project.effect(eid).and_then(|(o, _)| project.layer(o).ok()).map(|l| l.channels.keys().copied().collect()).unwrap_or_default();
        let set = project.texture_set(self.ts()).map(|t| t.channel_list()).unwrap_or_default();
        let key: CacheKey = (self.mesh_id, self.caches.get(self.ts()).map(|c| c.size), owner, set);
        let old = self.path_cache.take().filter(|c| c.eid == eid && c.key == key);
        let (prefix, suffix) = old.as_ref().map_or((0, 0), |c| {
            let p = c.shapes.iter().zip(&new).take_while(|(a, b)| same_path(a, b)).count();
            (p, c.shapes[p..].iter().rev().zip(new[p..].iter().rev()).take_while(|(a, b)| same_path(a, b)).count())
        });
        // Start from the last snapshot kept at or below the first changed path (the first one,
        // empty, is always kept).
        let start = old.as_ref().map_or(0, |c| (0..=prefix).rev().find(|&k| c.snaps[k].is_some()).unwrap_or(0));
        let mut snaps = old.as_ref().map_or_else(|| vec![Some(Snap::new())], |c| c.snaps[..=start].to_vec());
        let mut tiles = old.as_ref().map_or_else(Vec::new, |c| c.tiles[..start].to_vec());
        let mut state = snaps[start].clone().unwrap_or_default();
        // Each path goes through the regular stroke and fill code with its own settings; their
        // own history entries are dropped (the caller records one step).
        let saved = (self.tool.clone(), self.camera, self.target, self.quick_mask.editing);
        let history = (std::mem::take(&mut self.undo), std::mem::take(&mut self.redo));
        self.target = StackTarget::Effect(eid);
        self.quick_mask.editing = false;
        self.tool.tool = Tool::Path;
        self.painting_paths = true;
        let mut out = vec![Value::Null; start];
        let mut result = Ok(());
        for (j, (sh, _)) in new.iter().enumerate().skip(start) {
            // Painting is per tile: over the same tiles, an unchanged path paints the same.
            let o = if j < prefix { Some(j) } else { (j >= new.len() - suffix).then(|| j + old.as_ref().map_or(0, |c| c.shapes.len()) - new.len()) };
            if let (Some(c), Some(o)) = (&old, o)
                && let (Some(below), Some(after)) = (&c.snaps[o], &c.snaps[o + 1])
                && same_tiles(&state, below, &c.tiles[o])
            {
                copy_tiles(&mut state, after, &c.tiles[o]);
                tiles.push(c.tiles[o].clone());
                snaps.push(Some(state.clone()));
                evict(&mut snaps, &tiles, CACHE_BYTES, prefix);
                out.push(Value::Null);
                continue;
            }
            self.put_snap(eid, &state);
            self.path_tiles.clear();
            match self.paint_shape(sh) {
                Ok(v) => out.push(v),
                Err(e) => {
                    result = Err(e);
                    break;
                }
            }
            state = self.take_snap(eid);
            tiles.push(std::mem::take(&mut self.path_tiles));
            snaps.push(Some(state.clone()));
            evict(&mut snaps, &tiles, CACHE_BYTES, prefix);
        }
        self.put_snap(eid, &state);
        self.painting_paths = false;
        (self.tool, self.camera, self.target, self.quick_mask.editing) = saved;
        (self.undo, self.redo) = history;
        if result.is_ok() {
            self.path_cache = Some(PathCache { eid, key, shapes: new, snaps, tiles });
        }
        result.map(|_| out)
    }

    fn take_snap(&self, eid: LayerId) -> Snap {
        let p = &self.doc.pixels;
        let mask = p.masks.get(&eid).map(|m| (None, m.clone()));
        mask.into_iter().chain(p.layers.iter().filter(|((id, _), _)| *id == eid).map(|((_, c), img)| (Some(*c), img.clone()))).collect()
    }

    fn put_snap(&mut self, eid: LayerId, snap: &Snap) {
        let p = &mut self.doc.pixels;
        p.masks.remove(&eid);
        p.layers.retain(|(id, _), _| *id != eid);
        for (k, img) in snap {
            match k {
                None => p.masks.insert(eid, img.clone()),
                Some(c) => p.layers.insert((eid, *c), img.clone()),
            };
        }
    }

    fn paint_shape(&mut self, shape: &PathShape) -> Result<Value> {
        let paint = read_paint(shape)?;
        let (camera, viewport) = paint_view(shape, &paint);
        self.tool.brush = paint.brush;
        self.tool.values = paint.values;
        self.tool.enabled = paint.enabled;
        self.tool.mask_value = paint.mask_value;
        self.camera = camera;
        self.tool.path = state_of(shape);
        self.paint_path(viewport)
    }

    /// Where a 3D path's curve lands on the mesh painted through `camera`, per segment as [`spline_segments`] (None off the mesh)
    pub fn path_surface(&self, points: &[Vec3], closed: bool, camera: OrbitCamera, viewport: Vec2) -> Vec<Vec<Option<Vec3>>> {
        let aspect = viewport.x.max(1.0) / viewport.y.max(1.0);
        let Some(screen) = points.iter().map(|p| camera.project(*p, aspect).map(|n| to_pixels(n.truncate(), viewport))).collect::<Option<Vec<_>>>() else {
            return Vec::new();
        };

        let ctx = PickCtx { mesh: &self.mesh, bvh: &self.bvh, camera: &camera, viewport };
        spline_segments(&screen, closed, SURFACE_STEP_PX)
            .into_iter()
            .zip(&screen)
            .map(|(samples, first)| std::iter::once(*first).chain(samples).map(|q| pick(&ctx, q).map(|hit| hit.pos)).collect())
            .collect()
    }

    /// [`Session::path_surface`] of a stored 3D path through the camera and viewport it paints with
    pub fn stored_path_surface(&self, eid: LayerId, index: usize) -> Option<Vec<Vec<Option<Vec3>>>> {
        let shape = self.paths(eid)?.get(index)?;
        if shape.uv {
            return None;
        }
        let (camera, viewport) = paint_view(shape, &read_paint(shape).ok()?);
        let path = state_of(shape);
        Some(self.path_surface(&path.points, path.closed, camera, viewport))
    }

    /// Paint the Path tool's curve with the current brush (or fill it when closed and filled)
    /// into the paint target.
    fn paint_path(&mut self, viewport: Vec2) -> Result<Value> {
        let (uv, closed) = (self.tool.path.uv, self.tool.path.closed);
        let fill = closed && self.tool.path.fill;
        let pts: Vec<Vec2> = self.path_view_points(viewport).into_iter().flatten().collect();
        if pts.is_empty() {
            return Err(EngineError::Other("the path has no points: click the surface to add some".into()));
        }
        if fill && pts.len() < 3 {
            return Err(EngineError::Other("a filled path needs at least three points".into()));
        }
        let curve = spline(&pts, closed, if uv { STEP_UV } else { STEP_PX });
        Ok(if fill {
            json!({ "filledTiles": self.lasso_fill(curve, uv, viewport)? })
        } else if uv {
            json!({ "tiles": self.stroke_uv_points(&curve)? })
        } else {
            json!({ "stamps": self.stroke_points(&curve, viewport)? })
        })
    }

    /// Fill the inside of `poly` (viewport pixels over the visible surface, or texture coordinates); returns the touched tiles
    fn lasso_fill(&mut self, poly: Vec<Vec2>, uv: bool, viewport: Vec2) -> Result<usize> {
        let ts = self.ts();
        let (gbuf, mut canvas) = self.fill_canvas(ts)?;
        let w = gbuf.width.max(1);

        if uv {
            let lasso = Lasso::new(poly.iter().map(|p| *p * Vec2::new(w as f32, gbuf.height as f32)).collect());
            fill_coverage(&mut canvas, &gbuf, |i, _| lasso.coverage(Vec2::new((i as u32 % w) as f32 + 0.5, (i as u32 / w) as f32 + 0.5)));
        } else {
            let lasso = Lasso::new(poly);
            let view_proj = self.camera.view_proj(viewport.x.max(1.0) / viewport.y.max(1.0));
            let inv = view_proj.inverse();
            let (cull, mesh, bvh) = (self.tool.brush.backface_culling, &self.mesh, &self.bvh);

            // A visible source inside the lasso can paint each symmetry copy
            let (lo, hi) = mesh.bounds();
            let symmetry = self.tool.brush.sanitized().symmetry;
            let symmetry_origin = symmetry.origin((lo + hi) * 0.5);
            let inverses: Vec<_> = symmetry.transforms().iter().map(|m| m.inverse()).collect();

            fill_coverage(&mut canvas, &gbuf, |i, _| {
                let Some(s) = gbuf.surface(mesh, i) else { return 0.0 };
                let mut best = 0.0f32;

                for m in &inverses {
                    let pos = symmetry_origin + *m * (s.pos - symmetry_origin);
                    let c = view_proj * pos.extend(1.0);
                    if c.w <= MIN_PROJECTIVE_W {
                        continue;
                    }
                    let ndc = Vec2::new(c.x, c.y) / c.w;
                    let a = lasso.coverage(to_pixels(ndc, viewport));
                    if a <= best {
                        continue;
                    }

                    // Use the source surface normal; a copy may face away from the camera
                    let o = inv * ndc.extend(0.0).extend(1.0);
                    let origin = o.truncate() / o.w;
                    let to = pos - origin;
                    let d = to.length();
                    let Some(dir) = to.try_normalize() else { continue };
                    let Some(hit) =
                        bvh.closest(origin, dir, d * (1.0 + RAY_HIT_RELATIVE_TOLERANCE)).filter(|hit| hit.t >= d * (1.0 - RAY_HIT_RELATIVE_TOLERANCE))
                    else {
                        continue;
                    };

                    let Some(source) = surface_at(mesh, hit.tri, hit.u, hit.v) else { continue };

                    if cull && source.nrm.dot(dir) >= 0.0 {
                        continue;
                    }

                    best = a;
                }

                best
            });
        }
        let tiles = canvas.dirty.len();
        self.commit_fill("Path fill", ts, &canvas)?;
        Ok(tiles)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use crate::{Session, execute};

    /// The path sublayer's pixels, comparable.
    fn pixels(s: &Session, eid: texturecraft_core::LayerId) -> Vec<String> {
        let mut v: Vec<String> = s.doc.pixels.layers.iter().filter(|((id, _), _)| *id == eid).map(|(k, img)| format!("{k:?}{img:?}")).collect();
        v.sort();
        v
    }

    /// A session with horizontal UV paths at `rows` (one path sublayer) on a 512 texture.
    fn with_paths(rows: &[f32]) -> (Session, texturecraft_core::LayerId) {
        let mut s = Session::new_project("t", texturecraft_geom::primitives::sphere(1.0, 24, 12), "builtin:test", None, 512).unwrap();
        s.update();
        execute(&mut s, "tool.brush", &json!({"size": 6})).unwrap();
        let mut eid = 0;
        for v in rows {
            let r = execute(&mut s, "tool.path", &json!({"uvPoints": [[0.1, v], [0.9, v]], "commit": true})).unwrap();
            eid = r["committed"]["effect"].as_u64().unwrap();
        }
        (s, texturecraft_core::LayerId(eid))
    }

    /// Move path `index` to row `v` (a live edit) and check the result against painting every
    /// path. Returns which paths were repainted.
    fn edit_row(s: &mut Session, eid: texturecraft_core::LayerId, index: usize, v: f32) -> Vec<bool> {
        execute(s, "path.edit", &json!({"effect": eid.0, "index": index})).unwrap();
        let before = s.path_cache.clone();
        execute(s, "tool.path", &json!({"uvPoints": [[0.1, v], [0.5, v], [0.9, v]]})).unwrap();
        let cached = pixels(s, eid);
        s.path_cache = before;
        let painted = s.render_paths(eid).unwrap().iter().map(|r| !r.is_null()).collect();
        assert_eq!(cached, pixels(s, eid));
        s.path_cache = None;
        s.render_paths(eid).unwrap();
        assert_eq!(cached, pixels(s, eid), "same pixels as painting every path");
        assert!(!cached.is_empty());
        s.stop_path_edit();
        painted
    }

    #[test]
    fn cached_repaint_matches_a_full_repaint() {
        // Rows 0.2, 0.45 and 0.8 of 512 texels are in different 64-texel tile rows.
        let (mut s, eid) = with_paths(&[0.2, 0.45, 0.8]);
        assert_eq!(edit_row(&mut s, eid, 1, 0.47), [false, true, false], "only the edited path repaints");
        assert_eq!(edit_row(&mut s, eid, 0, 0.22), [true, false, false], "paths above keep their tiles");
        assert_eq!(edit_row(&mut s, eid, 0, 0.79), [true, false, true], "a path over the change repaints");
    }

    #[test]
    fn removing_a_path_keeps_the_paths_above() {
        let (mut s, eid) = with_paths(&[0.2, 0.45, 0.8]);
        let before = s.path_cache.clone();
        execute(&mut s, "path.remove", &json!({"effect": eid.0, "index": 0})).unwrap();
        let cached = pixels(&s, eid);
        s.path_cache = before;
        assert!(s.render_paths(eid).unwrap().iter().all(Value::is_null), "both paths above came from the cache");
        s.path_cache = None;
        s.render_paths(eid).unwrap();
        assert_eq!(cached, pixels(&s, eid), "same pixels as painting every path");
    }

    #[test]
    fn replacing_the_brush_alpha_image_repaints() {
        let (mut s, eid) = with_paths(&[0.2]);
        let img = |v: f32| std::sync::Arc::new(texturecraft_compose::ImageRes { width: 2, height: 2, rgba: vec![[v; 4]; 4], srgb: false });
        s.images.insert("a".into(), img(1.0));
        execute(&mut s, "tool.brush", &json!({"alpha": {"image": "a"}})).unwrap();
        execute(&mut s, "tool.path", &json!({"uvPoints": [[0.1, 0.8], [0.9, 0.8]], "commit": true})).unwrap();
        s.render_paths(eid).unwrap();
        s.images.insert("a".into(), img(0.5));
        let r = s.render_paths(eid).unwrap();
        assert!(r[0].is_null() && !r[1].is_null(), "only the path using the image repaints: {r:?}");
    }

    #[test]
    fn snapshots_stay_within_the_budget() {
        let (mut s, eid) = with_paths(&[0.2, 0.45, 0.8]);
        let c = s.path_cache.as_mut().unwrap();
        super::evict(&mut c.snaps, &c.tiles, usize::MAX, 0);
        assert_eq!(c.snaps.iter().flatten().count(), 4);
        super::evict(&mut c.snaps, &c.tiles, 0, 0);
        assert!(c.snaps[0].is_some() && c.snaps[3].is_some() && c.snaps[1..3].iter().all(Option::is_none), "only the free ones stay");
        // Appending still paints the new path alone.
        let shape = s.paths(eid).unwrap()[0].clone();
        if let Some(texturecraft_core::Effect { kind: texturecraft_core::EffectKind::Path(v), .. }) = s.doc.project.effect_mut(eid) {
            v.push(shape);
        }
        let r = s.render_paths(eid).unwrap();
        assert_eq!(r.iter().map(|x| !x.is_null()).collect::<Vec<_>>(), [false, false, false, true]);
        // Editing above evicted snapshots repaints from the bottom and matches a full repaint.
        let c = s.path_cache.as_mut().unwrap();
        super::evict(&mut c.snaps, &c.tiles, 0, 0);
        assert_eq!(edit_row(&mut s, eid, 2, 0.81), [true; 4], "the copy of path 1 sits on repainted tiles");
        // That edit kept the snapshot below path 3 within a zero budget: its next frame starts there.
        let c = s.path_cache.as_mut().unwrap();
        super::evict(&mut c.snaps, &c.tiles, 0, 2);
        assert!(c.snaps[2].is_some() && c.snaps[1].is_none());
        assert_eq!(edit_row(&mut s, eid, 2, 0.83), [false, false, true, true], "path 4 lost its snapshot too");
    }

    #[test]
    fn the_quick_mask_does_not_clip_paths() {
        let (mut s, eid) = with_paths(&[0.2, 0.8]);
        let plain = pixels(&s, eid);
        s.quick_mask.enabled = true;
        s.path_cache = None;
        s.render_paths(eid).unwrap();
        assert_eq!(plain, pixels(&s, eid));
    }

    #[test]
    fn owner_channel_change_skips_the_cache() {
        let (mut s, eid) = with_paths(&[0.3, 0.7]);
        s.render_paths(eid).unwrap();
        let owner = s.doc.project.effect(eid).unwrap().0;
        let layer = s.doc.project.layer_mut(owner).unwrap();
        let ch = *layer.channels.keys().next().unwrap();
        layer.channels.remove(&ch);
        let r = s.render_paths(eid).unwrap();
        assert!(!r[0].is_null(), "the bottom path was repainted: {r:?}");
        assert!(!s.doc.pixels.layers.contains_key(&(eid, ch)), "no pixels left in the removed channel");
    }

    /// The same 3D path baked from different view cameras (one with a box between the eye and
    /// the path) paints the same pixels, both as a new path and when a stored path is edited.
    #[test]
    fn baking_a_3d_path_ignores_the_camera() {
        use texturecraft_geom::glam::{Vec2, Vec3};
        use texturecraft_geom::{OrbitCamera, primitives};

        let mut mesh = primitives::plane(2.0, 16);
        let mut cube = primitives::cube(0.4);
        for p in &mut cube.positions {
            *p += Vec3::new(0.0, 0.25, 0.6);
        }
        mesh.append(&cube);
        let cams = [
            OrbitCamera { yaw: std::f32::consts::PI, pitch: 1.0, distance: 4.0, ..OrbitCamera::default() },
            OrbitCamera { yaw: 0.0, pitch: 0.35, distance: 4.0, ..OrbitCamera::default() },
            OrbitCamera { yaw: 0.7, pitch: 0.8, distance: 2.5, ..OrbitCamera::default() },
        ];
        let stroke = vec![Vec3::new(-0.8, 0.0, 0.0), Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.8, 0.0, 0.0)];
        let quad = vec![Vec3::new(-0.6, 0.0, -0.2), Vec3::new(0.6, 0.0, -0.2), Vec3::new(0.6, 0.0, 0.2), Vec3::new(-0.6, 0.0, 0.2)];
        let vp = Vec2::new(512.0, 512.0);
        let session = |fill: bool| {
            let mut s = Session::new_project("t", mesh.clone(), "builtin:test", None, 256).unwrap();
            s.update();
            execute(&mut s, "tool.material", &json!({"channel": "baseColor", "color": [1.0, 0.0, 0.0]})).unwrap();
            execute(&mut s, "tool.brush", &json!({"size": 4, "hardness": 90})).unwrap();
            execute(&mut s, "tool.path", &json!({"closed": fill, "fill": fill})).unwrap();
            s
        };
        let view = |cam: &OrbitCamera| OrbitCamera { scene_radius: 1.5, ..*cam };
        for (points, fill) in [(stroke, false), (quad, true)] {
            // A new path, committed from each view.
            let new: Vec<Vec<String>> = cams
                .iter()
                .map(|cam| {
                    let mut s = session(fill);
                    s.camera = view(cam);
                    s.tool.path.points = points.clone();
                    let r = s.commit_path(vp).unwrap();
                    assert_eq!(s.camera, view(cam), "fill {fill}: baking moved the view");
                    pixels(&s, texturecraft_core::LayerId(r["effect"].as_u64().unwrap()))
                })
                .collect();
            assert!(!new[0].is_empty(), "fill {fill}: the path painted nothing");
            for b in &new[1..] {
                assert!(*b == new[0], "fill {fill}: a new path's pixels depend on the camera");
            }
            // A stored path (drawn from the first view) moved to `points` while seen from each view.
            for cam in &cams {
                let mut s = session(fill);
                s.camera = view(&cams[0]);
                s.tool.path.points = points.iter().map(|p| *p + Vec3::new(0.0, 0.0, 0.3)).collect();
                let eid = s.commit_path(vp).unwrap()["effect"].as_u64().unwrap();
                execute(&mut s, "path.edit", &json!({"effect": eid, "index": 0})).unwrap();
                s.camera = view(cam);
                s.tool.path.points = points.clone();
                assert!(s.path_live().unwrap(), "fill {fill}: the edit repainted");
                assert_eq!(s.camera, view(cam), "fill {fill}: a live edit moved the view");
                let live = pixels(&s, texturecraft_core::LayerId(eid));
                assert!(live == new[0], "fill {fill}: an edited path's live pixels depend on the camera");
                s.commit_path(vp).unwrap();
                assert_eq!(s.camera, view(cam), "fill {fill}: committing an edit moved the view");
                assert!(pixels(&s, texturecraft_core::LayerId(eid)) == new[0], "fill {fill}: an edited path's pixels depend on the camera");
            }
        }
    }
}
