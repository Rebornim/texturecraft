use std::ops::Deref;

use super::Inputs;
use crate::ProjectorDepthCache;

pub(super) struct EvaluationInputs<'a> {
    inputs: &'a Inputs<'a>,
    pub(super) projectors: &'a ProjectorDepthCache,
}

impl<'a> EvaluationInputs<'a> {
    pub(super) fn new(inputs: &'a Inputs<'a>, projectors: &'a ProjectorDepthCache) -> Self {
        Self { inputs, projectors }
    }
}

impl<'a> Deref for EvaluationInputs<'a> {
    type Target = Inputs<'a>;

    fn deref(&self) -> &Self::Target {
        self.inputs
    }
}
