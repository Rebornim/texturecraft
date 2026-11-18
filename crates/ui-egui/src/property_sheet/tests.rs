use egui::{Color32, CornerRadius, Painter, Rect};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_phosphor::regular as ic;

use super::number_field::shown_number;
use super::slot::next_seed;
use super::{NumberField, PropertySheet};
use crate::theme::{Accent, Brightness, Tokens};

mod filtering;
mod gallery;

use filtering::Filtering;
use gallery::Gallery;

/// Width of the Properties panel the mockups are drawn at
const PANEL_WIDTH: f32 = 300.0;

/// The harness window: the panel plus room on its right for popups and tips, tall enough for the whole gallery unscrolled
const HARNESS_SIZE: egui::Vec2 = egui::vec2(PANEL_WIDTH + 260.0, 1000.0);

/// The i of the gallery's Depth Culling row
const DEPTH_CULLING_TIP: &str = "Paints only the surface the projector sees first";

fn swatch(color: Color32) -> impl Fn(&Painter, Rect) {
    move |painter: &Painter, rect: Rect| {
        painter.rect_filled(rect, CornerRadius::ZERO, color);
    }
}

fn filtering_picture(painter: &Painter, rect: Rect, filtering: Filtering) {
    let shade = match filtering {
        Filtering::BilinearHq => 200,
        Filtering::BilinearSharp => 150,
        Filtering::Nearest => 100,
    };
    painter.rect_filled(rect.shrink(2.0), CornerRadius::same(2), Color32::from_gray(shade));
}

fn gallery(ui: &mut egui::Ui, state: &mut Gallery) {
    let panel = Tokens::get(ui.ctx()).panel;
    egui::Frame::NONE.fill(panel).show(ui, |ui| {
        ui.set_width(PANEL_WIDTH);
        PropertySheet::show(ui, "gallery", |sheet| {
            sheet.section("Fill", true);
            if let Some(scale) = sheet.switch("Scale", &["Tiling", "Physical size"], state.scale) {
                state.scale = scale;
            }
            if let Some(picked) = sheet
                .dropdown(
                    "Filtering",
                    state.filtering,
                    &[Filtering::BilinearHq, Filtering::BilinearSharp, Filtering::Nearest],
                    Filtering::name,
                    Some(&filtering_picture),
                )
                .0
            {
                state.filtering = picked;
            }
            if let Some(merge) = sheet.slider("Hardness", &mut state.hardness, 0.0..=1.0, 2) {
                state.hardness_merges.push(merge);
            }
            sheet.signed_slider("Offset", &mut state.offset, -1.0..=1.0, 2);
            sheet.angle("Rotation", &mut state.rotation, 0.0..=360.0);
            let names = ["Projection Extends Outside Shape", "Projection Is Cropped To Shape"];
            if let Some(picked) = sheet
                .dropdown(
                    "A label wider than the column's cap",
                    state.long_choice,
                    &[0, 1],
                    |index| names.get(index).copied().unwrap_or_default().to_owned(),
                    None,
                )
                .0
            {
                state.long_choice = picked;
            }

            let reset = sheet.band("3D projection settings", true, Some(state.position != [0.0; 3]), None);
            if reset.reset {
                state.position = [0.0; 3];
            }
            if reset.open {
                let mut fields = [("X", 0), ("Y", 1), ("Z", 2)].map(|(tag, axis)| NumberField {
                    tag,
                    tag_color: crate::theme::AXIS_COLORS.get(axis).copied(),
                    value: state.position[axis],
                    speed: 0.01,
                    decimals: 2,
                    suffix: "",
                });
                if let Some((axis, merge)) = sheet.number_fields("Position", &mut fields, Some(&mut state.linked), false)
                    && let Some(slot) = state.position.get_mut(axis)
                {
                    *slot = fields[axis].value;
                    state.position_merges.push(merge);
                }
            }

            if sheet.card("Edges", Some(true)) {
                sheet.toggle("Wrap", &mut state.wrap);
                sheet.toggle("Shape Crop", &mut state.crop);
                sheet.info(DEPTH_CULLING_TIP);
                sheet.picture_toggle(&swatch(Color32::from_gray(160)), "Depth Culling", &mut state.depth_culling);
                sheet.picture_toggle(&swatch(Color32::from_gray(100)), "Backface Culling", &mut state.backface_culling);
            }
            sheet.slot(&swatch(Color32::from_rgb(0xb0, 0x5a, 0x3a)), "Base color", "Uniform color", Some(&mut state.channel_on), false, false);
            sheet.slider("Value", &mut state.channel_value, 0.0..=1.0, 2);

            sheet.section("Filter", false);
            if let Some((index, _)) = sheet.channel_pills(&["Color", "Metal", "Rough", "Normal", "Height"], &state.channels)
                && let Some(on) = state.channels.get_mut(index)
            {
                *on = !*on;
            }
            let (_, chip) = sheet.slider_with_chip("Intensity", &mut state.intensity, 0.0..=1.0, 2, &format!("÷{}", state.divider), "÷1000");
            let dividers = ["1", "10", "100", "1000"];
            if let Some(picked) = sheet.hanging_list(&chip, state.divider, &dividers, &|divider: &str| format!("÷{divider}"), None).0 {
                state.divider = picked;
            }
            let band = sheet.band("Parameters", true, None, Some("3"));
            if band.open {
                sheet.caption("Offset");
                sheet.signed_slider("Horizontal", &mut state.offset, -1.0..=1.0, 2);
                if sheet.fold("Attributes", Some("Default"), false) {
                    sheet.empty_note("No attributes");
                }
                sheet.seed("Seed", &mut state.seed);
                if sheet.button(ic::DICE_FIVE, "Randomize seeds").clicked() {
                    state.randomized += 1;
                }
            }
        });
    });
}

