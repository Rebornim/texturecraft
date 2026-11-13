//! Layer stack commands: add, remove, duplicate, select, rename, visibility, reorder, group,
//! per-channel blending and fill sources.

use serde_json::{Value, json};
use texturecraft_core::{
    BlendMode, Channel, ChannelBlend, Decal, EffectKind, Fill, FillSymmetry, Filtering, ImageInput, Layer, LayerId, LayerKind, Procedural, ProceduralKind,
    Projection, Projector, Sampling, ScaleMode, ShapeCrop, Source, SymmetryKind, UvTransform, UvWrap,
};
use texturecraft_paint::alphas::{self, LibraryAlpha};

use crate::commands::{
    CommandSpec, always, bad, blend_param, boolp, channel_param, color_param, content_param, f32p, has_layer, layer_param, normal_format_param, strp, u64p,
};
use crate::{EngineError, Result, Session, cmd, session::StackTarget};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!("layer.add.paint", "Add paint layer", ["Layers"], None, "{name?}", always, add_paint),
        cmd!("layer.add.fill", "Add fill layer", ["Layers"], None, "{name?, color?, roughness?, metallic?}", always, add_fill),
        cmd!("layer.add.group", "Add folder", ["Layers"], None, "{name?}", always, add_group),
        cmd!("layer.remove", "Remove layer(s)", ["Layers"], Some("Delete"), "{layer?}", has_layer, remove),
        cmd!("layer.duplicate", "Duplicate layer(s)", ["Layers"], Some("Ctrl+D"), "{layer?}", has_layer, duplicate),
        cmd!("layer.group", "Group layer(s)", ["Layers"], Some("Ctrl+G"), "{layer?}", has_layer, group),
        cmd!("layer.select", "Select layer", [], None, "{layer, target?: content|mask|<effect id>}", always, select),
        cmd!("layer.rename", "Rename layer", [], None, "{layer?, name}", has_layer, rename),
        cmd!("layer.visible", "Toggle layer visibility", [], None, "{layer?, visible?}", has_layer, visible),
        cmd!("layer.move", "Move layer", [], None, "{layer?, index?} | {layer?, above?: id|null, inside?: bool} | {layer?, delta: ±n}", has_layer, move_layer),
        cmd!("layer.blend", "Set channel blending", [], None, "{layer?, channel?: id|all, mode?, opacity?: 0..1, enabled?}", has_layer, blend),
        cmd!(
            "layer.fill.set",
            "Set fill source",
            [],
            None,
            "{layer? | effect?: content fill effect id | mask fill effect id (one grayscale source: no channel, enabled, solo or normalFormat), channel, color?|value?|procedural?: kind, scale?, seed?, contrast?, balance?, invert?, low?, high?, resolution?: {relative: -3..3} | {fixed: 128..8192 pow2} (samples across one projection tile, as a texture cooked at that size; relative 0 = the texture set's own, every texel) (the same kind keeps what is not given)|image?|alpha?: library alpha id or name (its picture becomes the image resource alpha_<id>.png), normalFormat?: openGl|directX (the image's normal map convention; alone, it changes the channel's image)|anchor?: anchor effect id, anchorChannel?, enabled?, solo?: true (this channel on, every other one off)} | {effect: filter effect id, input: image input id (distanceMap, customNoise), value?|procedural?|image?|alpha?|anchor? as a mask fill's source, or clear: true (back to its default gray)}",
            has_layer,
            fill_set
        ),
        cmd!(
            "layer.fill.projection",
            "Set fill projection",
            [],
            None,
            "{layer? | effect?: content or mask fill effect id, projection?: uvTile (Fill (Match per UV Tile))|uv|triplanar|planar|spherical|cylindrical|warp (default: the current one; a new warp's frame sits around the mesh, seen from the front), tiling?: number | [u, v] (UV transformations, every projection; uv: same as scale), rotation?: degrees (UV transformations), uvOffset?: [u, v] (UV transformations of a 3D projection), scale?: number | [u, v] (uv: the tiling; 3D projections: one number), offset?: [u, v] (uv) | [x, y, z] (3D projections), projectorRotation?: [x, y, z] degrees, projectorScale?: number | [x, y, z] (3D projection settings), shapeCrop?: cropped|extendsOutside (3D projections), backfaceCulling?: bool, backfaceAngle?: degrees, backfaceHardness?: 0..1 (planar, cylindrical), depthCulling?: bool, depthCullingHardness?: 0..1 (planar, warp; surfaces hidden from the projector behind others get none of it; new planar and warp projections start with it on at 0.75), sharpness?: 1..64 (triplanar), warpCorners?: [[x, y] × 4] (warp: top left, top right, bottom right, bottom left, across and up in frame sizes from its centre; unpulled = ±0.5), projectionDepth?: lengths of the frame's longer side it reaches in front of and behind it when cropped (warp), angle?: 1..360 degrees one repeat spans (cylindrical), axis?: 0|1|2 (planar, cylindrical), filtering?: bilinearHq|bilinearSharp|nearest (how images are read), wrap?: repeat|repeatHorizontally|repeatVertically|none (past the 0..1 tile; not tri-planar), symmetry?: bool (copies of a 3D projection; UV projections have none), symmetryType?: mirror|radial, symmetryAxis?: 0|1|2 (the mirror plane's normal or the turning axis), flipCopy?: [u, v] (turn the copies' images over), axisPosition?: number | [x, y, z] (where the plane or axis passes, -1..1 mesh radii from the mesh's centre), symmetryCount?: 2..64 (radial copies, the original included), angleSpan?: 1..360 degrees (radial copies spread over it; 360 spaces them evenly)} — what is not given keeps its value",
            has_layer,
            fill_projection
        ),
        cmd!("layer.colorTag", "Set layer color tag", [], None, "{layer?, tag?: 0..7 (absent clears)}", has_layer, color_tag),
        cmd!("layer.list", "List layers", [], None, "{ts?}", always, list),
    ]
}

