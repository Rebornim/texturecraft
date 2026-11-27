use std::collections::BTreeSet;

use super::{DescriptionMode, RowEdit};

/// The list's UI state: the row being edited, which descriptions show, folded UV tile sets and the selected tile
#[derive(Clone, Default)]
pub(crate) struct ListState {
    /// Set, what it edits, the text so far, and whether the field still needs focus
    pub(crate) edit: Option<(usize, RowEdit, String, bool)>,
    pub(crate) descriptions: DescriptionMode,
    /// UV tile sets whose tile rows are folded away
    pub(crate) folded: BTreeSet<usize>,
    /// Set and UDIM of the selected UV tile row
    pub(crate) tile: Option<(usize, u32)>,
}

impl ListState {
    fn id() -> egui::Id {
        egui::Id::unique("texture_set_list_state")
    }

    pub(crate) fn load(ctx: &egui::Context) -> Self {
        ctx.data(|data| data.get_temp::<Self>(Self::id())).unwrap_or_default()
    }

    pub(crate) fn store(self, ctx: &egui::Context) {
        ctx.data_mut(|data| data.insert_temp(Self::id(), self));
    }
}