/// A harness in `brightness`, with the app's fonts from the first frame
fn builder<State>(brightness: Brightness) -> egui_kittest::HarnessBuilder<State> {
    Harness::builder().with_size(HARNESS_SIZE).with_context_setup(move |ctx| {
        crate::theme::install(ctx);
        crate::theme::apply(ctx, brightness, Accent::default());
    })
}

fn gallery_harness(brightness: Brightness, gallery_state: Gallery) -> Harness<'static, Gallery> {
    let mut harness = builder(brightness).build_ui_state(gallery, gallery_state);
    harness.run_steps(3);
    harness
}

#[test]
fn toggles_switches_and_pills_change_their_state() {
    let _language = crate::i18n::TEST_LANGUAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut harness = gallery_harness(Brightness::MediumDark, Gallery::default());
    let at = harness.get_by_label("Shape Crop").rect().center();
    click_at(&mut harness, at);
    let at = harness.get_by_label("Physical size").rect().center();
    click_at(&mut harness, at);
    let at = harness.get_by_label("Normal").rect().center();
    click_at(&mut harness, at);
    harness.run_steps(2);

    assert!(harness.state().crop);
    assert_eq!(harness.state().scale, 1);
    assert_eq!(harness.state().channels, [true, true, true, true, true]);
}

/// A culling row sits on the panel's grid like a plain toggle: its name on the label column, its picture and switch where the controls start, its i at the row's end
#[test]
fn a_culling_row_reads_name_then_picture_and_switch_with_its_i_at_the_end() {
    let _language = crate::i18n::TEST_LANGUAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut harness = gallery_harness(Brightness::MediumDark, Gallery::default());
    let culling = harness.get_by_label("Depth Culling").rect();
    let plain = harness.get_by_label("Wrap").rect();
    let info = harness.get_by_label(DEPTH_CULLING_TIP).rect();
    assert_eq!(culling.left(), plain.left(), "the name starts where a plain toggle's does: {culling:?} vs {plain:?}");
    assert!(info.left() > culling.right(), "the i sits after the switch, at the row's end: {info:?} vs {culling:?}");

    let was = harness.state().depth_culling;
    let at = harness.get_by_label("Depth Culling").rect().center();
    click_at(&mut harness, at);
    harness.run_steps(2);
    assert_ne!(harness.state().depth_culling, was, "a click on the row flips it");
}

