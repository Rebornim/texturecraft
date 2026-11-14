use egui::{Color32, CornerRadius, Painter, Rect};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_phosphor::regular as ic;

use super::number_field::shown_number;
use super::slot::next_seed;
use super::{NumberField, PropertySheet};
use crate::theme::{Accent, Brightness, Tokens};

/// Width of the Properties panel the mockups are drawn at
const PANEL_WIDTH: f32 = 300.0;

/// The harness window: the panel plus room on its right for popups and tips, tall enough for the whole gallery unscrolled
const HARNESS_SIZE: egui::Vec2 = egui::vec2(PANEL_WIDTH + 260.0, 1000.0);

#[derive(Clone, Copy, PartialEq, Debug)]
enum Filtering {
    BilinearHq,
    BilinearSharp,
    Nearest,
}

impl Filtering {
    fn name(self) -> String {
        match self {
            Filtering::BilinearHq => "Bilinear | HQ",
            Filtering::BilinearSharp => "Bilinear | Sharp",
            Filtering::Nearest => "Nearest",
        }
        .to_owned()
    }
}

/// Every control of the shared set once, with the values the gallery last set
struct Gallery {
    scale: usize,
    filtering: Filtering,
    hardness: f32,
    hardness_merges: Vec<bool>,
    offset: f32,
    rotation: f32,
    position: [f32; 3],
    position_merges: Vec<bool>,
    linked: bool,
    wrap: bool,
    crop: bool,
    depth_culling: bool,
    backface_culling: bool,
    channel_on: bool,
    channels: [bool; 5],
    channel_value: f32,
    intensity: f32,
    divider: &'static str,
    seed: u32,
    randomized: u32,
    long_choice: usize,
}

impl Default for Gallery {
    fn default() -> Self {
        Gallery {
            scale: 0,
            filtering: Filtering::BilinearHq,
            hardness: 0.75,
            hardness_merges: Vec::new(),
            offset: -0.25,
            rotation: 30.0,
            position: [0.0, 1.25, -0.5],
            position_merges: Vec::new(),
            linked: true,
            wrap: true,
            crop: false,
            depth_culling: true,
            backface_culling: false,
            channel_on: true,
            channels: [true, true, true, false, true],
            channel_value: 0.8,
            intensity: 0.5,
            divider: "1",
            seed: 1234,
            randomized: 0,
            long_choice: 0,
        }
    }
}

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

