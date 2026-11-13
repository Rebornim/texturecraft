//! The filter picker hanging from the filter card: a grid of the library's filters that swaps the effect's filter

use egui_phosphor::regular as ic;
use serde_json::json;
use texturecraft_engine::core::{Filter, LayerId};
use texturecraft_engine::filter_lib::{filter_effects, find};

use crate::TextureCraftApp;
use crate::i18n::tr;
use crate::picker_group::PickerGroup;
use crate::picker_tab::PickerTab;
use crate::resource_picker;
use crate::resource_picker_item::ResourcePickerItem;
use crate::thumbs::Thumbs;
use crate::tile_picture::TilePicture;

/// The filter picker of filter effect `effect`, hanging from its card's row `pick`: a pick runs `effect.set {filter}`, masks offer only mask filters
pub(crate) fn picker(app: &mut TextureCraftApp, pick: &egui::Response, effect: u64, filter: &Filter) {
    let ctx = pick.ctx.clone();
    let on_mask = is_on_mask(app, effect);
    let current = find(filter.label()).map(|def| def.id);
    let popup = pick.id.with(("filter_picker", effect));
    let open_requested = take_open_request(&ctx, effect);
    if pick.clicked() || (open_requested && !egui::Popup::is_id_open(&ctx, popup)) {
        resource_picker::toggle(&ctx, popup);
    }

    let tabs = |thumbs: &mut Thumbs| {
        let mut items: Vec<ResourcePickerItem<&'static str>> = filter_effects()
            .iter()
            .filter(|def| def.mask || !on_mask)
            .map(|def| ResourcePickerItem {
                id: def.id,
                label: tr(def.label).to_owned(),
                caption: None,
                picture: TilePicture::Texture(thumbs.filter(&ctx, def.id)),
            })
            .collect();
        // One kind of resource: one grid in name order, no group captions
        items.sort_by_key(|item| item.label.to_lowercase());
        vec![PickerTab { icon: ic::FUNNEL, name: tr("Filters"), groups: vec![PickerGroup { header: None, items }] }]
    };
    let picked = resource_picker::show(pick, popup, &mut app.thumbs, current.as_ref(), current.is_some(), tabs);
    if let Some(id) = picked.filter(|id| Some(*id) != current) {
        app.run("effect.set", json!({"effect": effect, "filter": id}));
    }
}

/// Has the slot of filter effect `effect` open its picker if it shows this frame or the next, as Add filter does for the filter it adds
pub(crate) fn open_picker_when_shown(ctx: &egui::Context, effect: u64) {
    let frame = ctx.cumulative_frame_nr();
    ctx.data_mut(|data| data.insert_temp(open_request_id(), (effect, frame)));
}

/// Whether an open request for `effect`'s slot is waiting, removing it; a request older than a frame lapses
fn take_open_request(ctx: &egui::Context, effect: u64) -> bool {
    let frame = ctx.cumulative_frame_nr();
    let request = ctx.data_mut(|data| data.remove_temp::<(u64, u64)>(open_request_id()));

    // Properties can draw before or after the menu that asked, so a request lives for this frame and the next
    request.is_some_and(|(requested_effect, requested_frame)| requested_effect == effect && frame <= requested_frame + 1)
}

/// Where the effect whose slot should open its picker waits
fn open_request_id() -> egui::Id {
    egui::Id::unique("filter_slot_open_request")
}

/// Whether filter effect `effect` sits in a mask rather than the layer content
pub(crate) fn is_on_mask(app: &TextureCraftApp, effect: u64) -> bool {
    let project = &app.session.doc.project;
    let owner = project.effect(LayerId(effect)).and_then(|(owner, _)| project.layers.get(&owner));
    owner.and_then(|layer| layer.mask.as_ref()).is_some_and(|mask| mask.effects.iter().any(|masked| masked.id.0 == effect))
}
