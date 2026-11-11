//! Sharing bake settings between texture sets: live sync groups, and one-off copies from the bake menus

use serde_json::{Value, json};
use texturecraft_core::{BakeSettings, BakeSyncGroup, BakedMap};

use super::mesh_map_command::sets_param;
use crate::commands::{CommandSpec, always, bad, strp, u64p};
use crate::{Result, Session, cmd};

pub(super) fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "bake.sync",
            "Sync bake settings",
            [],
            None,
            "{group: \"common\"|map id, ts: index|[indices]|\"all\"} → {group, ts}; exactly these texture sets share the group's settings from now on, and the ones added take the settings the group shares",
            always,
            sync
        ),
        cmd!(
            "bake.copy",
            "Copy bake settings",
            [],
            None,
            "{what: \"settings\"|\"checked\"|\"check\", map?: id, from?: index (default: the active set), ts: index|[indices]|\"all\"} → {ts}; settings: every setting, or with map that map's settings; checked: which maps are checked; check: check map. Sets sharing a group with a target get the same change",
            always,
            copy
        ),
    ]
}

fn group_param(params: &Value, command: &str) -> Result<BakeSyncGroup> {
    match strp(params, "group") {
        Some("common") => Ok(BakeSyncGroup::Common),
        Some(id) => BakedMap::parse(id).map(BakeSyncGroup::Map).ok_or_else(|| bad(command, format!("unknown group {id}"))),
        None => Err(bad(command, "`group` must be \"common\" or a mesh map id")),
    }
}

fn group_id(group: BakeSyncGroup) -> &'static str {
    match group {
        BakeSyncGroup::Common => "common",
        BakeSyncGroup::Map(map) => map.id(),
    }
}

fn sync(session: &mut Session, params: &Value) -> Result<Value> {
    const COMMAND: &str = "bake.sync";
    let group = group_param(params, COMMAND)?;
    let members = sets_param(session, COMMAND, params)?;
    let sets = &session.doc.project.texture_sets;

    // Additions take the settings of a set that already shares them; with none left, of the active set when it joins, else of the first
    let staying = members.iter().copied().find(|index| sets.get(*index).is_some_and(|set| set.bake.sync.shares(group)));
    let active = session.ts();
    let source = staying.or_else(|| members.contains(&active).then_some(active)).or_else(|| members.first().copied());
    let source = source.and_then(|index| sets.get(index)).map(|set| set.bake.clone());

    let next: Vec<(usize, BakeSettings)> = sets
        .iter()
        .enumerate()
        .map(|(index, set)| {
            let mut settings = set.bake.clone();
            let joins = members.contains(&index);
            if let Some(source) = source.as_ref().filter(|_| joins && !settings.sync.shares(group)) {
                settings.copy_group(source, group);
            }
            settings.sync.set_shares(group, joins);
            (index, settings)
        })
        .collect();
    store(session, "Sync bake settings", next)?;
    Ok(json!({ "group": group_id(group), "ts": members }))
}

fn copy(session: &mut Session, params: &Value) -> Result<Value> {
    const COMMAND: &str = "bake.copy";
    let map = strp(params, "map").map(|id| BakedMap::parse(id).ok_or_else(|| bad(COMMAND, format!("unknown mesh map {id}")))).transpose()?;
    let from = match u64p(params, "from") {
        Some(index) => usize::try_from(index).map_err(|_| bad(COMMAND, "invalid `from`"))?,
        None => session.ts(),
    };
    let source = session.doc.project.texture_set(from)?.bake.clone();
    let targets = sets_param(session, COMMAND, params)?;
    let maps: Vec<BakedMap> = map.map_or_else(|| BakedMap::ALL.to_vec(), |map| vec![map]);

    let mut next: Vec<BakeSettings> = session.doc.project.texture_sets.iter().map(|set| set.bake.clone()).collect();
    let project = &session.doc.project;
    // Every target together with the sets sharing `group` with it, so a sync group never drifts apart
    let reached = |group: BakeSyncGroup| -> Vec<usize> {
        let mut reached: Vec<usize> = targets.iter().flat_map(|target| project.bake_synced_sets(*target, group)).collect();
        reached.sort_unstable();
        reached.dedup();
        reached
    };
    match strp(params, "what") {
        Some("settings") => {
            let groups = map.map_or_else(
                || std::iter::once(BakeSyncGroup::Common).chain(maps.iter().map(|map| BakeSyncGroup::Map(*map))).collect(),
                |map| vec![BakeSyncGroup::Map(map)],
            );
            for group in groups {
                for index in reached(group) {
                    if let Some(settings) = next.get_mut(index) {
                        settings.copy_group(&source, group);
                    }
                }
            }
            if map.is_none() {
                for (_, settings) in next.iter_mut().enumerate().filter(|(index, _)| targets.contains(index)) {
                    for map in BakedMap::ALL {
                        settings.maps.set_enabled(map, source.maps.is_enabled(map));
                    }
                }
            }
        }
        Some(what @ ("checked" | "check")) => {
            if what == "check" && map.is_none() {
                return Err(bad(COMMAND, "`check` needs a `map`"));
            }
            for (_, settings) in next.iter_mut().enumerate().filter(|(index, _)| targets.contains(index)) {
                for map in &maps {
                    settings.maps.set_enabled(*map, what == "check" || source.maps.is_enabled(*map));
                }
            }
        }
        _ => return Err(bad(COMMAND, "`what` must be settings, checked or check")),
    }

    store(session, "Copy bake settings", next.into_iter().enumerate().collect())?;
    Ok(json!({ "ts": targets }))
}

