//! A fill's whole material: set every channel of a fill layer or content fill effect from one built-in material at once

use serde_json::Value;
use texturecraft_core::{Fill, LayerKind};

use crate::commands::{CommandSpec, bad, has_layer, strp};
use crate::layer_cmds::{FillTarget, fill_mut, fill_target};
use crate::materials::{Builder, MATERIALS, SmartMaterial, build, find};
use crate::{EngineError, Result, Session, cmd};

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd!(
        "layer.fill.material",
        "Set fill material",
        [],
        None,
        "{layer? | effect?: content fill effect id, material: built-in material id (not a smart material) | null (back to the channels' defaults)} — sets every channel the material writes, turns those channels on and the others off; the projection stays",
        has_layer,
        set_material
    )]
}

/// The built-in materials a fill can hold whole: the plain ones (smart materials are folders of layers)
pub fn whole_materials() -> impl Iterator<Item = &'static SmartMaterial> {
    MATERIALS.iter().filter(|material| !material.smart)
}

const CMD: &str = "layer.fill.material";

fn set_material(s: &mut Session, p: &Value) -> Result<Value> {
    let target = fill_target(s, p, CMD)?;
    let material = match p.get("material") {
        Some(Value::Null) => None,
        Some(_) => {
            let id = strp(p, "material").ok_or_else(|| bad(CMD, "`material` is a material id or null"))?;
            let found = find(id).ok_or_else(|| bad(CMD, format!("unknown material `{id}`")))?;
            if found.smart {
                return Err(bad(CMD, format!("`{}` is a smart material (a folder of layers): add it from the shelf with material.apply", found.id)));
            }
            Some(found)
        }
        None => return Err(bad(CMD, "`material` is required: a material id or null")),
    };
    let label = material.map_or_else(|| "Clear fill material".to_string(), |found| format!("Set fill material {}", found.label));
    s.edit(&label, |s| {
        let channels = s.doc.project.texture_set(s.ts())?.channel_list();
        let whole = match material {
            Some(found) => Some((found.id, whole_fill(s, found, &channels)?)),
            None => None,
        };
        let f = fill_mut(s, target)?;
        let written: Vec<_> = match whole {
            Some((id, fill)) => {
                f.sources = fill.sources.into_iter().filter(|(channel, _)| channels.contains(channel)).collect();
                f.material = Some(id.to_string());
                f.sources.keys().copied().collect()
            }
            None => {
                f.sources.clear();
                f.material = None;
                return Ok(());
            }
        };
        if let FillTarget::Layer(id) = target {
            let layer = s.doc.project.layer_mut(id)?;
            layer.channels.retain(|channel, _| written.contains(channel));
            for channel in written {
                layer.channels.entry(channel).or_default();
            }
        }
        Ok(())
    })?;
    Ok(Value::Null)
}

/// The one unmasked fill a plain material is made of
fn whole_fill(s: &mut Session, material: &SmartMaterial, channels: &[texturecraft_core::Channel]) -> Result<Fill> {
    let layers = build(&mut Builder { p: &mut s.doc.project, channels: channels.to_vec() }, material.id)?;
    match <[_; 1]>::try_from(layers) {
        Ok([layer]) if layer.mask.is_none() => match layer.kind {
            LayerKind::Fill(fill) => Ok(fill),
            _ => Err(EngineError::Other(format!("material `{}` is not a fill", material.id))),
        },
        _ => Err(EngineError::Other(format!("material `{}` is more than one fill", material.id))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_whole_material_is_one_unmasked_fill() {
        let mut s = Session::new_project("t", texturecraft_geom::primitives::sphere(1.0, 16, 8), "builtin:sphere", None, 128).unwrap();
        let channels = s.doc.project.texture_set(s.ts()).map(|set| set.channel_list()).unwrap_or_default();
        for material in whole_materials() {
            assert!(whole_fill(&mut s, material, &channels).is_ok(), "{}", material.id);
        }
    }
}
