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
        "{} → {running, maps: [{textureSet, setName, map, state: \"waiting\" | \"baking\" | \"baked\" | \"failed\" | \"canceled\", reason?}]}. The latest bake map by map in bake order, as the Baking Log shows it; `reason` says why a map failed. Empty before the first bake",
        always,
        log
    )]
}

fn log(session: &mut Session, _: &Value) -> Result<Value> {
    let maps = session
        .bake_log
        .maps
        .iter()
        .map(|row| {
            let mut entry = serde_json::to_value(&row.state).map_err(|error| bad("bake.log", error.to_string()))?;
            if let Some(fields) = entry.as_object_mut() {
                fields.insert("textureSet".into(), json!(row.texture_set));
                fields.insert("setName".into(), json!(row.set_name));
                fields.insert("map".into(), json!(row.map.id()));
            }
            Ok(entry)
        })
        .collect::<Result<Vec<Value>>>()?;
    Ok(json!({ "running": session.bake_log.is_running(), "maps": maps }))
}
