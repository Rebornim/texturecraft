/// A Common Settings section the jump icons above the panel scroll to
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SettingsSection {
    Common,
    Cage,
    Skew,
}

impl SettingsSection {
    pub(crate) const ALL: [Self; 3] = [Self::Common, Self::Cage, Self::Skew];
}