#[test]
fn dropdown_list_hangs_from_its_button_at_its_width_and_picks() {
    let _language = crate::i18n::TEST_LANGUAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut harness = gallery_harness(Brightness::MediumDark, Gallery::default());
    let button = harness.get_by_label("Bilinear | HQ").rect();
    let at = harness.get_by_label("Bilinear | HQ").rect().center();
    click_at(&mut harness, at);
    harness.run_steps(2);

    let option = harness.get_by_label("Nearest").rect();
    let list = option.expand(super::choice::LIST_BORDER);
    assert_eq!(list.left(), button.left(), "the list starts at the button's left edge");
    assert_eq!(list.width(), button.width(), "the list is the button's width");
    assert!(option.top() > button.bottom(), "the list hangs below the button");

    let at = harness.get_by_label("Nearest").rect().center();
    click_at(&mut harness, at);
    harness.run_steps(2);
    assert_eq!(harness.state().filtering, Filtering::Nearest);
}

/// Presses and lets go at `at` through the real pointer path, so a control drawn over another gets the click it would get from a person
fn click_at<State>(harness: &mut Harness<'_, State>, at: egui::Pos2) {
    let modifiers = egui::Modifiers::NONE;
    harness.event(egui::Event::PointerMoved(at));
    harness.event(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: true, modifiers });
    harness.run_steps(1);
    harness.event(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: false, modifiers });
    harness.run_steps(1);
}

/// Clicks at `at`, then types `text` into the field that opened and presses Enter
fn type_at(harness: &mut Harness<'_, Gallery>, at: egui::Pos2, text: &str) {
    harness.drag_at(at);
    harness.drop_at(at);
    harness.run_steps(2);
    harness.get_by_role(egui::Role::TextInput).type_text(text);
    harness.run_steps(1);
    harness.key_press(egui::Key::Enter);
    harness.run_steps(2);
}

#[test]
fn a_click_on_a_value_or_the_seed_selects_it_so_typing_replaces_it() {
    let _language = crate::i18n::TEST_LANGUAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut harness = gallery_harness(Brightness::MediumDark, Gallery::default());
    let value = harness.get_by_label("Hardness value").rect();
    type_at(&mut harness, value.center(), "0.4");
    assert_eq!(harness.state().hardness, 0.4, "the slider's value is replaced, not appended to");

    let seed = harness.get_by_label("Seed value").rect();
    type_at(&mut harness, seed.center(), "77");
    assert_eq!(harness.state().seed, 77, "the seed is replaced, not appended to");
}

/// Clicks at `at` to type, then presses Enter without typing
fn open_and_leave_at(harness: &mut Harness<'_, Gallery>, at: egui::Pos2) {
    harness.drag_at(at);
    harness.drop_at(at);
    harness.run_steps(2);
    harness.key_press(egui::Key::Enter);
    harness.run_steps(2);
}

/// A value opened to type and left as it opened is not an edit: no undo step, and its hidden digits are not rounded away
#[test]
fn a_value_opened_and_left_untyped_changes_nothing() {
    let _language = crate::i18n::TEST_LANGUAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut harness = gallery_harness(Brightness::MediumDark, Gallery { hardness: 0.333, position: [1.2345, 0.0, 0.0], ..Gallery::default() });
    let hardness = harness.get_by_label("Hardness value").rect();
    let position = harness.get_by_label("Position X").rect();

    open_and_leave_at(&mut harness, hardness.center());
    open_and_leave_at(&mut harness, position.center());

    assert_eq!(harness.state().hardness, 0.333, "the slider keeps its unrounded value");
    assert!(harness.state().hardness_merges.is_empty(), "the slider reports no edit: {:?}", harness.state().hardness_merges);
    assert_eq!(harness.state().position[0], 1.2345, "the number field keeps its unrounded value");
    assert!(harness.state().position_merges.is_empty(), "the number field reports no edit: {:?}", harness.state().position_merges);
}