fn gallery(ui: &mut egui::Ui, g: &mut Gallery) {
    let panel = Tokens::get(ui.ctx()).panel;
    egui::Frame::NONE.fill(panel).show(ui, |ui| {
        ui.set_width(PANEL_WIDTH);
        PropertySheet::show(ui, "gallery", |s| {
            s.section("Fill", true);
            if let Some(scale) = s.switch("Scale", &["Tiling", "Physical size"], g.scale) {
                g.scale = scale;
            }
            if let Some(picked) = s
                .dropdown(
                    "Filtering",
                    g.filtering,
                    &[Filtering::BilinearHq, Filtering::BilinearSharp, Filtering::Nearest],
                    Filtering::name,
                    Some(&filtering_picture),
                )
                .0
            {
                g.filtering = picked;
            }
            if let Some(merge) = s.slider("Hardness", &mut g.hardness, 0.0..=1.0, 2) {
                g.hardness_merges.push(merge);
            }
            s.signed_slider("Offset", &mut g.offset, -1.0..=1.0, 2);
            s.angle("Rotation", &mut g.rotation, 0.0..=360.0);
            let names = ["Projection Extends Outside Shape", "Projection Is Cropped To Shape"];
            if let Some(picked) =
                s.dropdown("A label wider than the column's cap", g.long_choice, &[0, 1], |k| names.get(k).copied().unwrap_or_default().to_owned(), None).0
            {
                g.long_choice = picked;
            }

            let reset = s.band("3D projection settings", true, Some(g.position != [0.0; 3]), None);
            if reset.reset {
                g.position = [0.0; 3];
            }
            if reset.open {
                let mut fields = [("X", 0), ("Y", 1), ("Z", 2)].map(|(tag, axis)| NumberField {
                    tag,
                    tag_color: crate::theme::AXIS_COLORS.get(axis).copied(),
                    value: g.position[axis],
                    speed: 0.01,
                    decimals: 2,
                    suffix: "",
                });
                if let Some((axis, merge)) = s.number_fields("Position", &mut fields, Some(&mut g.linked), false)
                    && let Some(slot) = g.position.get_mut(axis)
                {
                    *slot = fields[axis].value;
                    g.position_merges.push(merge);
                }
            }

            if s.card("Edges", Some(true)) {
                s.toggle("Wrap", &mut g.wrap);
                s.toggle("Shape Crop", &mut g.crop);
                s.info(DEPTH_CULLING_TIP);
                s.picture_toggle(&swatch(Color32::from_gray(160)), "Depth Culling", &mut g.depth_culling);
                s.picture_toggle(&swatch(Color32::from_gray(100)), "Backface Culling", &mut g.backface_culling);
            }
            s.slot(&swatch(Color32::from_rgb(0xb0, 0x5a, 0x3a)), "Base color", "Uniform color", Some(&mut g.channel_on), false, false);
            s.slider("Value", &mut g.channel_value, 0.0..=1.0, 2);

            s.section("Filter", false);
            if let Some((index, _)) = s.channel_pills(&["Color", "Metal", "Rough", "Normal", "Height"], &g.channels)
                && let Some(on) = g.channels.get_mut(index)
            {
                *on = !*on;
            }
            let (_, chip) = s.slider_with_chip("Intensity", &mut g.intensity, 0.0..=1.0, 2, &format!("÷{}", g.divider), "÷1000");
            let dividers = ["1", "10", "100", "1000"];
            if let Some(picked) = s.hanging_list(&chip, g.divider, &dividers, &|d: &str| format!("÷{d}"), None).0 {
                g.divider = picked;
            }
            let band = s.band("Parameters", true, None, Some("3"));
            if band.open {
                s.caption("Offset");
                s.signed_slider("Horizontal", &mut g.offset, -1.0..=1.0, 2);
                if s.fold("Attributes", Some("Default"), false) {
                    s.empty_note("No attributes");
                }
                s.seed("Seed", &mut g.seed);
                if s.button(ic::DICE_FIVE, "Randomize seeds").clicked() {
                    g.randomized += 1;
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

fn harness(brightness: Brightness, gallery_state: Gallery) -> Harness<'static, Gallery> {
    let mut harness = builder(brightness).build_ui_state(gallery, gallery_state);
    harness.run_steps(3);
    harness
}

#[test]
fn toggles_switches_and_pills_change_their_state() {
    let _language = crate::i18n::TEST_LANGUAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut h = harness(Brightness::MediumDark, Gallery::default());
    h.get_by_label("Shape Crop").click();
    h.get_by_label("Physical size").click();
    h.get_by_label("Normal").click();
    h.run_steps(2);

    assert!(h.state().crop);
    assert_eq!(h.state().scale, 1);
    assert_eq!(h.state().channels, [true, true, true, true, true]);
}

/// A culling row sits on the panel's grid like a plain toggle: its name on the label column, its picture and switch where the controls start, its i at the row's end
#[test]
fn a_culling_row_reads_name_then_picture_and_switch_with_its_i_at_the_end() {
    let _language = crate::i18n::TEST_LANGUAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut h = harness(Brightness::MediumDark, Gallery::default());
    let culling = h.get_by_label("Depth Culling").rect();
    let plain = h.get_by_label("Wrap").rect();
    let info = h.get_by_label(DEPTH_CULLING_TIP).rect();
    assert_eq!(culling.left(), plain.left(), "the name starts where a plain toggle's does: {culling:?} vs {plain:?}");
    assert!(info.left() > culling.right(), "the i sits after the switch, at the row's end: {info:?} vs {culling:?}");

    let was = h.state().depth_culling;
    h.get_by_label("Depth Culling").click();
    h.run_steps(2);
    assert_ne!(h.state().depth_culling, was, "a click on the row flips it");
}

#[test]
fn dropdown_list_hangs_from_its_button_at_its_width_and_picks() {
    let _language = crate::i18n::TEST_LANGUAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut h = harness(Brightness::MediumDark, Gallery::default());
    let button = h.get_by_label("Bilinear | HQ").rect();
    h.get_by_label("Bilinear | HQ").click();
    h.run_steps(2);

    let option = h.get_by_label("Nearest").rect();
    let list = option.expand(super::choice::LIST_BORDER);
    assert_eq!(list.left(), button.left(), "the list starts at the button's left edge");
    assert_eq!(list.width(), button.width(), "the list is the button's width");
    assert!(option.top() > button.bottom(), "the list hangs below the button");

    h.get_by_label("Nearest").click();
    h.run_steps(2);
    assert_eq!(h.state().filtering, Filtering::Nearest);
}

/// Clicks at `at`, then types `text` into the field that opened and presses Enter
fn type_at(h: &mut Harness<'_, Gallery>, at: egui::Pos2, text: &str) {
    h.drag_at(at);
    h.drop_at(at);
    h.run_steps(2);
    h.get_by_role(egui::Role::TextInput).type_text(text);
    h.run_steps(1);
    h.key_press(egui::Key::Enter);
    h.run_steps(2);
}

#[test]
fn a_click_on_a_value_or_the_seed_selects_it_so_typing_replaces_it() {
    let _language = crate::i18n::TEST_LANGUAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut h = harness(Brightness::MediumDark, Gallery::default());
    let value = h.get_by_label("Hardness value").rect();
    type_at(&mut h, value.center(), "0.4");
    assert_eq!(h.state().hardness, 0.4, "the slider's value is replaced, not appended to");

    let seed = h.get_by_label("Seed value").rect();
    type_at(&mut h, seed.center(), "77");
    assert_eq!(h.state().seed, 77, "the seed is replaced, not appended to");
}

#[test]
fn a_drag_merges_after_its_first_step_and_a_click_does_not() {
    let _language = crate::i18n::TEST_LANGUAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut h = harness(Brightness::MediumDark, Gallery::default());
    let track = h.get_by_label("Hardness").rect();
    h.drag_at(track.center());
    h.run();
    h.hover_at(track.center() + egui::vec2(20.0, 0.0));
    h.run();
    h.hover_at(track.center() + egui::vec2(40.0, 0.0));
    h.run();
    h.drop_at(track.center() + egui::vec2(40.0, 0.0));
    h.run();
    let drag_merges = h.state().hardness_merges.clone();

    let click = track.left_center() + egui::vec2(20.0, 0.0);
    h.drag_at(click);
    h.drop_at(click);
    h.run();

    assert_eq!(drag_merges.first(), Some(&false), "the drag's first change starts an undo step: {drag_merges:?}");
    assert!(drag_merges.iter().skip(1).all(|merge| *merge), "later changes continue it: {drag_merges:?}");
    let merges = &h.state().hardness_merges;
    assert_eq!(merges.len(), drag_merges.len() + 1, "the click changes the value once: {merges:?}");
    assert_eq!(merges.last(), Some(&false), "the click starts its own undo step: {merges:?}");
}

/// A drag whose first frames change nothing (the knob held at its end) still starts its own undo step with its first change
#[test]
fn a_drag_pressed_past_the_tracks_end_starts_an_undo_step_with_its_first_change() {
    let _language = crate::i18n::TEST_LANGUAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut h = harness(Brightness::MediumDark, Gallery { hardness: 1.0, ..Gallery::default() });
    let track = h.get_by_label("Hardness").rect();
    let past_end = track.right_center() - egui::vec2(1.0, 0.0);
    h.drag_at(past_end);
    h.run();
    h.hover_at(past_end + egui::vec2(20.0, 0.0));
    h.run();
    h.hover_at(track.center());
    h.run();
    h.hover_at(track.center() - egui::vec2(20.0, 0.0));
    h.run();
    h.drop_at(track.center() - egui::vec2(20.0, 0.0));
    h.run();

    let merges = &h.state().hardness_merges;
    assert_eq!(merges.first(), Some(&false), "the first real change starts an undo step: {merges:?}");
    assert!(merges.iter().skip(1).all(|merge| *merge), "later changes continue it: {merges:?}");
}

/// A number field held still until egui starts its drag, then moved, starts its own undo step with its first change
#[test]
fn a_number_field_held_still_then_dragged_starts_an_undo_step_with_its_first_change() {
    let _language = crate::i18n::TEST_LANGUAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut h = harness(Brightness::MediumDark, Gallery::default());
    let field = h.get_by_label("Position X").rect();
    h.drag_at(field.center());

    // Past egui's longest click, the press becomes a drag that has not moved
    h.run_steps(5);
    h.hover_at(field.center() + egui::vec2(10.0, 0.0));
    h.run();
    h.hover_at(field.center() + egui::vec2(20.0, 0.0));
    h.run();
    h.drop_at(field.center() + egui::vec2(20.0, 0.0));
    h.run();

    let merges = &h.state().position_merges;
    assert_eq!(merges.first(), Some(&false), "the first move starts an undo step: {merges:?}");
    assert!(merges.iter().skip(1).all(|merge| *merge), "later moves continue it: {merges:?}");
}

/// A ramp with one marker at `state.0`, recording each marker drag's merge in `state.1`
fn one_marker_ramp(ui: &mut egui::Ui, state: &mut (f32, Vec<bool>)) {
    ui.set_width(PANEL_WIDTH);
    PropertySheet::show(ui, "ramp", |s| {
        if let Some(drag) = s.gradient_ramp(&[[1.0, 0.0, 0.0]], &[state.0], &|_| [0.5; 3]) {
            state.0 = drag.position;
            state.1.push(drag.merge);
        }
    });
}

/// A marker pressed past the ramp's end changes nothing at first; its first move inward still starts its own undo step
#[test]
fn a_gradient_marker_pressed_past_the_ramps_end_starts_an_undo_step_with_its_first_change() {
    let _language = crate::i18n::TEST_LANGUAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut h = builder(Brightness::MediumDark).build_ui_state(one_marker_ramp, (1.0, Vec::new()));
    h.run_steps(2);
    let marker = h.get_by_label("Color 1").rect();
    let past_end = marker.right_center() - egui::vec2(1.0, 0.0);
    h.drag_at(past_end);
    h.run();
    h.hover_at(past_end + egui::vec2(20.0, 0.0));
    h.run();
    h.hover_at(past_end - egui::vec2(40.0, 0.0));
    h.run();
    h.hover_at(past_end - egui::vec2(60.0, 0.0));
    h.run();
    h.drop_at(past_end - egui::vec2(60.0, 0.0));
    h.run();

    let merges = &h.state().1;
    assert_eq!(merges.first(), Some(&false), "the first real change starts an undo step: {merges:?}");
    assert!(merges.iter().skip(1).all(|merge| *merge), "later changes continue it: {merges:?}");
}

#[test]
fn the_label_column_fits_the_widest_short_label_and_long_labels_stack() {
    let _language = crate::i18n::TEST_LANGUAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let h = harness(Brightness::MediumDark, Gallery::default());
    let hardness = h.get_by_label("Hardness").rect();
    let filtering = h.get_by_label("Bilinear | HQ").rect();
    let crop = h.get_by_label("Projection Extends Outside Shape").rect();

    let column = filtering.left();
    assert!((hardness.left() - column).abs() < 0.5, "sliders start on the controls column");
    assert!(crop.left() < column, "a label wider than the cap stacks above a full-width control");
}

#[test]
fn reset_restores_the_band_and_the_seed_dice_steps_deterministically() {
    let _language = crate::i18n::TEST_LANGUAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut h = harness(Brightness::MediumDark, Gallery::default());
    h.get_by_label("Random").click();
    h.get_by_label("Restore defaults").click();
    h.get_by_label("Randomize seeds").click();
    h.run_steps(2);

    assert_eq!(h.state().seed, next_seed(1234));
    assert_eq!(h.state().position, [0.0; 3]);
    assert_eq!(h.state().randomized, 1);
}

/// The gallery in every theme, closed and with a list open, for comparing with the mockups
#[test]
#[ignore]
fn property_sheet_gallery_snapshots() {
    let _language = crate::i18n::TEST_LANGUAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(output_dir) = std::env::var_os("TC_UI_SNAPSHOTS").map(std::path::PathBuf::from) else { return };

    std::fs::create_dir_all(&output_dir).unwrap();
    for (brightness, name) in [(Brightness::MediumDark, "medium"), (Brightness::Dark, "dark"), (Brightness::Light, "light")] {
        let mut h = builder(brightness).with_pixels_per_point(2.0).wgpu().build_ui_state(gallery, Gallery::default());
        h.run_steps(4);
        h.render().unwrap().save(output_dir.join(format!("sheet-{name}.png"))).unwrap();

        h.get_by_label("Bilinear | HQ").click();
        h.run_steps(2);
        h.get_by_label("Bilinear | Sharp").hover();
        h.run_steps(2);
        h.render().unwrap().save(output_dir.join(format!("sheet-{name}-list-open.png"))).unwrap();
        h.key_press(egui::Key::Escape);
        h.run_steps(2);

        // A value: the grey line on hover, then typing on that line in accent; the seed's box lights the same way
        let value = h.get_by_label("Hardness value").rect().center();
        h.hover_at(value);
        h.run_steps(2);
        h.render().unwrap().save(output_dir.join(format!("sheet-{name}-value-hover.png"))).unwrap();
        h.drag_at(value);
        h.drop_at(value);
        h.run_steps(2);
        h.get_by_role(egui::Role::TextInput).type_text("0.4");
        h.run_steps(2);
        h.render().unwrap().save(output_dir.join(format!("sheet-{name}-value-typing.png"))).unwrap();
        h.key_press(egui::Key::Escape);
        h.run_steps(2);
        let seed = h.get_by_label("Seed value").rect().center();
        h.hover_at(seed);
        h.run_steps(2);
        h.render().unwrap().save(output_dir.join(format!("sheet-{name}-seed-hover.png"))).unwrap();
        h.drag_at(seed);
        h.drop_at(seed);
        h.run_steps(2);
        h.render().unwrap().save(output_dir.join(format!("sheet-{name}-seed-typing.png"))).unwrap();
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
        PropertySheet::show(ui, "folds", |s| {
            if *extra_shown && s.fold("Extra", None, true) {
                s.empty_note("Extra rows");
            }
            if s.fold("Edges", None, true) {
                s.empty_note("Edge rows");
            }
            if s.fold("Parameters", None, false) {
                s.empty_note("Parameter rows");
            }
        });
    };
    let mut h = builder(Brightness::MediumDark).build_ui_state(folds, true);
    h.run_steps(2);
    h.get_by_label("Edges").click();
    h.run_steps(2);
    assert!(h.query_by_label("Edge rows").is_none());

    *h.state_mut() = false;
    h.run_steps(2);

    assert!(h.query_by_label("Edge rows").is_none(), "Edges stays folded");
    assert!(h.query_by_label("Parameter rows").is_none(), "Parameters stays folded");
}

/// Two colour bars, one under the other, each opening its own picker
fn two_color_bars(ui: &mut egui::Ui) {
    PropertySheet::show(ui, "color_bars", |s| {
        s.color_bar("Bar A", [0.2, 0.4, 0.6], None, &[]);
        s.color_bar("Bar B", [0.6, 0.4, 0.2], None, &[]);
    });
}

/// The colour space picked in one picker is the one the next picker opens in, even for a bar drawn before it all along
#[test]
fn the_next_picker_opens_in_the_colour_space_last_picked() {
    let _language = crate::i18n::TEST_LANGUAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut h = builder(Brightness::MediumDark).build_ui(two_color_bars);
    h.run_steps(2);
    h.get_by_role_and_label(egui::Role::Button, "Bar B").click();
    h.run_steps(2);
    h.get_by_role_and_label(egui::Role::ComboBox, "sRGB").click();
    h.run_steps(2);
    h.get_by_role_and_label(egui::Role::ListBoxOption, "Linear").click();
    h.run_steps(2);
    assert!(h.query_by_role_and_label(egui::Role::ComboBox, "Linear").is_some(), "Bar B's picker shows Linear");

    h.get_by_role_and_label(egui::Role::Button, "Close").click();
    h.run_steps(2);
    h.get_by_role_and_label(egui::Role::Button, "Bar A").click();
    h.run_steps(2);

    assert!(h.query_by_role_and_label(egui::Role::TitleBar, "Bar A").is_some(), "Bar A's picker is open");
    assert!(h.query_by_role_and_label(egui::Role::ComboBox, "Linear").is_some(), "Bar A's picker opens in Linear");
}

/// A colour bar's picker stays open when a colour bar above it comes and goes (its state follows its name, not its place)
#[test]
fn a_color_bars_picker_stays_open_when_a_row_above_it_goes_away() {
    let _language = crate::i18n::TEST_LANGUAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let bars = |ui: &mut egui::Ui, extra_shown: &mut bool| {
        PropertySheet::show(ui, "color_bars", |s| {
            if *extra_shown {
                s.color_bar("Extra", [0.5; 3], None, &[]);
            }
            s.color_bar("Tint", [0.2, 0.4, 0.6], None, &[]);
        });
    };
    let mut h = builder(Brightness::MediumDark).build_ui_state(bars, true);
    h.run_steps(2);
    h.get_by_role_and_label(egui::Role::Button, "Tint").click();
    h.run_steps(2);
    assert!(h.query_by_role_and_label(egui::Role::TitleBar, "Tint").is_some(), "the picker opened");

    *h.state_mut() = false;
    h.run_steps(2);

    assert!(h.query_by_role_and_label(egui::Role::TitleBar, "Tint").is_some(), "the picker stays open");
}

/// A fold under a later section, before that section's first band, keeps its state when a band of the section above comes and goes
#[test]
fn a_fold_before_a_sections_first_band_keeps_its_state_when_the_band_above_goes_away() {
    let _language = crate::i18n::TEST_LANGUAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let sections = |ui: &mut egui::Ui, band_shown: &mut bool| {
        PropertySheet::show(ui, "sections", |s| {
            s.section("First", false);
            if *band_shown && s.band("Band", true, None, None).open {
                s.empty_note("Band rows");
            }
            s.section("Second", false);
            if s.fold("Edges", None, true) {
                s.empty_note("Edge rows");
            }
        });
    };
    let mut h = builder(Brightness::MediumDark).build_ui_state(sections, true);
    h.run_steps(2);
    h.get_by_label("Edges").click();
    h.run_steps(2);
    assert!(h.query_by_label("Edge rows").is_none());

    *h.state_mut() = false;
    h.run_steps(2);

    assert!(h.query_by_label("Edge rows").is_none(), "Edges stays folded");
}