/// Insert a layer above the selection (inside it when it is a group) and select it.
pub(crate) fn insert_selected(s: &mut Session, layer: Layer) -> Result<LayerId> {
    let ts = s.ts();
    let sel = s.selected_layer();
    let inside = sel.and_then(|id| s.doc.project.layer(id).ok()).is_some_and(|l| l.is_group());
    let id = s.doc.project.insert_layer(ts, layer, sel, inside)?;
    s.selected.insert(ts, id);
    s.target = StackTarget::Content;
    Ok(id)
}

fn channels_of(s: &Session) -> Vec<Channel> {
    s.doc.project.texture_set(s.ts()).map(|t| t.channel_list()).unwrap_or_default()
}

fn add_paint(s: &mut Session, p: &Value) -> Result<Value> {
    let ts = s.ts();
    let name = strp(p, "name").map(str::to_string).unwrap_or_else(|| s.doc.project.next_layer_name(ts, "Layer"));
    let id = s.edit("Add paint layer", |s| {
        let id = s.doc.project.alloc_id();
        insert_selected(s, Layer::new(id, name, LayerKind::Paint, &channels_of(s)))
    })?;
    Ok(json!({ "layer": id.0 }))
}

/// A uniform fill from the `color`, `roughness`, `metallic`, `height` and `opacity` params.
pub(crate) fn fill_from(cmd: &str, p: &Value) -> Result<Fill> {
    let mut fill = Fill::default();
    if let Some(c) = color_param(cmd, p, "color")? {
        fill.sources.insert(Channel::BaseColor, Source::Uniform(c));
    }
    for (key, ch) in [("roughness", Channel::Roughness), ("metallic", Channel::Metallic), ("height", Channel::Height), ("opacity", Channel::Opacity)] {
        if let Some(v) = f32p(p, key) {
            fill.sources.insert(ch, Source::Uniform([v; 3]));
        }
    }
    Ok(fill)
}

fn add_fill(s: &mut Session, p: &Value) -> Result<Value> {
    let ts = s.ts();
    let name = strp(p, "name").map(str::to_string).unwrap_or_else(|| s.doc.project.next_layer_name(ts, "Fill layer"));
    let fill = fill_from("layer.add.fill", p)?;
    let id = s.edit("Add fill layer", |s| {
        let id = s.doc.project.alloc_id();
        insert_selected(s, Layer::new(id, name, LayerKind::Fill(fill), &channels_of(s)))
    })?;
    Ok(json!({ "layer": id.0 }))
}

fn add_group(s: &mut Session, p: &Value) -> Result<Value> {
    let ts = s.ts();
    let name = strp(p, "name").map(str::to_string).unwrap_or_else(|| s.doc.project.next_layer_name(ts, "Folder"));
    let id = s.edit("Add folder", |s| {
        let id = s.doc.project.alloc_id();
        let mut l = Layer::new(
            id,
            name,
            LayerKind::Group { children: Vec::new(), collapsed: false, contents: Default::default(), randomized: Vec::new() },
            &channels_of(s),
        );
        for b in l.channels.values_mut() {
            b.mode = BlendMode::Passthrough;
        }
        insert_selected(s, l)
    })?;
    Ok(json!({ "layer": id.0 }))
}

fn remove(s: &mut Session, p: &Value) -> Result<Value> {
    let id = layer_param(s, "layer.remove", p)?;
    let before = s.doc.clone();
    let removed = s.edit("Remove layer", |s| {
        let (ts, parent, i) = s.doc.project.locate(id).ok_or(EngineError::Other("layer not in a stack".into()))?;
        let removed = s.doc.project.remove_layer(id)?;
        // Instances of what was removed keep their look as independent layers.
        crate::instance_cmds::detach_orphans(s, &before)?;
        // Select the layer below (or above) the removed one.
        let list = match parent {
            Some(g) => s.doc.project.layer(g).map(|l| l.children().to_vec()).unwrap_or_default(),
            None => s.doc.project.texture_set(ts).map(|t| t.stack.clone()).unwrap_or_default(),
        };
        let next = list.get(i.saturating_sub(1)).or(list.first()).copied().or(parent);
        match next {
            Some(n) => s.selected.insert(ts, n),
            None => s.selected.remove(&ts),
        };
        Ok(removed)
    })?;
    Ok(json!({ "removed": removed.iter().map(|l| l.0).collect::<Vec<_>>() }))
}

fn duplicate(s: &mut Session, p: &Value) -> Result<Value> {
    let id = layer_param(s, "layer.duplicate", p)?;
    let (ts, _, _) = s.doc.project.locate(id).ok_or(EngineError::Other("layer not in a stack".into()))?;
    let src = s.doc.clone();
    let new = s.edit("Duplicate layer", |s| {
        let new = crate::layer_ops::copy_tree(&src, id, &mut s.doc, ts, Some(id), false, 0)?;
        let name = format!("{} copy", src.project.layer(id)?.name);
        s.doc.project.layer_mut(new)?.name = name;
        Ok(new)
    })?;
    s.selected.insert(ts, new);
    Ok(json!({ "layer": new.0 }))
}

