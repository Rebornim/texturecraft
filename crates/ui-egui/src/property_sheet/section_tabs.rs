use egui::{Align2, Color32, CornerRadius, FontId, Id, Rect, Role, Sense, Ui, WidgetInfo, pos2, vec2};

use super::{PAD, SMALL_TEXT_SIZE};
use crate::theme::{Tokens, semibold};

/// Height of the tab strip
const STRIP_HEIGHT: f32 = 30.0;

/// From the panel's left edge to the first tab
const FIRST_TAB: f32 = PAD - 4.0;

/// A tab's width beyond its name: icon and air
const TAB_PADDING: f32 = 38.0;

/// Air between tabs
const TAB_GAP: f32 = 2.0;

/// The chosen tab starts this far below the strip's top and reaches this far into the panel
const TAB_TOP: f32 = 4.0;
const TAB_BELOW: f32 = 6.0;

/// From a tab's left edge to its icon's centre, and to its name
const ICON_CENTER: f32 = 14.0;
const NAME_LEFT: f32 = 26.0;

/// Size of a tab's icon
const TAB_ICON_SIZE: f32 = 13.0;

/// A section counts as in view once its caption is this close to the top of the scrolled area
const IN_VIEW_SLACK: f32 = 30.0;

/// Which strip the sections belong to: the docked panel and the right-click popup can show the same sections at once
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub(crate) enum SectionScope {
    /// The docked Properties panel
    Docked,
    /// The right-click Properties popup over a view
    Popup,
}

/// Where the scope of the sections being drawn is kept while a strip's contents run
fn scope_id() -> Id {
    Id::unique("property_sheet_section_scope")
}

/// Where a strip keeps its section positions and the section one of its tabs asked to jump to
fn sections_id(scope: SectionScope) -> Id {
    Id::unique("property_sheet_sections").with(scope)
}

fn jump_id(scope: SectionScope) -> Id {
    Id::unique("property_sheet_jump").with(scope)
}

/// Runs `body` with its section captions tied to the `scope` strip
pub(crate) fn in_scope<R>(ui: &mut Ui, scope: SectionScope, body: impl FnOnce(&mut Ui) -> R) -> R {
    let outer = ui.data(|data| data.get_temp::<SectionScope>(scope_id()));
    ui.data_mut(|data| data.insert_temp(scope_id(), scope));
    let result = body(ui);
    ui.data_mut(|data| data.insert_temp(scope_id(), outer.unwrap_or(SectionScope::Docked)));
    result
}

/// The strip the sections being drawn belong to: the docked panel's unless a caller said otherwise
fn current_scope(ui: &Ui) -> SectionScope {
    ui.data(|data| data.get_temp::<SectionScope>(scope_id())).unwrap_or(SectionScope::Docked)
}

/// The sections' captions seen last frame, top to bottom, the area they scroll in and whether it is scrolled to its end
#[derive(Clone, Default)]
pub(super) struct SectionTops {
    pub(super) tops: Vec<(String, f32)>,
    pub(super) view_top: f32,
    pub(super) view_bottom: f32,
    pub(super) at_end: bool,
}

impl SectionTops {
    /// The section in view: the last caption at the top, or at the scroll's end the last one shown (a short last section never reaches the top)
    fn lit(&self) -> Option<&str> {
        let scrolled = self.tops.first().is_some_and(|(_, top)| *top < self.view_top);
        let shown = |top: f32| if self.at_end && scrolled { top < self.view_bottom } else { top <= self.view_top + IN_VIEW_SLACK };
        let reached = self.tops.iter().rev().find(|(_, top)| shown(*top));
        reached.or(self.tops.first()).map(|(title, _)| title.as_str())
    }
}

/// Stores this frame's section captions for the tabs
pub(super) fn remember(ui: &Ui, tops: SectionTops) {
    let scope = current_scope(ui);
    ui.data_mut(|data| data.insert_temp(sections_id(scope), tops));
}

/// Whether a tab asked to show `title`: the caller scrolls to it once
pub(super) fn take_jump(ui: &Ui, title: &str) -> bool {
    let jump = jump_id(current_scope(ui));
    let wanted = ui.data(|data| data.get_temp::<String>(jump)).is_some_and(|wanted| wanted == title);
    if wanted {
        ui.data_mut(|data| data.remove::<String>(jump));
    }
    wanted
}

/// The tab strip on the `scope` panel's top edge, top corners rounded by `top_corner`: one tab per section; a click scrolls to it
pub(crate) fn section_tabs(ui: &mut Ui, scope: SectionScope, top_corner: u8, tabs: &[(&str, &str)]) {
    let tokens = Tokens::get(ui.ctx());
    let (strip, _) = ui.allocate_exact_size(vec2(ui.available_width(), STRIP_HEIGHT), Sense::hover());
    let corners = CornerRadius { nw: top_corner, ne: top_corner, sw: 0, se: 0 };
    ui.painter().rect_filled(strip, corners, tokens.chrome);
    let seen = ui.data(|data| data.get_temp::<SectionTops>(sections_id(scope))).unwrap_or_default();
    let lit = seen.lit().map(str::to_owned);

    let mut x = strip.left() + FIRST_TAB;
    for (index, (icon, title)) in tabs.iter().enumerate() {
        let on = lit.as_deref().map_or(index == 0, |lit| lit == *title);
        let width = ui.painter().layout_no_wrap((*title).to_owned(), semibold(SMALL_TEXT_SIZE), Color32::WHITE).size().x + TAB_PADDING;
        let tab = Rect::from_min_max(pos2(x, strip.top() + TAB_TOP), pos2(x + width, strip.bottom() + TAB_BELOW));
        let response = ui.interact(tab, ui.scope_id().with(("section_tab", index)), Sense::click());
        response.widget_info(|| WidgetInfo::selected(Role::Tab, true, on, *title));
        if response.clicked() {
            ui.data_mut(|data| data.insert_temp(jump_id(scope), (*title).to_owned()));
        }

        let color = if on || response.hovered() { tokens.text } else { tokens.text_dim };
        let painter = ui.painter();
        if on {
            painter.rect_filled(tab, CornerRadius::same(5), tokens.panel);
        }
        let center_y = strip.center().y + TAB_TOP / 2.0;
        painter.text(pos2(x + ICON_CENTER, center_y), Align2::CENTER_CENTER, *icon, FontId::proportional(TAB_ICON_SIZE), color);
        let font = if on { semibold(SMALL_TEXT_SIZE) } else { FontId::proportional(SMALL_TEXT_SIZE) };
        painter.text(pos2(x + NAME_LEFT, center_y), Align2::LEFT_CENTER, *title, font, color);
        x += width + TAB_GAP;
    }
}