/// A typed value that lands on the current one is no edit: one past the end kept at the end, or the same number written another way
#[test]
fn a_typed_value_equal_to_the_current_one_changes_nothing() {
    let _language = crate::i18n::TEST_LANGUAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut harness = gallery_harness(Brightness::MediumDark, Gallery { hardness: 1.0, ..Gallery::default() });
    let hardness = harness.get_by_label("Hardness value").rect();
    type_at(&mut harness, hardness.center(), "5");
    assert_eq!(harness.state().hardness, 1.0, "a value past the end is kept at the end");
    assert!(harness.state().hardness_merges.is_empty(), "kept at the value it had, it reports no edit: {:?}", harness.state().hardness_merges);

    harness.state_mut().hardness = 0.75;
    harness.run_steps(1);
    type_at(&mut harness, hardness.center(), ".750");
    assert_eq!(harness.state().hardness, 0.75);
    assert!(harness.state().hardness_merges.is_empty(), "the same number written another way reports no edit: {:?}", harness.state().hardness_merges);
}

#[test]
fn a_drag_merges_after_its_first_step_and_a_click_does_not() {
    let _language = crate::i18n::TEST_LANGUAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut harness = gallery_harness(Brightness::MediumDark, Gallery::default());
    let track = harness.get_by_label("Hardness").rect();
    harness.drag_at(track.center());
    harness.run();
    harness.hover_at(track.center() + egui::vec2(20.0, 0.0));
    harness.run();
    harness.hover_at(track.center() + egui::vec2(40.0, 0.0));
    harness.run();
    harness.drop_at(track.center() + egui::vec2(40.0, 0.0));
    harness.run();
    let drag_merges = harness.state().hardness_merges.clone();

    let click = track.left_center() + egui::vec2(20.0, 0.0);
    harness.drag_at(click);
    harness.drop_at(click);
    harness.run();

    assert_eq!(drag_merges.first(), Some(&false), "the drag's first change starts an undo step: {drag_merges:?}");
    assert!(drag_merges.iter().skip(1).all(|merge| *merge), "later changes continue it: {drag_merges:?}");
    let merges = &harness.state().hardness_merges;
    assert_eq!(merges.len(), drag_merges.len() + 1, "the click changes the value once: {merges:?}");
    assert_eq!(merges.last(), Some(&false), "the click starts its own undo step: {merges:?}");
}

/// A drag whose first frames change nothing (the knob held at its end) still starts its own undo step with its first change
#[test]
fn a_drag_pressed_past_the_tracks_end_starts_an_undo_step_with_its_first_change() {
    let _language = crate::i18n::TEST_LANGUAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut harness = gallery_harness(Brightness::MediumDark, Gallery { hardness: 1.0, ..Gallery::default() });
    let track = harness.get_by_label("Hardness").rect();
    let past_end = track.right_center() - egui::vec2(1.0, 0.0);
    harness.drag_at(past_end);
    harness.run();
    harness.hover_at(past_end + egui::vec2(20.0, 0.0));
    harness.run();
    harness.hover_at(track.center());
    harness.run();
    harness.hover_at(track.center() - egui::vec2(20.0, 0.0));
    harness.run();
    harness.drop_at(track.center() - egui::vec2(20.0, 0.0));
    harness.run();

    let merges = &harness.state().hardness_merges;
    assert_eq!(merges.first(), Some(&false), "the first real change starts an undo step: {merges:?}");
    assert!(merges.iter().skip(1).all(|merge| *merge), "later changes continue it: {merges:?}");
}

/// A number field held still until egui starts its drag, then moved, starts its own undo step with its first change
#[test]
fn a_number_field_held_still_then_dragged_starts_an_undo_step_with_its_first_change() {
    let _language = crate::i18n::TEST_LANGUAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut harness = gallery_harness(Brightness::MediumDark, Gallery::default());
    let field = harness.get_by_label("Position X").rect();
    harness.drag_at(field.center());

    // Past egui's longest click, the press becomes a drag that has not moved
    harness.run_steps(5);
    harness.hover_at(field.center() + egui::vec2(10.0, 0.0));
    harness.run();
    harness.hover_at(field.center() + egui::vec2(20.0, 0.0));
    harness.run();
    harness.drop_at(field.center() + egui::vec2(20.0, 0.0));
    harness.run();

    let merges = &harness.state().position_merges;
    assert_eq!(merges.first(), Some(&false), "the first move starts an undo step: {merges:?}");
    assert!(merges.iter().skip(1).all(|merge| *merge), "later moves continue it: {merges:?}");
}

