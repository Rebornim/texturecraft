use egui::ecolor::hsv_from_rgb;

use super::{ColorModel, ColorSpace};

/// One colour bar's picker between frames: open or not, the colour it opened on, and the hue and saturation a gray would lose
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct PickerState {
    pub(crate) open: bool,
    /// The colour when the picker opened (linear), shown beside the new one and restored by a click
    pub(crate) before: [f32; 3],
    /// The colour being edited (linear)
    pub(crate) linear: [f32; 3],
    /// Hue, saturation and value of the shown values, kept across grays and blacks
    pub(crate) hsv: [f32; 3],
    pub(crate) space: ColorSpace,
    pub(crate) model: ColorModel,
    /// Where its title bar's drag left the window's top left corner; None while it stands beside the panel
    pub(crate) moved_to: Option<egui::Pos2>,
}

impl PickerState {
    /// Follows `linear` when it changed outside the picker (undo, another panel), keeping the hue when it didn't
    pub(crate) fn follow(&mut self, linear: [f32; 3]) {
        if self.linear != linear {
            self.set_linear(linear);
        }
    }

    /// Sets the colour from outside the hue field (typed, restored, picked), deriving hue, saturation and value again
    pub(crate) fn set_linear(&mut self, linear: [f32; 3]) {
        self.linear = linear;
        let (hue, saturation, value) = hsv_from_rgb(self.shown());
        self.hsv = [hue, saturation, value];
    }

    /// Sets the colour from hue, saturation and value (field, strip, HSV sliders)
    pub(crate) fn set_hsv(&mut self, hsv: [f32; 3]) {
        self.hsv = hsv.map(|part| part.clamp(0.0, 1.0));
        let [hue, saturation, value] = self.hsv;
        self.linear = self.space.linear(egui::ecolor::rgb_from_hsv((hue, saturation, value)));
    }

    /// Shows the values in `space` from now on
    pub(crate) fn set_space(&mut self, space: ColorSpace) {
        self.space = space;
        self.set_linear(self.linear);
    }

    /// The colour's values in the shown space
    pub(crate) fn shown(&self) -> [f32; 3] {
        self.space.shown(self.linear)
    }

    /// Opens on `linear`, which becomes the colour to go back to
    pub(crate) fn open_on(&mut self, linear: [f32; 3]) {
        self.open = true;
        self.before = linear;
        self.set_linear(linear);
    }
}
