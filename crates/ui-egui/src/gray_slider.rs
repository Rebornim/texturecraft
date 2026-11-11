//! A gray value from black to white, as the reference shows a pattern's colors: a gradient bar with a knob, the value, and a button that inverts it

use egui::{Rect, Sense, Stroke, Ui, vec2};
use egui_phosphor::regular as ic;

use crate::i18n::tr;
use crate::theme::Tokens;

const BAR_HEIGHT: f32 = 8.0;
const BAR_ROUNDING: f32 = 2.0;
const KNOB_RADIUS: f32 = 6.0;
const KNOB_STROKE: f32 = 1.5;
const BAR_BORDER_WIDTH: f32 = 1.0;
/// The narrowest bar, room for the knob to move
const MIN_BAR_WIDTH: f32 = KNOB_RADIUS * 4.0;

/// `label` and `value` over a black-to-white bar and an Invert value button: `Some(merge)` on every frame that changes it, as [`crate::widgets::slider_edit`]
pub(crate) fn gray_slider(ui: &mut Ui, label: &str, value: &mut f32) -> Option<bool> {
    let tokens = Tokens::get(ui.ctx());
    ui.horizontal(|ui| {
        ui.label(label);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.label(format!("{value:.2}")));
    });
    ui.horizontal(|ui| {
        let mut edit = None;
        let button_width = ui.spacing().interact_size.y;
        let width = (ui.available_width() - button_width - ui.spacing().item_spacing.x).max(MIN_BAR_WIDTH);
        let (rect, response) = ui.allocate_exact_size(vec2(width, KNOB_RADIUS * 2.0 + KNOB_STROKE * 2.0), Sense::click_and_drag());
        let bar = Rect::from_center_size(rect.center(), vec2(rect.width() - KNOB_RADIUS * 2.0, BAR_HEIGHT));
        if let Some(pointer) = response.interact_pointer_pos().filter(|_| response.dragged() || response.clicked()) {
            *value = ((pointer.x - bar.left()) / bar.width().max(1.0)).clamp(0.0, 1.0);
            edit = Some(response.dragged() && !response.drag_started());
        }

        let painter = ui.painter();
        let mut mesh = egui::Mesh::default();
        for (corner, gray) in [(bar.left_top(), 0), (bar.right_top(), 255), (bar.right_bottom(), 255), (bar.left_bottom(), 0)] {
            mesh.colored_vertex(corner, egui::Color32::from_gray(gray));
        }
        mesh.add_triangle(0, 1, 2);
        mesh.add_triangle(0, 2, 3);
        painter.add(mesh);
        painter.rect_stroke(bar, BAR_ROUNDING, Stroke::new(BAR_BORDER_WIDTH, tokens.field_border), egui::StrokeKind::Outside);
        let knob = egui::pos2(bar.left() + bar.width() * value.clamp(0.0, 1.0), bar.center().y);
        painter.circle(knob, KNOB_RADIUS, tokens.panel, Stroke::new(KNOB_STROKE, tokens.text_strong));

        if ui.button(ic::CIRCLE_HALF).on_hover_text(tr("Invert value")).clicked() {
            *value = 1.0 - *value;
            edit = Some(false);
        }
        edit
    })
    .inner
}