fn group(s: &mut Session, p: &Value) -> Result<Value> {
    let id = layer_param(s, "layer.group", p)?;
    let g = s.edit("Group layers", |s| {
        let (ts, _, _) = s.doc.project.locate(id).ok_or(EngineError::Other("layer not in a stack".into()))?;
        let gid = s.doc.project.alloc_id();
        let name = s.doc.project.next_layer_name(ts, "Folder");
        let channels = s.doc.project.texture_set(ts)?.channel_list();
        let mut gl =
            Layer::new(gid, name, LayerKind::Group { children: Vec::new(), collapsed: false, contents: Default::default(), randomized: Vec::new() }, &channels);
        for b in gl.channels.values_mut() {
            b.mode = BlendMode::Passthrough;
        }
        s.doc.project.insert_layer(ts, gl, Some(id), false)?;
        s.doc.project.move_above(id, Some(gid), true)?;
        s.selected.insert(ts, gid);
        Ok(gid)
    })?;
    Ok(json!({ "layer": g.0 }))
}

fn select(s: &mut Session, p: &Value) -> Result<Value> {
    let id = layer_param(s, "layer.select", p)?;
    let (ts, _, _) = s.doc.project.locate(id).ok_or_else(|| bad("layer.select", "layer not in a stack"))?;

    // Paths paint into the active set
    if ts != s.ts() {
        s.finish_path()?;
    }

    s.active_set = ts;
    s.selected.insert(ts, id);
    s.target = match p.get("target") {
        Some(Value::String(t)) if t == "mask" => {
            if s.doc.project.layer(s.doc.project.mask_owner(id))?.mask.is_none() {
                return Err(bad("layer.select", "the layer has no mask"));
            }
            StackTarget::Mask
        }
        Some(Value::String(t)) if t == "content" => StackTarget::Content,
        Some(v) => match v.as_u64() {
            Some(e) if s.doc.project.effect(LayerId(e)).is_some_and(|(owner, _)| owner == id) => StackTarget::Effect(LayerId(e)),
            _ => return Err(bad("layer.select", "target must be content, mask or an effect id of this layer")),
        },
        None => StackTarget::Content,
    };

    s.revision = s.revision.wrapping_add(1);
    Ok(json!({ "layer": id.0, "textureSet": ts }))
}

fn rename(s: &mut Session, p: &Value) -> Result<Value> {
    let id = layer_param(s, "layer.rename", p)?;
    let name = strp(p, "name").map(str::trim).filter(|n| !n.is_empty()).ok_or_else(|| bad("layer.rename", "`name` is required"))?.to_string();
    if name.chars().count() > 256 {
        return Err(bad("layer.rename", "name is longer than 256 characters"));
    }
    s.edit("Rename layer", |s| {
        s.doc.project.layer_mut(id)?.name = name;
        Ok(())
    })?;
    Ok(Value::Null)
}

fn visible(s: &mut Session, p: &Value) -> Result<Value> {
    let id = layer_param(s, "layer.visible", p)?;
    let v = s.edit("Layer visibility", |s| {
        let l = s.doc.project.layer_mut(id)?;
        l.visible = boolp(p, "visible").unwrap_or(!l.visible);
        Ok(l.visible)
    })?;
    Ok(json!({ "visible": v }))
}

fn move_layer(s: &mut Session, p: &Value) -> Result<Value> {
    let id = layer_param(s, "layer.move", p)?;
    s.edit("Move layer", |s| {
        if let Some(d) = p.get("delta").and_then(Value::as_i64) {
            let (_, _, i) = s.doc.project.locate(id).ok_or(EngineError::Other("layer not in a stack".into()))?;
            let to = (i as i64 + d).max(0) as usize;
            s.doc.project.move_layer(id, to)?;
        } else if let Some(i) = u64p(p, "index") {
            s.doc.project.move_layer(id, usize::try_from(i).unwrap_or(usize::MAX))?;
        } else if p.get("above").is_some() {
            let above = u64p(p, "above").map(LayerId);
            s.doc.project.move_above(id, above, boolp(p, "inside").unwrap_or(false))?;
        } else {
            return Err(bad("layer.move", "give `delta`, `index` or `above`"));
        }
        Ok(())
    })?;
    Ok(Value::Null)
}

fn blend(s: &mut Session, p: &Value) -> Result<Value> {
    let id = content_param(s, "layer.blend", p)?;
    let all = strp(p, "channel") == Some("all");
    let ch = if all { None } else { channel_param("layer.blend", p, "channel")? };
    let mode = blend_param("layer.blend", p, "mode")?;
    let opacity = f32p(p, "opacity").map(|o| o.clamp(0.0, 1.0));
    let enabled = boolp(p, "enabled");
    let ts_channels = channels_of(s);
    s.edit("Layer blending", |s| {
        let l = s.doc.project.layer_mut(id)?;
        let targets: Vec<Channel> = match ch {
            Some(c) => vec![c],
            None => ts_channels.clone(),
        };
        for c in targets {
            match enabled {
                Some(false) => {
                    l.channels.remove(&c);
                    continue;
                }
                Some(true) => {
                    let mode = if l.is_group() { BlendMode::Passthrough } else { BlendMode::Normal };
                    l.channels.entry(c).or_insert(ChannelBlend { mode, opacity: 1.0 });
                }
                None => {}
            }
            if let Some(b) = l.channels.get_mut(&c) {
                if let Some(m) = mode {
                    b.mode = m;
                }
                if let Some(o) = opacity {
                    b.opacity = o;
                }
            }
        }
        Ok(())
    })?;
    Ok(Value::Null)
}