/// A ramp with one marker at `state.0`, recording each marker drag's merge in `state.1`
fn one_marker_ramp(ui: &mut egui::Ui, state: &mut (f32, Vec<bool>)) {
    ui.set_width(PANEL_WIDTH);
    PropertySheet::show(ui, "ramp", |sheet| {
        if let Some(drag) = sheet.gradient_ramp(&[[1.0, 0.0, 0.0]], &[state.0], &|_| [0.5; 3]) {
            state.0 = drag.position;
            state.1.push(drag.merge);
        }
    });
}

/// A marker pressed past the ramp's end changes nothing at first; its first move inward still starts its own undo step
#[test]
fn a_gradient_marker_pressed_past_the_ramps_end_starts_an_undo_step_with_its_first_change() {
    let _language = crate::i18n::TEST_LANGUAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut harness = builder(Brightness::MediumDark).build_ui_state(one_marker_ramp, (1.0, Vec::new()));
    harness.run_steps(2);
    let marker = harness.get_by_label("Color 1").rect();
    let past_end = marker.right_center() - egui::vec2(1.0, 0.0);
    harness.drag_at(past_end);
    harness.run();
    harness.hover_at(past_end + egui::vec2(20.0, 0.0));
    harness.run();
    harness.hover_at(past_end - egui::vec2(40.0, 0.0));
    harness.run();
    harness.hover_at(past_end - egui::vec2(60.0, 0.0));
    harness.run();
    harness.drop_at(past_end - egui::vec2(60.0, 0.0));
    harness.run();

    let merges = &harness.state().1;
    assert_eq!(merges.first(), Some(&false), "the first real change starts an undo step: {merges:?}");
    assert!(merges.iter().skip(1).all(|merge| *merge), "later changes continue it: {merges:?}");
}

#[test]
fn the_label_column_fits_the_widest_short_label_and_long_labels_stack() {
    let _language = crate::i18n::TEST_LANGUAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let harness = gallery_harness(Brightness::MediumDark, Gallery::default());
    let hardness = harness.get_by_label("Hardness").rect();
    let filtering = harness.get_by_label("Bilinear | HQ").rect();
    let crop = harness.get_by_label("Projection Extends Outside Shape").rect();

    let column = filtering.left();
    assert!((hardness.left() - column).abs() < 0.5, "sliders start on the controls column");
    assert!(crop.left() < column, "a label wider than the cap stacks above a full-width control");
}

#[test]
fn reset_restores_the_band_and_the_seed_dice_steps_deterministically() {
    let _language = crate::i18n::TEST_LANGUAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut harness = gallery_harness(Brightness::MediumDark, Gallery::default());
    let at = harness.get_by_label("Random").rect().center();
    click_at(&mut harness, at);
    let at = harness.get_by_label("Restore defaults").rect().center();
    click_at(&mut harness, at);
    let at = harness.get_by_label("Randomize seeds").rect().center();
    click_at(&mut harness, at);
    harness.run_steps(2);

    assert_eq!(harness.state().seed, next_seed(1234));
    assert_eq!(harness.state().position, [0.0; 3]);
    assert_eq!(harness.state().randomized, 1);
}

