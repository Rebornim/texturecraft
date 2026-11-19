//! The shown tab of a docked panel group, drawn as the Properties section tabs draw theirs: a raised box joined to the panel's body

use craft_ui::docking::{DockContent, Layout, Node};
use egui::text::{LayoutJob, TextWrapping};
use egui::{Rect, Ui, pos2};

use super::{HEADER_TITLE_SIZE, Panel, Panels};
use crate::property_sheet::{shown_tab_box, tab_rect};
use crate::theme::semibold;

/// Room the dock leaves around a tab's name, half on each side (craft-ui's dock renderer keeps its own copy private)
const DOCK_TAB_PADDING: f32 = 24.0;

/// From a panel's left edge to its header's name
pub(crate) const HEADER_TITLE_LEFT: f32 = DOCK_TAB_PADDING / 2.0;

/// Width of the dock's "…" button for the tabs that do not fit (private in craft-ui's dock renderer as well)
const OVERFLOW_WIDTH: f32 = 22.0;

/// Room between a tab's edges and its box, so neighbouring boxes never touch
const BOX_INSET: f32 = 4.0;

/// The panels of the group holding `panel` and whether that group floats; a collapsed stack has no tabs
fn group_of(layout: &Layout<Panel>, panel: Panel) -> Option<(&[Panel], bool)> {
    let docked = layout.root.iter().map(|root| (root, false));
    let floating = layout.floating.iter().map(|window| (&window.root, true));
    let mut nodes: Vec<(&Node<Panel>, bool)> = docked.chain(floating).collect();
    while let Some((node, floats)) = nodes.pop() {
        match node {
            Node::Split { first, second, .. } => {
                nodes.push((first, floats));
                nodes.push((second, floats));
            }
            Node::Tabs { panels, .. } if panels.contains(&panel) => return Some((panels, floats)),
            Node::Dock { columns, .. } => {
                let group = columns.iter().flat_map(|column| &column.groups).find(|group| group.panels.contains(&panel));
                if let Some(group) = group {
                    return Some((&group.panels, floats));
                }
            }
            _ => {}
        }
    }
    None
}

impl Panels<'_> {
    /// Over the tab the dock drew for `panel`, the panel shown in its group, paints the section tabs' raised box and the name again; a panel alone keeps its plain header. `ui` is the panel's body, right under the strip
    pub(super) fn paint_shown_group_tab(&self, ui: &Ui, panel: Panel) {
        let Some((panels, floating)) = group_of(&self.app.ui.dock.workspace.layout, panel) else { return };
        if panels.len() < 2 {
            return;
        }

        let Some(shown) = panels.iter().position(|member| *member == panel) else { return };

        // The dock's own tab layout: names at their natural width, shrunk or moved to "…" when the strip is short
        let body = ui.max_rect();
        let strip = Rect::from_min_max(pos2(body.left(), body.top() - self.style.tab_height), pos2(body.right(), body.top()));
        let font = semibold(HEADER_TITLE_SIZE);
        let names: Vec<String> = panels.iter().map(|member| self.logical_label(member).to_uppercase()).collect();
        let natural: Vec<f32> =
            names.iter().map(|name| ui.painter().layout_no_wrap(name.clone(), font.clone(), self.style.text).size().x + DOCK_TAB_PADDING).collect();
        let header = self.group_header_width(panels, &panel, floating, strip.width()).clamp(0.0, strip.width());
        let available = (strip.width() - header - self.style.drag_bar_width).max(0.0);
        let fit = craft_ui::tabs::fit(&natural, shown, available, self.style.min_tab_width, OVERFLOW_WIDTH);
        let mut left = strip.left();
        for (index, width) in fit.shown {
            if index != shown {
                left += width;
                continue;
            }

            // The body's clip stops at the strip, which the box rises into
            let clip = ui.clip_rect();
            let mut painter = ui.painter().clone();
            painter.set_clip_rect(Rect::from_x_y_ranges(clip.x_range(), strip.top()..=clip.bottom()));
            shown_tab_box(&painter, &self.tokens, tab_rect(strip, left + BOX_INSET, left + width - BOX_INSET));

            let Some(name) = names.get(index) else { return };
            let mut job = LayoutJob::simple_singleline(name.clone(), font, self.style.text);
            job.wrap = TextWrapping::truncate_at_width((width - DOCK_TAB_PADDING).max(0.0));
            let galley = painter.layout_job(job);
            let at = pos2(left + HEADER_TITLE_LEFT, strip.center().y - galley.size().y / 2.0);
            painter.galley(at, galley, self.style.text);
            return;
        }
    }
}