/// Which fill `layer.fill.*` edits: a fill layer's (`layer`), a content fill effect's
/// (`effect`) or a mask fill effect's (`effect`: one grayscale source, no channels or material).
#[derive(Clone, Copy)]
pub(crate) enum FillTarget {
    Layer(LayerId),
    Effect(LayerId),
    Mask(LayerId),
}

pub(crate) fn fill_target(s: &Session, p: &Value, cmd: &str) -> Result<FillTarget> {
    match u64p(p, "effect").map(LayerId) {
        Some(e) => match s.doc.project.effect(e).map(|(_, x)| &x.kind) {
            Some(EffectKind::Material(_)) => Ok(FillTarget::Effect(e)),
            Some(EffectKind::Fill { .. }) => Ok(FillTarget::Mask(e)),
            _ => Err(bad(cmd, format!("{} is not a fill effect", e.0))),
        },
        None => Ok(FillTarget::Layer(content_param(s, cmd, p)?)),
    }
}

/// The material of the fill at `target`; a mask fill has none
pub(crate) fn fill_mut(s: &mut Session, target: FillTarget) -> Result<&mut Fill> {
    match target {
        FillTarget::Layer(id) => match &mut s.doc.project.layer_mut(id)?.kind {
            LayerKind::Fill(f) => Ok(f),
            _ => Err(EngineError::Other("not a fill layer".into())),
        },
        FillTarget::Effect(e) => match s.doc.project.effect_mut(e).map(|x| &mut x.kind) {
            Some(EffectKind::Material(f)) => Ok(f),
            _ => Err(EngineError::Other("not a content fill effect".into())),
        },
        FillTarget::Mask(_) => Err(EngineError::Other("a mask fill has one grayscale source and no channels or material".into())),
    }
}

/// How the fill at `target` is projected and sampled: every fill has both
pub(crate) fn placement_mut(s: &mut Session, target: FillTarget) -> Result<(&mut Projection, &mut Sampling)> {
    if let FillTarget::Mask(e) = target {
        return match s.doc.project.effect_mut(e).map(|x| &mut x.kind) {
            Some(EffectKind::Fill { projection, sampling, .. }) => Ok((projection, sampling)),
            _ => Err(EngineError::Other("not a mask fill effect".into())),
        };
    }
    let fill = fill_mut(s, target)?;
    Ok((&mut fill.projection, &mut fill.sampling))
}

/// The grayscale source of the mask fill effect `effect`
fn mask_source_mut(s: &mut Session, effect: LayerId) -> Result<&mut Source> {
    match s.doc.project.effect_mut(effect).map(|x| &mut x.kind) {
        Some(EffectKind::Fill { source, .. }) => Ok(source),
        _ => Err(EngineError::Other("not a mask fill effect".into())),
    }
}

