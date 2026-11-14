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
                    tag_color: Some([Color32::from_rgb(0xd8, 0x60, 0x4f), Color32::from_rgb(0x6c, 0xbc, 0x5a), Color32::from_rgb(0x5a, 0x8c, 0xe0)][axis]),
                    value: g.position[axis],
                    speed: 0.01,
                    decimals: 2,
                    suffix: "",
                });
                if let Some((axis, _)) = s.number_fields("Position", &mut fields, Some(&mut g.linked), false)
                    && let Some(slot) = g.position.get_mut(axis)
                {
                    *slot = fields[axis].value;
                }
            }

            if s.card("Edges", Some(true)) {
                s.toggle("Wrap", &mut g.wrap);
                s.toggle("Shape Crop", &mut g.crop);
                s.picture_toggle(Some(&swatch(Color32::from_gray(160))), "Depth Culling", &mut g.depth_culling, false);
                s.picture_toggle(None, "Backface Culling", &mut g.backface_culling, true);
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

/// A harness showing the gallery in `brightness`, with the app's fonts from the first frame
fn builder(brightness: Brightness) -> egui_kittest::HarnessBuilder<Gallery> {
    Harness::builder().with_size(egui::vec2(PANEL_WIDTH + 260.0, 1000.0)).with_context_setup(move |ctx| {
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

#[test]
fn dropdown_list_hangs_from_its_button_at_its_width_and_picks() {
    let _language = crate::i18n::TEST_LANGUAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut h = harness(Brightness::MediumDark, Gallery::default());
    let button = h.get_by_label("Bilinear | HQ").rect();
    h.get_by_label("Bilinear | HQ").click();
    h.run_steps(2);

    let option = h.get_by_label("Nearest").rect();
    let list = option.expand(1.0);
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
    let track = h.get_by_label("Hardness").rect();
    type_at(&mut h, egui::pos2(track.right() + 20.0, track.center().y), "0.4");
    assert_eq!(h.state().hardness, 0.4, "the slider's value is replaced, not appended to");

    let dice = h.get_by_label("Random").rect();
    type_at(&mut h, egui::pos2(dice.left() - 20.0, dice.center().y), "77");
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

    let merges = &h.state().hardness_merges;
    assert_eq!(merges.first(), Some(&false), "the drag's first change starts an undo step: {merges:?}");
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
        let track = h.get_by_label("Hardness").rect();
        let value = egui::pos2(track.right() + 20.0, track.center().y);
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
        let dice = h.get_by_label("Random").rect();
        let seed = egui::pos2(dice.left() - 20.0, dice.center().y);
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
