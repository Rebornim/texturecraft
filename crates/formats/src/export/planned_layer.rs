/// An output map written as a layer of a container file (PSD) rather than a file of its own
#[derive(Clone, Debug, PartialEq)]
pub struct PlannedLayer {
    /// Index into the preset's maps
    pub map: usize,
    /// The layer's name, in the groups its `/`-separated folders make
    pub name: String,
}