fn fill_set(s: &mut Session, p: &Value) -> Result<Value> {
    if let Some(input) = strp(p, "input") {
        return input_set(s, p, input);
    }
    let target = fill_target(s, p, "layer.fill.set")?;
    if let FillTarget::Mask(effect) = target {
        return mask_fill_set(s, p, effect);
    }
    let ch = channel_param("layer.fill.set", p, "channel")?.ok_or_else(|| bad("layer.fill.set", "`channel` is required"))?;
    // `solo`: this channel on, every other one off
    let solo = boolp(p, "solo") == Some(true);
    let enabled = if solo { Some(true) } else { boolp(p, "enabled") };
    let normal_format = normal_format_param("layer.fill.set", p)?;
    let low = color_param("layer.fill.set", p, "low")?;
    let high = color_param("layer.fill.set", p, "high")?;
    let color = color_param("layer.fill.set", p, "color")?;
    let value = color_param("layer.fill.set", p, "value")?;
    let mut library_alpha = None;
    let source = if let Some(kind) = strp(p, "procedural") {
        let kind = ProceduralKind::parse(kind).ok_or_else(|| bad("layer.fill.set", format!("unknown procedural `{kind}`")))?;
        let current = fill_mut(s, target)?.sources.get(&ch).cloned();
        Some(procedural_source(p, kind, current, low, high)?)
    } else if let Some(c) = color.or(value) {
        Some(Source::Uniform(c))
    } else if let Some(name) = strp(p, "image") {
        Some(Source::Image { name: name.to_string(), normal_format: normal_format.unwrap_or_default() })
    } else if let Some(name) = strp(p, "alpha") {
        let alpha = alphas::find(name).ok_or_else(|| bad("layer.fill.set", format!("unknown alpha `{name}`")))?;
        library_alpha = Some(alpha);
        Some(Source::Image { name: crate::fill_alpha::resource_name(alpha), normal_format: normal_format.unwrap_or_default() })
    } else if let Some(a) = u64p(p, "anchor") {
        let reader = match target {
            FillTarget::Layer(id) => Some(id),
            FillTarget::Effect(e) | FillTarget::Mask(e) => s.doc.project.effect(e).map(|(owner, _)| owner),
        };
        let reader = reader.ok_or_else(|| bad("layer.fill.set", "the fill is not in a layer"))?;
        let anchor_channel = channel_param("layer.fill.set", p, "anchorChannel")?.unwrap_or(ch);
        Some(crate::mask_cmds::anchor_source(s, "layer.fill.set", reader, LayerId(a), anchor_channel)?)
    } else if enabled.is_some() || normal_format.is_some() {
        None
    } else {
        return Err(bad("layer.fill.set", "give `color`, `value`, `procedural`, `image`, `alpha`, `anchor`, `normalFormat`, `enabled` or `solo`"));
    };
    if normal_format.is_some() && source.as_ref().is_some_and(|src| !matches!(src, Source::Image { .. })) {
        return Err(bad("layer.fill.set", "`normalFormat` goes with `image` or alone"));
    }
    // A format-only edit leaves the channel on or off
    let touches_channel = source.is_some() || enabled.is_some();
    s.edit("Set fill", |s| {
        if let Some(alpha) = library_alpha {
            s.add_library_alpha(alpha)?;
        }
        let f = fill_mut(s, target)?;
        // `normalFormat` alone: the convention of the channel's image
        if let (None, Some(nf)) = (&source, normal_format) {
            match f.sources.get_mut(&ch) {
                Some(Source::Image { normal_format, .. }) => *normal_format = nf,
                _ => return Err(bad("layer.fill.set", "`normalFormat` needs an image on the channel")),
            }
        }
        // A channel set on its own: the sources are no longer one whole material
        if source.is_some() {
            f.material = None;
        }
        match target {
            FillTarget::Mask(_) => return Err(bad("layer.fill.set", "a mask fill has no channels")),
            // A fill effect's channel is on while it has a source
            FillTarget::Effect(_) if enabled == Some(false) => {
                f.sources.remove(&ch);
            }
            FillTarget::Effect(_) => {
                let src = source.unwrap_or_else(|| f.source(ch));
                f.sources.insert(ch, src);
                if solo {
                    f.sources.retain(|c, _| *c == ch);
                }
            }
            FillTarget::Layer(id) => {
                if let Some(src) = source {
                    f.sources.insert(ch, src);
                }
                let l = s.doc.project.layer_mut(id)?;
                if enabled == Some(false) {
                    l.channels.remove(&ch);
                } else if touches_channel {
                    l.channels.entry(ch).or_default();
                }
                if solo {
                    l.channels.retain(|c, _| *c == ch);
                }
            }
        }
        Ok(())
    })?;
    Ok(Value::Null)
}

/// `layer.fill.set` on mask fill effect `effect`: its one grayscale source (no `channel`, `enabled`, `solo` or `normalFormat`)
fn mask_fill_set(s: &mut Session, p: &Value, effect: LayerId) -> Result<Value> {
    if ["channel", "enabled", "solo", "normalFormat"].iter().any(|key| p.get(key).is_some()) {
        return Err(bad(FILL_SET, "a mask fill has one grayscale source: leave out `channel`, `enabled`, `solo` and `normalFormat`"));
    }
    let current = mask_source_mut(s, effect)?.clone();
    let (source, library_alpha) = gray_source(s, p, effect, current)?;
    s.edit("Set fill", |s| {
        if let Some(alpha) = library_alpha {
            s.add_library_alpha(alpha)?;
        }
        *mask_source_mut(s, effect)? = source;
        Ok(())
    })?;
    Ok(Value::Null)
}

/// `layer.fill.set {effect, input}`: the source of a filter effect's image input `id`, or its default gray back (`clear`)
fn input_set(s: &mut Session, p: &Value, id: &str) -> Result<Value> {
    let effect = u64p(p, "effect").map(LayerId).ok_or_else(|| bad(FILL_SET, "`input` goes with a filter `effect`"))?;
    let found = s.doc.project.effect(effect).map(|(_, found)| found).ok_or_else(|| bad(FILL_SET, format!("no effect {}", effect.0)))?;
    let EffectKind::Filter(filter) = &found.kind else { return Err(bad(FILL_SET, format!("{} is not a filter effect", effect.0))) };
    let input = ImageInput::named(filter, id).ok_or_else(|| bad(FILL_SET, format!("the {} filter has no image input `{id}`", filter.label())))?;
    if ["channel", "enabled", "solo", "normalFormat"].iter().any(|key| p.get(key).is_some()) {
        return Err(bad(FILL_SET, "an image input is one grayscale source: leave out `channel`, `enabled`, `solo` and `normalFormat`"));
    }
    let (source, library_alpha) = if p.get("clear").and_then(Value::as_bool) == Some(true) {
        (None, None)
    } else {
        let (source, alpha) = gray_source(s, p, effect, found.input(&input))?;
        (Some(source), alpha)
    };
    s.edit("Set image input", |s| {
        if let Some(alpha) = library_alpha {
            s.add_library_alpha(alpha)?;
        }
        let found = s.doc.project.effect_mut(effect).ok_or_else(|| bad(FILL_SET, "the effect is gone"))?;
        // Its default gray is not stored, so a project that never set the input saves as before
        match source.filter(|source| *source != input.default_source()) {
            Some(source) => found.inputs.insert(input.id.to_string(), source),
            None => found.inputs.remove(input.id),
        };
        Ok(())
    })?;
    Ok(Value::Null)
}

