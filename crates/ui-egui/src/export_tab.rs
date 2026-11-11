use crate::i18n::tr;

/// The Export textures window's tabs, in the order they show
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ExportTab {
    #[default]
    Settings,
    OutputTemplates,
    ListOfExports,
}

impl ExportTab {
    pub const ALL: [ExportTab; 3] = [ExportTab::Settings, ExportTab::OutputTemplates, ExportTab::ListOfExports];

    pub fn label(self) -> &'static str {
        match self {
            ExportTab::Settings => tr("Settings"),
            ExportTab::OutputTemplates => tr("Output templates"),
            ExportTab::ListOfExports => tr("List of exports"),
        }
    }
}
