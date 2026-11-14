use super::IN_VIEW_SLACK;

/// The sections' captions seen last frame, top to bottom, the area they scroll in and whether it is scrolled to its end
#[derive(Clone, Default)]
pub(crate) struct SectionTops {
    pub(crate) tops: Vec<(String, f32)>,
    pub(crate) view_top: f32,
    pub(crate) view_bottom: f32,
    pub(crate) at_end: bool,
}

impl SectionTops {
    /// The section in view: the last caption at the top, or at the scroll's end the last one shown (a short last section never reaches the top)
    pub(crate) fn lit(&self) -> Option<&str> {
        let scrolled = self.tops.first().is_some_and(|(_, top)| *top < self.view_top);
        let shown = |top: f32| if self.at_end && scrolled { top < self.view_bottom } else { top <= self.view_top + IN_VIEW_SLACK };
        let reached = self.tops.iter().rev().find(|(_, top)| shown(*top));
        reached.or(self.tops.first()).map(|(title, _)| title.as_str())
    }
}
