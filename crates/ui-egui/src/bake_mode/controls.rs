use crate::theme::{Tokens, caption};
use egui::collapsing_header::CollapsingState;
use egui::{RichText, Ui};

/// Height of a section header's reset button, in points
const RESET_BUTTON_SIZE: f32 = 20.0;

/// A section's title bar with its chevron and reset button; clicking the bar opens or closes the section. Returns the reset choice: 0 for a plain reset, else the index into `reset_menu`
fn section_header(ui: &mut Ui, title: &str, reset_menu: &[&str], collapsing_state: &mut CollapsingState) -> Option<usize> {
    let tokens = Tokens::get(ui.ctx());
    let header = egui::Frame::NONE.fill(tokens.row).corner_radius(tokens.cr()).inner_margin(egui::Margin::symmetric(8, 4)).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.set_min_height(ui.spacing().interact_size.y);
            let chevron = ui.allocate_exact_size(egui::vec2(ui.spacing().indent, ui.spacing().icon_width), egui::Sense::hover()).0;
            ui.add(egui::Label::new(caption(title).color(tokens.text_strong)).selectable(false));
            let reset = ui
                .with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.allocate_exact_size(egui::vec2(RESET_BUTTON_SIZE, RESET_BUTTON_SIZE), egui::Sense::hover()).0
                })
                .inner;
            (chevron, reset)
        })
        .inner
    });

    let (chevron_rect, reset_rect) = header.inner;
    let response = ui.interact(header.response.rect, ui.make_persistent_id("section_header"), egui::Sense::click());
    if response.clicked() {
        collapsing_state.toggle(ui);
    }

    let chevron = crate::widgets::section_chevron(ui, collapsing_state.is_open());
    ui.put(chevron_rect, egui::Label::new(chevron).selectable(false));

    // Added after the bar's own response, so the button takes the clicks over it
    let tooltip = crate::i18n::trf("Reset {section}", &[("section", &title)]);
    let mut button_ui = ui.new_child(egui::UiBuilder::new().max_rect(reset_rect));
    let button = crate::widgets::icon_button(&mut button_ui, egui_phosphor::regular::CLOCK_COUNTER_CLOCKWISE, &tooltip, false, RESET_BUTTON_SIZE);
    if reset_menu.is_empty() {
        return button.clicked().then_some(0);
    }

    let mut chosen = None;
    egui::Popup::menu(&button).show(|ui| {
        for (index, item) in reset_menu.iter().enumerate() {
            if ui.button(*item).clicked() {
                chosen = Some(index);
            }
        }
    });
    chosen
}

/// A collapsible section with `contents` under a header with a reset button (a menu of `reset_menu` when not empty); returns the reset choice (see [`section_header`])
pub(super) fn section(ui: &mut Ui, title: &str, reset_menu: &[&str], contents: impl FnOnce(&mut Ui)) -> Option<usize> {
    let mut collapsing_state = CollapsingState::load_with_default_open(ui.ctx(), ui.make_persistent_id("section"), true);
    let reset = section_header(ui, title, reset_menu, &mut collapsing_state);

    collapsing_state.show_body_unindented(ui, |ui| egui::Frame::NONE.inner_margin(egui::Margin::same(8)).show(ui, contents));
    reset
}

/// A bold sub-header inside a section, its controls indented below it (without the vertical line egui's indent draws)
pub(super) fn subsection(ui: &mut Ui, title: &str, contents: impl FnOnce(&mut Ui)) {
    ui.label(RichText::new(title).font(crate::theme::semibold(13.0)).color(Tokens::get(ui.ctx()).text_strong));
    indented(ui, contents);
}

/// `contents` one indent step in from the left, as the reference sets controls that belong to the one above
pub(super) fn indented(ui: &mut Ui, contents: impl FnOnce(&mut Ui)) {
    let indent = ui.spacing().indent;
    ui.horizontal_top(|ui| {
        ui.add_space(indent);
        ui.vertical(contents);
    });
}

pub(super) fn choice<T: Copy + PartialEq>(ui: &mut Ui, label: &str, value: &mut T, options: &[(T, &str)]) {
    let caption = ui.label(label);
    let selected = options.iter().find(|(option, _)| option == value).map_or("", |(_, label)| *label);
    egui::ComboBox::from_id_salt(label)
        .width(ui.available_width())
        .selected_text(selected)
        .show_ui(ui, |ui| {
            for (option, label) in options {
                ui.selectable_value(value, *option, *label);
            }
        })
        .response
        .labelled_by(caption.id);
}

pub(super) fn number(ui: &mut Ui, label: &str, value: &mut f32, range: std::ops::RangeInclusive<f32>, decimals: usize) {
    let response = crate::widgets::slider(ui, label, value, range.clone(), decimals);
    if response.changed() && (response.dragged() || response.clicked()) {
        let scale = 10_f32.powi(decimals as i32);
        *value = ((*value * scale).round() / scale).clamp(*range.start(), *range.end());
    }
}

pub(super) fn integer(ui: &mut Ui, label: &str, value: &mut u32, range: std::ops::RangeInclusive<u32>) {
    let mut number_value = *value as f32;
    let response = crate::widgets::slider(ui, label, &mut number_value, *range.start() as f32..=*range.end() as f32, 0);
    if response.changed() {
        *value = (number_value.round() as u32).clamp(*range.start(), *range.end());
    }
}

pub(super) fn text_input(ui: &mut Ui, label: &str, value: &mut String) {
    ui.label(label);
    ui.add(egui::TextEdit::singleline(value).desired_width(ui.available_width()));
}

pub(super) fn hint(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).color(Tokens::get(ui.ctx()).text_dim).size(11.0));
}