/// Store `next` as one undoable edit, when anything changed
fn store(session: &mut Session, label: &str, next: Vec<(usize, BakeSettings)>) -> Result<()> {
    let changed =
        |session: &Session| next.iter().any(|(index, settings)| session.doc.project.texture_sets.get(*index).is_some_and(|set| set.bake != *settings));
    if !changed(session) {
        return Ok(());
    }

    session.edit(label, |session| {
        for (index, settings) in next {
            session.doc.project.texture_set_mut(index)?.bake = settings;
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{execute, sample_session};
    use texturecraft_core::BakeSettings;

    fn session() -> Session {
        let session = sample_session(128, 32).unwrap();
        assert_eq!(session.set_count(), 2);
        session
    }

    fn bake(session: &Session, index: usize) -> &BakeSettings {
        &session.doc.project.texture_sets[index].bake
    }

    #[test]
    fn projects_saved_before_sync_share_everything() {
        let settings: BakeSettings = serde_json::from_value(json!({ "common": { "dilation": 4 } })).unwrap();
        assert!(settings.sync.common);
        assert!(BakedMap::ALL.iter().all(|map| settings.sync.shares(BakeSyncGroup::Map(*map))));
    }

    #[test]
    fn a_set_left_out_of_common_sync_keeps_its_own_common_settings() {
        let mut session = session();
        assert_eq!(session.doc.project.bake_synced_sets(0, BakeSyncGroup::Common), [0, 1]);

        execute(&mut session, "bake.sync", &json!({ "group": "common", "ts": [0] })).unwrap();
        assert_eq!(session.doc.project.bake_synced_sets(0, BakeSyncGroup::Common), [0]);
        assert_eq!(session.doc.project.bake_synced_sets(1, BakeSyncGroup::Common), [1]);
        assert_eq!(session.doc.project.bake_synced_sets(1, BakeSyncGroup::Map(BakedMap::Normal)), [0, 1], "map sync is separate");

        let reached = session.doc.project.bake_synced_sets(0, BakeSyncGroup::Common);
        execute(&mut session, "bake.settings", &json!({ "ts": reached, "patch": { "common": { "dilation": 7 } } })).unwrap();
        assert_eq!((bake(&session, 0).common.dilation, bake(&session, 1).common.dilation), (7, 32));
    }

    #[test]
    fn a_set_joining_a_group_takes_its_settings_and_keeps_its_own_inclusion() {
        let mut session = session();
        execute(&mut session, "bake.sync", &json!({ "group": "ambientOcclusion", "ts": [0] })).unwrap();
        session.doc.project.texture_sets[1].bake.maps.ao.parameters.rays.rays = 3;
        session.doc.project.texture_sets[1].bake.common.is_enabled = false;
        execute(&mut session, "bake.sync", &json!({ "group": "common", "ts": [0] })).unwrap();
        session.doc.project.texture_sets[1].bake.common.dilation = 5;

        execute(&mut session, "bake.sync", &json!({ "group": "ambientOcclusion", "ts": "all" })).unwrap();
        execute(&mut session, "bake.sync", &json!({ "group": "common", "ts": "all" })).unwrap();
        assert_eq!(bake(&session, 1).maps.ao.parameters, bake(&session, 0).maps.ao.parameters);
        assert_eq!(bake(&session, 1).common.dilation, bake(&session, 0).common.dilation);
        assert!(!bake(&session, 1).common.is_enabled, "whether the set bakes is its own");

        session.undo().unwrap();
        assert_eq!(bake(&session, 1).common.dilation, 5, "one undo per sync");
    }

    #[test]
    fn copies_check_maps_and_carry_settings_without_linking() {
        let mut session = session();
        execute(&mut session, "bake.sync", &json!({ "group": "height", "ts": [0] })).unwrap();
        session.doc.project.texture_sets[0].bake.maps.height.parameters.scaling_divisor = 9.0;

        execute(&mut session, "bake.copy", &json!({ "what": "check", "map": "height", "ts": "all" })).unwrap();
        assert!(bake(&session, 0).maps.height.is_enabled && bake(&session, 1).maps.height.is_enabled);

        execute(&mut session, "bake.copy", &json!({ "what": "settings", "map": "height", "from": 0, "ts": [1] })).unwrap();
        assert_eq!(bake(&session, 1).maps.height.parameters.scaling_divisor, 9.0);
        assert!(!bake(&session, 1).sync.shares(BakeSyncGroup::Map(BakedMap::Height)), "a copy does not link");

        session.doc.project.texture_sets[0].bake.maps.select([BakedMap::Normal]);
        execute(&mut session, "bake.copy", &json!({ "what": "checked", "from": 0, "ts": [1] })).unwrap();
        assert_eq!(bake(&session, 1).maps.enabled().collect::<Vec<_>>(), [BakedMap::Normal]);
    }

    #[test]
    fn a_copy_reaches_the_sets_sharing_the_group_with_its_target() {
        let mut session = session();
        let third = session.doc.project.texture_sets[1].clone();
        session.doc.project.texture_sets.push(third);
        execute(&mut session, "bake.sync", &json!({ "group": "ambientOcclusion", "ts": [0, 1] })).unwrap();
        session.doc.project.texture_sets[2].bake.maps.ao.parameters.rays.rays = 3;

        execute(&mut session, "bake.copy", &json!({ "what": "settings", "from": 2, "ts": [0] })).unwrap();
        assert_eq!((bake(&session, 0).maps.ao.parameters.rays.rays, bake(&session, 1).maps.ao.parameters.rays.rays), (3, 3), "set 1 shares AO with set 0");
        assert!(!bake(&session, 2).sync.shares(BakeSyncGroup::Map(BakedMap::AmbientOcclusion)));
    }
}
