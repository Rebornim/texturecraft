//! The `bake.log` query: the Baking Log's rows for automation

use serde_json::{Value, json};

use crate::commands::{CommandSpec, always, bad};
use crate::{Result, Session, cmd};

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd!(
        "bake.log",
        "Baking log",
        [],
        None,
        "{} → {running, maps: [{textureSet, setName, map, state: \"waiting\" | \"baking\" | \"baked\" | \"failed\" | \"canceled\", reason?}], highPolys: [{textureSet, setName, file, state: \"waiting\" | \"loading\" | \"loaded\" | \"failed\" | \"canceled\", reason?}]}. The latest bake map by map in bake order, and the high poly files each set bakes from, as the Baking Log shows them; `reason` says why a map or file failed. Empty before the first bake",
        always,
        log
    )]
}

fn log(session: &mut Session, _: &Value) -> Result<Value> {
    let log = &session.bake_log;
    let maps = log.maps.iter().map(|row| entry(&row.state, row.texture_set, &row.set_name, "map", json!(row.map.id()))).collect::<Result<Vec<Value>>>()?;
    let highs = log.highs.iter().map(|row| entry(&row.state, row.texture_set, &row.set_name, "file", json!(row.file))).collect::<Result<Vec<Value>>>()?;
    Ok(json!({ "running": log.is_running() || log.is_loading_high_polys(), "maps": maps, "highPolys": highs }))
}

/// One row: its `state` (and `reason`), its texture set, and what it is under `key`
fn entry(state: &impl serde::Serialize, texture_set: usize, set_name: &str, key: &str, what: Value) -> Result<Value> {
    let mut entry = serde_json::to_value(state).map_err(|error| bad("bake.log", error.to_string()))?;
    if let Some(fields) = entry.as_object_mut() {
        fields.insert("textureSet".into(), json!(texture_set));
        fields.insert("setName".into(), json!(set_name));
        fields.insert(key.into(), what);
    }
    Ok(entry)
}
