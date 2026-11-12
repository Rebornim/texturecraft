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

/// Where the Properties panel keeps its section positions and the section a tab asked to jump to
fn sections_id() -> Id {
    Id::unique("property_sheet_sections")
}

fn jump_id() -> Id {
    Id::unique("property_sheet_jump")
}

/// The sections' captions seen last frame, top to bottom, and the top of the area they scroll in
#[derive(Clone, Default)]
pub(super) struct SectionTops {
    pub(super) tops: Vec<(String, f32)>,
    pub(super) view_top: f32,
}

impl SectionTops {
    /// The section in view: the last whose caption has reached the top
    fn lit(&self) -> Option<&str> {
        let reached = self.tops.iter().rev().find(|(_, top)| *top <= self.view_top + IN_VIEW_SLACK);
        reached.or(self.tops.first()).map(|(title, _)| title.as_str())
    }
}

/// Stores this frame's section captions for the tabs
pub(super) fn remember(ui: &Ui, tops: SectionTops) {
    ui.data_mut(|data| data.insert_temp(sections_id(), tops));
}

/// Whether a tab asked to show `title`: the caller scrolls to it once
pub(super) fn take_jump(ui: &Ui, title: &str) -> bool {
    let wanted = ui.data(|data| data.get_temp::<String>(jump_id())).is_some_and(|jump| jump == title);
    if wanted {
        ui.data_mut(|data| data.remove::<String>(jump_id()));
    }
    wanted
}

/// The tab strip on the panel's top edge: one tab per section (icon, section title); a click scrolls to the section
pub(crate) fn section_tabs(ui: &mut Ui, tabs: &[(&str, &str)]) {
    let tokens = Tokens::get(ui.ctx());
    let (strip, _) = ui.allocate_exact_size(vec2(ui.available_width(), STRIP_HEIGHT), Sense::hover());
    ui.painter().rect_filled(strip, CornerRadius::ZERO, tokens.chrome);
    let seen = ui.data(|data| data.get_temp::<SectionTops>(sections_id())).unwrap_or_default();
    let lit = seen.lit().map(str::to_owned);

    let mut x = strip.left() + FIRST_TAB;
    for (index, (icon, title)) in tabs.iter().enumerate() {
        let on = lit.as_deref().map_or(index == 0, |lit| lit == *title);
        let width = ui.painter().layout_no_wrap((*title).to_owned(), semibold(SMALL_TEXT_SIZE), Color32::WHITE).size().x + TAB_PADDING;
        let tab = Rect::from_min_max(pos2(x, strip.top() + TAB_TOP), pos2(x + width, strip.bottom() + TAB_BELOW));
        let response = ui.interact(tab, ui.scope_id().with(("section_tab", index)), Sense::click());
        response.widget_info(|| WidgetInfo::selected(Role::Tab, true, on, *title));
        if response.clicked() {
            ui.data_mut(|data| data.insert_temp(jump_id(), (*title).to_owned()));
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