/// The grayscale source `p` asks for in place of `current` (read by a mask or filter effect `effect`), and the library alpha it needs added
fn gray_source(s: &Session, p: &Value, effect: LayerId, current: Source) -> Result<(Source, Option<&'static LibraryAlpha>)> {
    let low = color_param(FILL_SET, p, "low")?;
    let high = color_param(FILL_SET, p, "high")?;
    let color = color_param(FILL_SET, p, "color")?;
    let value = color_param(FILL_SET, p, "value")?;
    if let Some(kind) = strp(p, "procedural") {
        let kind = ProceduralKind::parse(kind).ok_or_else(|| bad(FILL_SET, format!("unknown procedural `{kind}`")))?;
        Ok((procedural_source(p, kind, Some(current), low, high)?, None))
    } else if let Some(c) = color.or(value) {
        Ok((Source::Uniform(c.map(|gray| gray.clamp(0.0, 1.0))), None))
    } else if let Some(name) = strp(p, "image") {
        Ok((Source::image(name.to_string()), None))
    } else if let Some(name) = strp(p, "alpha") {
        let alpha = alphas::find(name).ok_or_else(|| bad(FILL_SET, format!("unknown alpha `{name}`")))?;
        Ok((Source::image(crate::fill_alpha::resource_name(alpha)), Some(alpha)))
    } else if let Some(anchor) = u64p(p, "anchor") {
        let reader = s.doc.project.effect(effect).map(|(owner, _)| owner).ok_or_else(|| bad(FILL_SET, "the effect is not in a layer"))?;
        // A gray reads one channel of the anchor's stack: the one it read, else Height
        let kept = match current {
            Source::Anchor { channel, .. } => channel,
            _ => Channel::Height,
        };
        let channel = channel_param(FILL_SET, p, "anchorChannel")?.unwrap_or(kept);
        Ok((crate::mask_cmds::anchor_source(s, FILL_SET, reader, LayerId(anchor), channel)?, None))
    } else {
        Err(bad(FILL_SET, "give `value`, `procedural`, `image`, `alpha` or `anchor`"))
    }
}

/// The command [`fill_set`] runs as
const FILL_SET: &str = "layer.fill.set";

/// The procedural source `p` asks for, with its already parsed `low`/`high` colours: the channel's pattern keeps what `p` does not give; another kind starts from its defaults
fn procedural_source(p: &Value, kind: ProceduralKind, current: Option<Source>, new_low: Option<[f32; 3]>, new_high: Option<[f32; 3]>) -> Result<Source> {
    let (mut pattern, low, high) = match current {
        Some(Source::Procedural { pattern, low, high }) if pattern.kind == kind => (pattern, low, high),
        _ => (Procedural::new(kind), [0.0; 3], [1.0; 3]),
    };
    if let Some(scale) = f32p(p, "scale") {
        pattern.scale = scale.clamp(0.001, 1000.0);
    }
    if let Some(seed) = u64p(p, "seed") {
        pattern.seed = seed as u32;
    }
    if let Some(contrast) = f32p(p, "contrast") {
        pattern.contrast = contrast.clamp(0.0, 20.0);
    }
    if let Some(balance) = f32p(p, "balance") {
        pattern.balance = balance.clamp(-1.0, 1.0);
    }
    if let Some(invert) = boolp(p, "invert") {
        pattern.invert = invert;
    }
    // `{relative}` or `{fixed}` as `material.link.resolution`; the texture set's own size is stored as none
    if let Some(size) = p.get("resolution") {
        pattern.resolution = crate::link_cmds::picked_resolution(size)
            .ok_or_else(|| bad("layer.fill.set", "`resolution` takes `{relative: -3..3}` or `{fixed: 128..8192, a power of two}`"))?;
    }
    Ok(Source::Procedural { pattern, low: new_low.unwrap_or(low), high: new_high.unwrap_or(high) })
}

/// A new fill's projection of kind `id` (`uv`, `uvTile`, `triplanar`, `planar`, `spherical`, `cylindrical`, or a `warp` from frame `fit`)
pub(crate) fn new_projection(id: &str, fit: impl FnOnce() -> Decal) -> Option<Projection> {
    match id {
        "warp" => Some(Projection::warp(fit())),
        _ => new_unframed_projection(id),
    }
}

/// A new fill's projection of kind `id`, any kind but a warp (which needs a frame)
pub(crate) fn new_unframed_projection(id: &str) -> Option<Projection> {
    let projection = match id {
        "uv" => Projection::default(),
        "uvTile" => Projection::UvTile,
        "triplanar" | "tri-planar" => {
            Projection::Triplanar { scale: 1.0, sharpness: 8.0, offset: [0.0; 3], uv: Default::default(), projector: Default::default() }
        }
        "planar" => Projection::Planar { scale: 1.0, axis: 2, offset: [0.0; 3], uv: Default::default(), projector: Projector::depth_culled(), placed: None },
        "spherical" => Projection::Spherical { scale: 1.0, uv: Default::default(), offset: [0.0; 3], projector: Default::default() },
        "cylindrical" => Projection::Cylindrical { scale: 1.0, axis: 1, uv: Default::default(), offset: [0.0; 3], projector: Default::default(), angle: 360.0 },
        _ => return None,
    };
    Some(projection)
}