/// The gallery in every theme, closed and with a list open, for comparing with the mockups
#[test]
#[ignore]
fn property_sheet_gallery_snapshots() {
    let _language = crate::i18n::TEST_LANGUAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(output_dir) = std::env::var_os("TC_UI_SNAPSHOTS").map(std::path::PathBuf::from) else { return };

    std::fs::create_dir_all(&output_dir).unwrap();
    for (brightness, name) in [(Brightness::MediumDark, "medium"), (Brightness::Dark, "dark"), (Brightness::Light, "light")] {
        let mut harness = builder(brightness).with_pixels_per_point(2.0).wgpu().build_ui_state(gallery, Gallery::default());
        harness.run_steps(4);
        harness.render().unwrap().save(output_dir.join(format!("sheet-{name}.png"))).unwrap();

        let at = harness.get_by_label("Bilinear | HQ").rect().center();
        click_at(&mut harness, at);
        harness.run_steps(2);
        harness.get_by_label("Bilinear | Sharp").hover();
        harness.run_steps(2);
        harness.render().unwrap().save(output_dir.join(format!("sheet-{name}-list-open.png"))).unwrap();
        harness.key_press(egui::Key::Escape);
        harness.run_steps(2);

        // A value: the grey line on hover, then typing on that line in accent; the seed's box lights the same way
        let value = harness.get_by_label("Hardness value").rect().center();
        harness.hover_at(value);
        harness.run_steps(2);
        harness.render().unwrap().save(output_dir.join(format!("sheet-{name}-value-hover.png"))).unwrap();
        harness.drag_at(value);
        harness.drop_at(value);
        harness.run_steps(2);
        harness.get_by_role(egui::Role::TextInput).type_text("0.4");
        harness.run_steps(2);
        harness.render().unwrap().save(output_dir.join(format!("sheet-{name}-value-typing.png"))).unwrap();
        harness.key_press(egui::Key::Escape);
        harness.run_steps(2);
        let seed = harness.get_by_label("Seed value").rect().center();
        harness.hover_at(seed);
        harness.run_steps(2);
        harness.render().unwrap().save(output_dir.join(format!("sheet-{name}-seed-hover.png"))).unwrap();
        harness.drag_at(seed);
        harness.drop_at(seed);
        harness.run_steps(2);
        harness.render().unwrap().save(output_dir.join(format!("sheet-{name}-seed-typing.png"))).unwrap();
    }
}

#[test]
fn a_number_field_reads_zero_without_a_minus_sign() {
    assert_eq!(shown_number(-0.0, 0), "0");
    assert_eq!(shown_number(-0.004, 2), "0.00");
    assert_eq!(shown_number(-0.3, 0), "0");
    assert_eq!(shown_number(-0.6, 0), "-1");
    assert_eq!(shown_number(-1.25, 2), "-1.25");
    assert_eq!(shown_number(2.5, 1), "2.5");
}

/// Folds keep their own open state when a fold above them comes and goes (the state follows the name, not the row's place)
#[test]
fn a_fold_keeps_its_state_when_rows_above_it_come_and_go() {
    let _language = crate::i18n::TEST_LANGUAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let folds = |ui: &mut egui::Ui, extra_shown: &mut bool| {
        PropertySheet::show(ui, "folds", |sheet| {
            if *extra_shown && sheet.fold("Extra", None, true) {
                sheet.empty_note("Extra rows");
            }
            if sheet.fold("Edges", None, true) {
                sheet.empty_note("Edge rows");
            }
            if sheet.fold("Parameters", None, false) {
                sheet.empty_note("Parameter rows");
            }
        });
    };
    let mut harness = builder(Brightness::MediumDark).build_ui_state(folds, true);
    harness.run_steps(2);
    let at = harness.get_by_label("Edges").rect().center();
    click_at(&mut harness, at);
    harness.run_steps(2);
    assert!(harness.query_by_label("Edge rows").is_none());

    *harness.state_mut() = false;
    harness.run_steps(2);

    assert!(harness.query_by_label("Edge rows").is_none(), "Edges stays folded");
    assert!(harness.query_by_label("Parameter rows").is_none(), "Parameters stays folded");
}

/// Two colour bars, one under the other, each opening its own picker
fn two_color_bars(ui: &mut egui::Ui) {
    PropertySheet::show(ui, "color_bars", |sheet| {
        sheet.color_bar("Bar A", [0.2, 0.4, 0.6], None, &[]);
        sheet.color_bar("Bar B", [0.6, 0.4, 0.2], None, &[]);
    });
}

