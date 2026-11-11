//! The dialog that picks texture sets: which ones share settings, or which ones a copy goes to

use egui::{Align, Layout, RichText, Ui};
use egui_phosphor::regular as icon;
use serde_json::json;
use texturecraft_engine::core::{BakeSyncGroup, BakedMap};

use super::set_picker_purpose::SetPickerPurpose;
use crate::TextureCraftApp;
use crate::i18n::{baked_map_label, tr, trf};

/// Width of the dialog, in points
const DIALOG_WIDTH: f32 = 420.0;

/// An open picker: what it is for, and per texture set whether it is ticked
#[derive(Clone, Debug)]
pub(super) struct SetPicker {
    purpose: SetPickerPurpose,
    chosen: Vec<bool>,
}

impl SetPicker {
    fn id() -> egui::Id {
        egui::Id::unique("bake_set_picker")
    }

    /// Open the picker for `purpose`; a sync picker starts with the sets sharing the group ticked, a copy picker with none
    pub(super) fn open(app: &TextureCraftApp, ctx: &egui::Context, purpose: SetPickerPurpose) {
        let chosen = app
            .session
            .doc
            .project
            .texture_sets
            .iter()
            .map(|set| match purpose {
                SetPickerPurpose::Sync(group) => set.bake.sync.shares(group),
                SetPickerPurpose::Copy { .. } => false,
            })
            .collect();
        ctx.data_mut(|data| data.insert_temp(Self::id(), SetPicker { purpose, chosen }));
    }

    /// The open picker, if any; OK runs its command
    pub(super) fn show(app: &mut TextureCraftApp, ctx: &egui::Context) {
        let Some(mut picker) = ctx.data(|data| data.get_temp::<SetPicker>(Self::id())) else { return };

        let names: Vec<String> = app.session.doc.project.texture_sets.iter().map(|set| set.name.clone()).collect();
        let active = app.session.ts();
        let modal = egui::Modal::new(Self::id().with("modal")).show(ctx, |ui| {
            ui.set_width(DIALOG_WIDTH);
            ui.label(RichText::new(picker.title()).strong());
            ui.add_space(8.0);
            if let SetPickerPurpose::Sync(group) = picker.purpose {
                ui.horizontal_top(|ui| {
                    ui.label(RichText::new(icon::GEAR).size(18.0));
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        ui.label(tr("All the selected Texture Sets will share their "));
                        ui.label(RichText::new(group_label(group)).strong());
                        ui.label(tr(". New additions will take on the currently synced settings."));
                    });
                });
                ui.add_space(6.0);
            }
            picker.list(ui, &names, active);
            ui.add_space(12.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let cancel = ui.button(tr("Cancel")).clicked();
                let ok = ui.button(tr("OK")).clicked();
                (ok, cancel)
            })
            .inner
        });

        let (ok, cancel) = modal.inner;
        if ok {
            picker.apply(app, active);
        }
        if ok || cancel || modal.should_close() {
            ctx.data_mut(|data| data.remove::<SetPicker>(Self::id()));
        } else {
            ctx.data_mut(|data| data.insert_temp(Self::id(), picker));
        }
    }

    fn title(&self) -> String {
        match self.purpose {
            SetPickerPurpose::Sync(group) => trf("{settings} Sync", &[("settings", &group_label(group))]),
            SetPickerPurpose::Copy { what: "checked", .. } => tr("Apply Checked to More Texture Sets").to_owned(),
            SetPickerPurpose::Copy { what: "check", map: Some(map) } => trf("Check {map} for More Texture Sets", &[("map", &tr(baked_map_label(map)))]),
            SetPickerPurpose::Copy { map: Some(map), .. } => trf("Apply {map} Settings to More Texture Sets", &[("map", &tr(baked_map_label(map)))]),
            SetPickerPurpose::Copy { .. } => tr("Sync All Settings to More Texture Sets").to_owned(),
        }
    }

    /// The master checkbox and one checkbox per set; a copy goes from the active set, so it isn't listed
    fn list(&mut self, ui: &mut Ui, names: &[String], active: usize) {
        let listed: Vec<usize> = (0..names.len()).filter(|index| matches!(self.purpose, SetPickerPurpose::Sync(_)) || *index != active).collect();
        let ticked = listed.iter().filter(|index| self.chosen.get(**index).copied().unwrap_or(false)).count();
        let all_ticked = ticked == listed.len();
        let label = if all_ticked { tr("Deselect All") } else { tr("Select All") };
        let mut all = all_ticked;
        if ui.add(egui::Checkbox::new(&mut all, label).indeterminate(ticked > 0 && !all_ticked)).changed() {
            for index in &listed {
                if let Some(chosen) = self.chosen.get_mut(*index) {
                    *chosen = all;
                }
            }
        }
        super::controls::indented(ui, |ui| {
            ui.vertical(|ui| {
                for index in listed {
                    if let (Some(chosen), Some(name)) = (self.chosen.get_mut(index), names.get(index)) {
                        ui.checkbox(chosen, name);
                    }
                }
            });
        });
    }

    fn apply(&self, app: &mut TextureCraftApp, active: usize) {
        let ticked: Vec<usize> = self.chosen.iter().enumerate().filter(|(_, chosen)| **chosen).map(|(index, _)| index).collect();
        match self.purpose {
            SetPickerPurpose::Sync(group) => {
                app.run("bake.sync", json!({ "group": group_id(group), "ts": ticked }));
            }
            SetPickerPurpose::Copy { what, map } if !ticked.is_empty() => {
                app.run("bake.copy", json!({ "what": what, "map": map.map(BakedMap::id), "from": active, "ts": ticked }));
            }
            SetPickerPurpose::Copy { .. } => {}
        }
    }
}

/// The group as the `bake.sync` command names it
pub(super) fn group_id(group: BakeSyncGroup) -> &'static str {
    match group {
        BakeSyncGroup::Common => "common",
        BakeSyncGroup::Map(map) => map.id(),
    }
}

fn group_label(group: BakeSyncGroup) -> String {
    match group {
        BakeSyncGroup::Common => tr("Common Settings").to_owned(),
        BakeSyncGroup::Map(map) => trf("{map} Settings", &[("map", &tr(baked_map_label(map)))]),
    }
}