/// The projection `p` asks for: fields it does not give keep their values from `current` when that is of the same kind (or `p` names no kind), and the kind's defaults otherwise; a new warp starts from frame `fit`
pub(crate) fn projection_from(p: &Value, cmd: &str, current: Option<&Projection>, fit: impl FnOnce() -> Decal) -> Result<Projection> {
    let named = match strp(p, "projection") {
        None => None,
        Some("warp") if matches!(current, Some(Projection::Warp { .. })) => current.cloned(),
        Some(id) => Some(new_projection(id, fit).ok_or_else(|| bad(cmd, format!("unknown projection `{id}`")))?),
    };
    let mut proj = match (named, current) {
        (Some(n), Some(c)) if std::mem::discriminant(&n) == std::mem::discriminant(c) => c.clone(),
        (Some(n), _) => n,
        (None, c) => c.cloned().unwrap_or_default(),
    };
    let scale = floats::<2>(p, "scale", 0.001, 1000.0);
    let tiling = floats::<2>(p, "tiling", 0.001, 1000.0);
    let rotation = f32p(p, "rotation").map(|r| r.rem_euclid(360.0));
    let axis = u64p(p, "axis").map(|a| a.min(2) as u8);
    // The UV transformations of a 3D projection: tiling, rotation and an offset across the surface
    let place = |uv: &mut UvTransform| {
        uv.tiling = tiling.unwrap_or(uv.tiling);
        uv.rotation = rotation.unwrap_or(uv.rotation);
        uv.offset = floats(p, "uvOffset", -1000.0, 1000.0).unwrap_or(uv.offset);
    };
    // The 3D projection settings: the projector's offset, turn and per-axis scale
    let offset3 = floats::<3>(p, "offset", -1000.0, 1000.0);
    let crop = match strp(p, "shapeCrop") {
        None => None,
        Some(id) => Some(ShapeCrop::from_id(id).ok_or_else(|| bad(cmd, format!("unknown shapeCrop `{id}`: cropped or extendsOutside")))?),
    };
    let turn = |pr: &mut Projector| {
        pr.rotation = floats(p, "projectorRotation", -360.0, 360.0).unwrap_or(pr.rotation);
        pr.scale = floats(p, "projectorScale", 0.001, 1000.0).unwrap_or(pr.scale);
        pr.crop = crop.unwrap_or(pr.crop);
        pr.backface.on = p.get("backfaceCulling").and_then(Value::as_bool).unwrap_or(pr.backface.on);
        pr.backface.angle = f32p(p, "backfaceAngle").map_or(pr.backface.angle, |v| v.clamp(0.0, 180.0));
        pr.backface.hardness = f32p(p, "backfaceHardness").map_or(pr.backface.hardness, |v| v.clamp(0.0, 1.0));
        pr.depth.on = p.get("depthCulling").and_then(Value::as_bool).unwrap_or(pr.depth.on);
        pr.depth.hardness = f32p(p, "depthCullingHardness").map_or(pr.depth.hardness, |v| v.clamp(0.0, 1.0));
    };
    match &mut proj {
        Projection::Uv { scale: s, rotation: r, offset } => {
            // A UV projection's scale is its tiling
            *s = tiling.or(scale).unwrap_or(*s);
            *r = rotation.unwrap_or(*r);
            *offset = floats(p, "offset", -1000.0, 1000.0).unwrap_or(*offset);
        }
        Projection::UvTile => {}
        Projection::Triplanar { scale: s, sharpness, offset, uv, projector } => {
            *s = scale.map_or(*s, |v| v[0]);
            *sharpness = f32p(p, "sharpness").map_or(*sharpness, |v| v.clamp(1.0, 64.0));
            *offset = offset3.unwrap_or(*offset);
            place(uv);
            turn(projector);
        }
        Projection::Planar { scale: s, axis: a, offset, uv, projector, .. } => {
            *s = scale.map_or(*s, |v| v[0]);
            *a = axis.unwrap_or(*a);
            *offset = offset3.unwrap_or(*offset);
            place(uv);
            turn(projector);
        }
        Projection::Spherical { scale: s, uv, offset, projector } => {
            *s = scale.map_or(*s, |v| v[0]);
            *offset = offset3.unwrap_or(*offset);
            place(uv);
            turn(projector);
        }
        Projection::Cylindrical { scale: s, axis: a, uv, offset, projector, angle } => {
            *s = scale.map_or(*s, |v| v[0]);
            *angle = f32p(p, "angle").map_or(*angle, |v| v.clamp(1.0, 360.0));
            *a = axis.unwrap_or(*a);
            *offset = offset3.unwrap_or(*offset);
            place(uv);
            turn(projector);
        }
        Projection::Warp { corners, depth, uv, projector, .. } => {
            *corners = warp_corners(p).unwrap_or(*corners);
            *depth = f32p(p, "projectionDepth").map_or(*depth, |v| v.clamp(0.0, MAX_PROJECTION_DEPTH));
            place(uv);
            turn(projector);
        }
    }
    Ok(proj)
}

/// The `warpCorners` param: four [across, up] corners in frame sizes from the frame's centre, finite and within reach of the frame
fn warp_corners(p: &Value) -> Option<[[f32; 2]; 4]> {
    let list = p.get("warpCorners")?.as_array().filter(|list| list.len() == 4)?;
    let mut corners = Projection::FRAME_CORNERS;
    for (corner, value) in corners.iter_mut().zip(list) {
        let pair = value.as_array().filter(|pair| pair.len() == 2)?;
        for (axis, v) in corner.iter_mut().zip(pair) {
            *axis = v.as_f64().map(|v| v as f32).filter(|v| v.is_finite())?.clamp(-MAX_CORNER_REACH, MAX_CORNER_REACH);
        }
    }
    Some(corners)
}

