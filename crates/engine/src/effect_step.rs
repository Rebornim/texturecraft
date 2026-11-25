//! Which way a step of `effect.moveUp` / `effect.moveDown` moves an effect

/// Which way an effect moves one place: up is later in its stack (rows list the top first)
#[derive(Clone, Copy)]
pub(crate) enum EffectStep {
    Up,
    Down,
}