/// The colour space picked in one picker is the one the next picker opens in, even for a bar drawn before it all along
#[test]
fn the_next_picker_opens_in_the_colour_space_last_picked() {
    let _language = crate::i18n::TEST_LANGUAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut harness = builder(Brightness::MediumDark).build_ui(two_color_bars);
    harness.run_steps(2);
    let at = harness.get_by_role_and_label(egui::Role::Button, "Bar B").rect().center();
    click_at(&mut harness, at);
    harness.run_steps(2);
    let at = harness.get_by_role_and_label(egui::Role::ComboBox, "sRGB").rect().center();
    click_at(&mut harness, at);
    harness.run_steps(2);
    let at = harness.get_by_role_and_label(egui::Role::ListBoxOption, "Linear").rect().center();
    click_at(&mut harness, at);
    harness.run_steps(2);
    assert!(harness.query_by_role_and_label(egui::Role::ComboBox, "Linear").is_some(), "Bar B's picker shows Linear");

    let at = harness.get_by_role_and_label(egui::Role::Button, "Close").rect().center();
    click_at(&mut harness, at);
    harness.run_steps(2);
    let at = harness.get_by_role_and_label(egui::Role::Button, "Bar A").rect().center();
    click_at(&mut harness, at);
    harness.run_steps(2);

    assert!(harness.query_by_role_and_label(egui::Role::TitleBar, "Bar A").is_some(), "Bar A's picker is open");
    assert!(harness.query_by_role_and_label(egui::Role::ComboBox, "Linear").is_some(), "Bar A's picker opens in Linear");
}

/// A colour bar's picker stays open when a colour bar above it comes and goes (its state follows its name, not its place)
#[test]
fn a_color_bars_picker_stays_open_when_a_row_above_it_goes_away() {
    let _language = crate::i18n::TEST_LANGUAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let bars = |ui: &mut egui::Ui, extra_shown: &mut bool| {
        PropertySheet::show(ui, "color_bars", |sheet| {
            if *extra_shown {
                sheet.color_bar("Extra", [0.5; 3], None, &[]);
            }
            sheet.color_bar("Tint", [0.2, 0.4, 0.6], None, &[]);
        });
    };
    let mut harness = builder(Brightness::MediumDark).build_ui_state(bars, true);
    harness.run_steps(2);
    let at = harness.get_by_role_and_label(egui::Role::Button, "Tint").rect().center();
    click_at(&mut harness, at);
    harness.run_steps(2);
    assert!(harness.query_by_role_and_label(egui::Role::TitleBar, "Tint").is_some(), "the picker opened");

    *harness.state_mut() = false;
    harness.run_steps(2);

    assert!(harness.query_by_role_and_label(egui::Role::TitleBar, "Tint").is_some(), "the picker stays open");
}

/// A fold under a later section, before that section's first band, keeps its state when a band of the section above comes and goes
#[test]
fn a_fold_before_a_sections_first_band_keeps_its_state_when_the_band_above_goes_away() {
    let _language = crate::i18n::TEST_LANGUAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let sections = |ui: &mut egui::Ui, band_shown: &mut bool| {
        PropertySheet::show(ui, "sections", |sheet| {
            sheet.section("First", false);
            if *band_shown && sheet.band("Band", true, None, None).open {
                sheet.empty_note("Band rows");
            }
            sheet.section("Second", false);
            if sheet.fold("Edges", None, true) {
                sheet.empty_note("Edge rows");
            }
        });
    };
    let mut harness = builder(Brightness::MediumDark).build_ui_state(sections, true);
    harness.run_steps(2);
    let at = harness.get_by_label("Edges").rect().center();
    click_at(&mut harness, at);
    harness.run_steps(2);
    assert!(harness.query_by_label("Edge rows").is_none());

    *harness.state_mut() = false;
    harness.run_steps(2);

    assert!(harness.query_by_label("Edge rows").is_none(), "Edges stays folded");
}
