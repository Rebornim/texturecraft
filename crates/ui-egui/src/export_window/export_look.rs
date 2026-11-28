use egui::{Color32, CornerRadius, Stroke, Style, TextStyle, style::StyleModifier};

use crate::theme::{Tokens, contrast_ratio, lerp_color};

/// Text size of the whole window, labels and values alike
pub(crate) const TEXT_SIZE: f32 = 12.0;

/// Height and corner radius of the window's controls: dropdowns, fields and buttons
pub(crate) const CONTROL_HEIGHT: f32 = 25.0;
pub(crate) const CONTROL_RADIUS: u8 = 3;

/// Height of an option in an open list
const OPTION_HEIGHT: f32 = 22.0;
/// An open list's margin round its options
const LIST_MARGIN: i8 = 3;
/// An option's text inset, lining up with the dropdown's value above it
const OPTION_INSET: f32 = 3.0;

/// A scroll bar's strip beside its list: the bar and the air on either side of it
const SCROLL_BAR_WIDTH: f32 = 4.0;
const SCROLL_BAR_MARGIN: f32 = 4.0;

/// How far the dividers sit below the frame colour, so they read on it
const DIVIDER_DARKENING: f32 = 0.26;

/// Split the difference between the field outline and text so control edges read against the panel
const BORDER_TEXT_MIX: f32 = 0.5;

/// How far the Presets list's names sit from the text colour towards the dim one
const LIST_TEXT_DIMMING: f32 = 0.37;

/// The Export textures window's colours from the theme: controls a step darker than the window, raised faces a step lighter
#[derive(Clone, Copy)]
pub(crate) struct ExportLook {
    pub(crate) window: Color32,
    pub(crate) control: Color32,
    pub(crate) control_border: Color32,
    /// A step lighter than controls: hover, lit and unusable faces
    pub(crate) raised: Color32,
    pub(crate) chrome: Color32,
    pub(crate) divider: Color32,
    pub(crate) outline: Color32,
    pub(crate) text: Color32,
    pub(crate) text_dim: Color32,
    pub(crate) list_text: Color32,
    pub(crate) accent: Color32,
}

impl ExportLook {
    pub(crate) fn get(ctx: &egui::Context) -> Self {
        let tokens = Tokens::get(ctx);
        Self {
            window: tokens.panel,
            control: tokens.row,
            control_border: lerp_color(tokens.field_border, tokens.text, BORDER_TEXT_MIX),
            raised: tokens.separator,
            chrome: tokens.chrome,
            divider: lerp_color(tokens.chrome, Color32::BLACK, DIVIDER_DARKENING),
            outline: lerp_color(tokens.button_border, tokens.text, BORDER_TEXT_MIX),
            text: tokens.text,
            text_dim: tokens.text_dim,
            list_text: lerp_color(tokens.text, tokens.text_dim, LIST_TEXT_DIMMING),
            accent: tokens.accent,
        }
    }

    /// Prefer the theme lettering when it meets the normal-text contrast threshold.
    pub(crate) fn lettering_on(&self, fill: Color32) -> Color32 {
        const MIN_TEXT_CONTRAST: f32 = 4.5;
        if contrast_ratio(self.text, fill) >= MIN_TEXT_CONTRAST {
            return self.text;
        }

        if contrast_ratio(Color32::WHITE, fill) >= MIN_TEXT_CONTRAST { Color32::WHITE } else { Color32::BLACK }
    }

    /// 12 point text, the window's control colours, scroll bars in their own strip beside the list
    pub(crate) fn apply(&self, style: &mut Style) {
        for text_style in [TextStyle::Body, TextStyle::Button] {
            if let Some(font) = style.text_styles.get_mut(&text_style) {
                font.size = TEXT_SIZE;
            }
        }
        style.spacing.interact_size.y = CONTROL_HEIGHT;
        style.spacing.button_padding.x = super::drawn::VALUE_INSET;
        style.spacing.scroll = egui::style::ScrollStyle {
            bar_width: SCROLL_BAR_WIDTH,
            bar_inner_margin: SCROLL_BAR_MARGIN,
            bar_outer_margin: SCROLL_BAR_MARGIN,
            ..egui::style::ScrollStyle::solid()
        };

        // The scroll strip shows no track, only the bar
        style.visuals.extreme_bg_color = Color32::TRANSPARENT;
        let widgets = &mut style.visuals.widgets;
        for (state, fill, border) in [
            (&mut widgets.inactive, self.control, self.control_border),
            (&mut widgets.hovered, self.raised, self.outline),
            (&mut widgets.active, self.raised, self.outline),
            (&mut widgets.open, self.control, self.control_border),
        ] {
            state.weak_bg_fill = fill;
            state.bg_stroke = Stroke::new(1.0, border);
            state.corner_radius = CornerRadius::same(CONTROL_RADIUS);
            state.expansion = 0.0;
        }

        // Scroll bars and checks use the strong fill: the raised grey
        widgets.inactive.bg_fill = self.raised;
        widgets.hovered.bg_fill = self.outline;
        widgets.active.bg_fill = self.outline;
        widgets.inactive.fg_stroke.color = self.text;
        widgets.open.fg_stroke.color = self.text;
        style.visuals.selection.bg_fill = self.chrome;
        style.visuals.selection.stroke = Stroke::new(1.0, self.text);
    }

    /// An open list: flush under its button, one light edge, the chosen option in the frame colour
    pub(crate) fn list_style(&self) -> StyleModifier {
        let look = *self;
        StyleModifier::new(move |style: &mut Style| {
            look.apply(style);
            style.spacing.interact_size.y = OPTION_HEIGHT;
            style.spacing.menu_margin = egui::Margin::same(LIST_MARGIN);
            style.spacing.button_padding.x = OPTION_INSET;
            style.spacing.item_spacing.y = 0.0;
            style.visuals.window_fill = look.control;
            style.visuals.window_stroke = Stroke::new(1.0, look.control_border);
            style.visuals.menu_corner_radius = CornerRadius::same(CONTROL_RADIUS);
            style.visuals.widgets.hovered.weak_bg_fill = look.raised;
            style.visuals.widgets.hovered.bg_stroke = Stroke::NONE;
            style.visuals.widgets.inactive.bg_stroke = Stroke::NONE;
            style.visuals.widgets.active.bg_stroke = Stroke::NONE;
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{Accent, Brightness, apply};

    #[test]
    fn export_button_and_empty_swatch_text_and_control_edges_read_in_each_theme() {
        let context = egui::Context::default();
        for brightness in Brightness::ALL {
            for accent in Accent::ALL {
                apply(&context, brightness, accent);
                let look = ExportLook::get(&context);

                for fill in [look.accent, look.raised] {
                    assert!(contrast_ratio(look.lettering_on(fill), fill) >= 4.5, "{brightness:?} {accent:?}");
                }
                for background in [look.window, look.control] {
                    assert!(contrast_ratio(look.control_border, background) >= 3.0, "{brightness:?}");
                }
            }
        }
    }
}
