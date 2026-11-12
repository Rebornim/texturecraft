use super::psd_layer::PsdLayer;

/// A record of a PSD's layer list, top of the stack first: a group opens, a layer, or the group closes
pub enum PsdEntry<'a> {
    GroupStart(&'a str),
    Layer(&'a PsdLayer),
    GroupEnd,
}
