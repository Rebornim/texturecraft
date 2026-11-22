//! The Properties panel's rows, groups and controls, shared by every panel so they line up

mod section_tabs;

pub(crate) use section_tabs::{full_height_tab_rect, shown_tab_box};

/// Side padding: every row starts and ends this far inside the panel
pub(crate) const PAD: f32 = 12.0;