/// How far a warp's corner may be pulled from the frame's centre, in frame sizes
const MAX_CORNER_REACH: f32 = 100.0;

/// The largest Projection Depth, in lengths of the frame's longer side
const MAX_PROJECTION_DEPTH: f32 = 100.0;

/// `current` with the `filtering`, `wrap`, `scaleMode` and symmetry params applied
pub(crate) fn sampling_from(p: &Value, cmd: &str, current: Sampling) -> Result<Sampling> {
    let mut sampling = current;
    if let Some(id) = strp(p, "filtering") {
        sampling.filtering = Filtering::from_id(id).ok_or_else(|| bad(cmd, format!("unknown filtering `{id}`: bilinearHq, bilinearSharp or nearest")))?;
    }
    if let Some(id) = strp(p, "wrap") {
        sampling.wrap = UvWrap::from_id(id).ok_or_else(|| bad(cmd, format!("unknown wrap `{id}`: repeat, repeatHorizontally, repeatVertically or none")))?;
    }
    if let Some(id) = strp(p, "scaleMode") {
        sampling.scale = ScaleMode::from_id(id).ok_or_else(|| bad(cmd, format!("unknown scaleMode `{id}`: tiling or physicalSize")))?;
    }
    sampling.symmetry = symmetry_from(p, cmd, sampling.symmetry)?;
    Ok(sampling)
}

/// `current` with the `symmetry`, `symmetryType`, `symmetryAxis`, `flipCopy`, `axisPosition`, `symmetryCount` and `angleSpan` params applied
fn symmetry_from(p: &Value, cmd: &str, current: FillSymmetry) -> Result<FillSymmetry> {
    let mut symmetry = current;
    symmetry.on = boolp(p, "symmetry").unwrap_or(symmetry.on);
    if let Some(id) = strp(p, "symmetryType") {
        symmetry.kind = SymmetryKind::from_id(id).ok_or_else(|| bad(cmd, format!("unknown symmetryType `{id}`: mirror or radial")))?;
    }
    if let Some(axis) = u64p(p, "symmetryAxis") {
        symmetry.axis = u8::try_from(axis).ok().filter(|axis| *axis <= 2).ok_or_else(|| bad(cmd, "symmetryAxis is 0 (X), 1 (Y) or 2 (Z)"))?;
    }
    if let Some(flips) = p.get("flipCopy") {
        let [across, down] = flips.as_array().map(Vec::as_slice).unwrap_or_default() else {
            return Err(bad(cmd, "flipCopy is [u, v], two booleans"));
        };

        symmetry.flip = [across, down].map(|flip| flip.as_bool().unwrap_or(false));
    }
    let reach = FillSymmetry::REACH;
    symmetry.position = floats(p, "axisPosition", -reach, reach).unwrap_or(symmetry.position);
    if let Some(count) = u64p(p, "symmetryCount") {
        let (fewest, most) = (u64::from(*FillSymmetry::COUNTS.start()), u64::from(*FillSymmetry::COUNTS.end()));
        symmetry.count = u32::try_from(count.clamp(fewest, most)).unwrap_or(*FillSymmetry::COUNTS.start());
    }
    symmetry.span = f32p(p, "angleSpan").map_or(symmetry.span, |span| span.clamp(MIN_ANGLE_SPAN, 360.0));
    Ok(symmetry)
}

/// The narrowest a radial symmetry's copies may spread, in degrees
const MIN_ANGLE_SPAN: f32 = 1.0;

/// `key` as `N` finite numbers clamped to `lo..=hi`: an array of `N`, or one number for all.
fn floats<const N: usize>(p: &Value, key: &str, lo: f32, hi: f32) -> Option<[f32; N]> {
    let one = |x: &Value| x.as_f64().map(|f| f as f32).filter(|f| f.is_finite()).map(|f| f.clamp(lo, hi));
    match p.get(key)? {
        Value::Array(a) if a.len() == N => {
            let mut out = [0.0; N];
            for (o, x) in out.iter_mut().zip(a) {
                *o = one(x)?;
            }
            Some(out)
        }
        v => one(v).map(|f| [f; N]),
    }
}

fn fill_projection(s: &mut Session, p: &Value) -> Result<Value> {
    let target = fill_target(s, p, "layer.fill.projection")?;
    let mesh = s.mesh.clone();
    s.edit("Fill projection", |s| {
        let (projection, sampling) = placement_mut(s, target)?;
        *projection = projection_from(p, "layer.fill.projection", Some(projection), || crate::warp_cmds::fitted_frame(&mesh))?;
        *sampling = sampling_from(p, "layer.fill.projection", *sampling)?;
        Ok(())
    })?;
    Ok(Value::Null)
}

fn color_tag(s: &mut Session, p: &Value) -> Result<Value> {
    let id = layer_param(s, "layer.colorTag", p)?;
    let tag = u64p(p, "tag").map(|t| t.min(7) as u8);
    s.edit("Layer color tag", |s| {
        s.doc.project.layer_mut(id)?.color_tag = tag;
        Ok(())
    })?;
    Ok(Value::Null)
}

fn list(s: &mut Session, p: &Value) -> Result<Value> {
    let ts = u64p(p, "ts").map(|t| t as usize).unwrap_or(s.ts());
    Ok(crate::inspect::stack(s, ts))
}
