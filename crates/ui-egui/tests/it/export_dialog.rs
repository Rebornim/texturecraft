//! The Export textures dialog: it fits the app window, Esc closes it, and its notes say what each setting writes

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable};
use texturecraft_engine::core::NormalFormat;
use texturecraft_ui_egui::TextureCraftApp;
use texturecraft_ui_egui::export_draft::ExportDraft;

const MANY_SETS: usize = 80;
const LAPTOP: egui::Vec2 = egui::vec2(1280.0, 720.0);

/// The sample project with `set_count` texture sets in an app window of `size`, Export textures open
fn harness(set_count: usize, size: egui::Vec2) -> Harness<'static, TextureCraftApp> {
    let mut session = texturecraft_engine::sample_session(256, 64).unwrap();
    let template = session.doc.project.texture_sets[0].clone();
    while session.doc.project.texture_sets.len() < set_count {
        let mut set = template.clone();
        set.name = format!("Set {}", session.doc.project.texture_sets.len() + 1);
        session.doc.project.texture_sets.push(set);
    }
    let mut harness = Harness::builder().with_size(size).build_eframe(|cc| TextureCraftApp::new(cc, session));
    harness.state_mut().ui.windows.export = true;
    harness.run_steps(3);
    harness
}

/// The open window's unsaved edits
fn draft<'a>(harness: &'a mut Harness<'static, TextureCraftApp>) -> &'a mut ExportDraft {
    harness.state_mut().ui.export.settings.as_mut().unwrap()
}

fn exported_sets(harness: &mut Harness<'static, TextureCraftApp>) -> usize {
    draft(harness).sets.iter().filter(|set| !set.skip).count()
}

#[test]
fn many_texture_sets_keep_the_dialog_inside_the_window() {
    for size in [LAPTOP, egui::vec2(1280.0, 480.0)] {
        let harness = harness(MANY_SETS, size);
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
        let dialog = harness.get_all_by_role(Role::Window).find(|node| node.accesskit_node().label().as_deref() == Some("Export textures")).unwrap();
        assert!(screen.contains_rect(dialog.rect()), "the dialog {:?} leaves the {size:?} window", dialog.rect());
        for label in ["Export", "Cancel", "Reset all"] {
            let button = harness.get_all_by_label(label).find(|node| node.accesskit_node().role() == Role::Button).unwrap();
            assert!(dialog.rect().contains_rect(button.rect()), "{label} {:?} is outside the dialog {:?}", button.rect(), dialog.rect());
        }
    }
}

#[test]
fn the_set_list_menu_checks_unchecks_or_inverts_every_set() {
    let mut harness = harness(MANY_SETS, LAPTOP);
    draft(&mut harness).sets[0].skip = true;
    for (item, exported) in [("Invert all", 1), ("Check all", MANY_SETS), ("Uncheck all", 0)] {
        let menus = harness.get_all_by_label(egui_phosphor::regular::LIST);
        menus.min_by(|a, b| a.rect().min.x.total_cmp(&b.rect().min.x)).unwrap().click();
        harness.run_steps(2);
        harness.get_by_label(item).click();
        harness.run_steps(2);
        assert_eq!(exported_sets(&mut harness), exported, "{item}");
    }
    harness.get_by_label("Include all").click();
    harness.run_steps(2);
    assert_eq!(exported_sets(&mut harness), MANY_SETS, "Include all");
}

#[test]
fn dragging_across_ticks_sets_them_all() {
    let mut harness = harness(4, LAPTOP);
    let ticks: Vec<egui::Pos2> = ["Ball", "Base", "Set 3"]
        .iter()
        .map(|name| {
            let row = harness.get_by_label(name).rect().center().y;
            harness.get_all_by_role(Role::CheckBox).find(|node| (node.rect().center().y - row).abs() < 6.0).unwrap().rect().center()
        })
        .collect();
    let button = |pos, pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
    harness.hover_at(ticks[0]);
    harness.event(button(ticks[0], true));
    harness.run_steps(1);
    for &tick in &ticks[1..] {
        harness.hover_at(tick);
        harness.run_steps(1);
    }
    harness.event(button(ticks[2], false));
    harness.run_steps(2);
    let skipped: Vec<bool> = draft(&mut harness).sets.iter().map(|set| set.skip).collect();
    assert_eq!(skipped, [true, true, true, false], "the three ticks passed over are off, the fourth untouched");
}

#[test]
fn escape_closes_the_dialog() {
    let mut harness = harness(2, LAPTOP);
    harness.key_press(egui::Key::Escape);
    harness.run_steps(2);
    assert!(!harness.state().ui.windows.export);
}

/// The value shown in the dialog's Normal map format combo
fn normal_format_shown(harness: &mut Harness<'static, TextureCraftApp>, preset: &str, project: NormalFormat, chosen: &str) -> String {
    let draft = draft(harness);
    draft.settings.parameters.preset = preset.into();
    draft.settings.normal_format = chosen.into();
    harness.state_mut().session.doc.project.normal_format = project;
    harness.run_steps(2);
    let row = harness.get_by_label("Normal map format").rect().center().y;
    let combo = harness.get_all_by_role(Role::ComboBox).find(|node| (node.rect().center().y - row).abs() < 4.0).unwrap();
    combo.value().unwrap_or_default()
}

#[test]
fn the_normal_format_follows_the_preset_visibly() {
    let mut harness = harness(2, LAPTOP);
    let unreal = "Unreal Engine (Packed)";
    let document = "Document channels + Normal + AO";
    assert_eq!(normal_format_shown(&mut harness, unreal, NormalFormat::OpenGl, ""), "From template / project (DirectX)");
    assert_eq!(normal_format_shown(&mut harness, document, NormalFormat::OpenGl, ""), "From template / project (OpenGL)");
    assert_eq!(normal_format_shown(&mut harness, document, NormalFormat::DirectX, ""), "From template / project (DirectX)");
    assert_eq!(normal_format_shown(&mut harness, unreal, NormalFormat::OpenGl, "openGl"), "OpenGL", "the override wins");
}

#[test]
fn the_color_space_says_data_maps_stay_linear() {
    let harness = harness(2, LAPTOP);
    harness.get_by_label("Color maps only: normal, roughness, metallic and other data maps stay linear");
}

#[test]
fn save_settings_writes_the_project_and_cancel_drops_the_edits() {
    let mut harness = harness(2, LAPTOP);
    draft(&mut harness).settings.parameters.file_type.format = "tga".into();
    harness.get_by_label("Save settings").click();
    harness.run_steps(2);
    let saved = harness.state().session.doc.project.export.clone().unwrap();
    assert_eq!(saved.parameters.file_type.format, "tga");
    assert!(harness.state().ui.windows.export, "Save settings keeps the window open");

    draft(&mut harness).settings.parameters.file_type.format = "png".into();
    harness.get_by_label("Cancel").click();
    harness.run_steps(2);
    assert!(harness.state().ui.export.settings.is_none(), "the edits are dropped");
    assert_eq!(harness.state().session.doc.project.export.as_ref().unwrap().parameters.file_type.format, "tga");

    harness.state_mut().ui.windows.export = true;
    harness.run_steps(2);
    assert_eq!(draft(&mut harness).settings.parameters.file_type.format, "tga", "the window reopens on the saved settings");
}

#[test]
fn the_bottom_bar_counts_the_textures_to_export() {
    let mut harness = harness(2, LAPTOP);
    let plan = texturecraft_engine::execute(&mut harness.state_mut().session, "export.plan", &serde_json::json!({})).unwrap();
    let all = plan.as_array().unwrap().len();
    harness.get_by_label(&format!("{all} textures selected for export"));

    draft(&mut harness).sets[1].skip = true;
    harness.run_steps(2);
    let one_set = plan.as_array().unwrap().iter().filter(|file| file["ts"] == 0).count();
    harness.get_by_label(&format!("{one_set} textures selected for export"));
}

/// The pen on the row labelled `parameter` in a texture set's settings
fn pen<'a>(harness: &'a Harness<'static, TextureCraftApp>, parameter: &str) -> egui_kittest::Node<'a> {
    let row = harness.get_all_by_label(parameter).last().unwrap().rect().center().y;
    harness.get_all_by_label("Override for this texture set").find(|node| (node.rect().center().y - row).abs() < 6.0).unwrap()
}

#[test]
fn a_texture_set_overrides_a_parameter_with_its_pen() {
    let mut harness = harness(2, LAPTOP);
    harness.get_by_label("Base").click();
    harness.run_steps(2);
    assert_eq!(draft(&mut harness).selected, Some(1));
    assert!(harness.query_by_label("Based on each texture set's size").is_none(), "a set shows its own size, not the global choice");
    harness.get_by_label("256x256 (texture set size)");

    pen(&harness, "Size").click();
    harness.run_steps(2);
    assert_eq!(draft(&mut harness).sets[1].overrides.size, Some(0), "the override starts from the global value");
    assert!(harness.get_all_by_label("*").next().is_some(), "the set's row marks the override");

    pen(&harness, "Size").click();
    harness.run_steps(2);
    assert_eq!(draft(&mut harness).sets[1].overrides.size, None, "a second click goes back to the global value");
    assert!(harness.query_all_by_label("*").next().is_none(), "the marker goes with the override");
}

#[test]
fn the_section_menu_overrides_every_parameter_or_none() {
    let mut harness = harness(2, LAPTOP);
    harness.get_by_label("Base").click();
    harness.run_steps(2);
    for (item, overridden) in [("Enable override for all items", true), ("Disable override for all items", false)] {
        // The upper menu, on General export parameters
        let menus = harness.get_all_by_label(egui_phosphor::regular::LIST);
        menus.min_by(|a, b| a.rect().min.y.total_cmp(&b.rect().min.y)).unwrap().click();
        harness.run_steps(2);
        harness.get_by_label(item).click();
        harness.run_steps(2);
        let overrides = draft(&mut harness).sets[1].overrides.clone();
        let all = [overrides.preset.is_some(), overrides.file_type.is_some(), overrides.size.is_some(), overrides.padding.is_some()];
        assert_eq!(all, [overridden; 4], "{item}");
    }
}

#[test]
fn a_texture_set_leaves_out_a_map_or_gives_it_its_own_file_type() {
    let mut harness = harness(2, LAPTOP);
    let count = |harness: &mut Harness<'static, TextureCraftApp>| {
        let params = draft(harness).params();
        texturecraft_engine::execute(&mut harness.state_mut().session, "export.plan", &params).unwrap().as_array().unwrap().len()
    };
    let all = count(&mut harness);
    harness.get_by_label("Base").click();
    harness.run_steps(2);
    let key = "$mesh_$textureSet_roughness";
    let row = harness.get_by_label(key).rect().center().y;
    harness.get_all_by_role(Role::CheckBox).find(|node| (node.rect().center().y - row).abs() < 6.0).unwrap().click();
    harness.run_steps(2);
    assert!(draft(&mut harness).sets[1].maps[key].skip);
    assert_eq!(count(&mut harness), all - 1, "the map is left out of that set only");

    pen(&harness, key).click();
    harness.run_steps(2);
    assert_eq!(
        draft(&mut harness).sets[1].maps[key].file_type,
        Some(texturecraft_engine::core::ExportFileType::new("png", 8)),
        "the override starts from what the map writes"
    );
}

#[test]
fn the_window_opens_on_settings_and_reset_all_belongs_to_settings() {
    let mut harness = harness(2, LAPTOP);
    harness.get_by_label("Output templates").click();
    harness.run_steps(2);
    assert!(harness.query_by_label("Reset all").is_none(), "Reset all is a Settings button");
    harness.key_press(egui::Key::Escape);
    harness.run_steps(2);
    harness.state_mut().ui.windows.export = true;
    harness.run_steps(2);
    assert_eq!(harness.state().ui.export.tab, 0);
    harness.get_by_label("Reset all");
}

#[test]
fn the_reset_icon_resets_the_general_parameters() {
    let mut harness = harness(2, LAPTOP);
    draft(&mut harness).settings.parameters.size = 512;
    draft(&mut harness).sets[1].overrides.size = Some(1024);
    harness.get_by_label("Reset the general export parameters").click();
    harness.run_steps(2);
    assert_eq!(draft(&mut harness).settings.parameters.size, 0);
    assert_eq!(draft(&mut harness).sets[1].overrides.size, Some(1024), "a set's overrides are its own");
}

/// The combo on the dialog row labelled `label` (other panels have labels like Size too), the `nth` from the left
fn combo_on_row<'a>(harness: &'a Harness<'static, TextureCraftApp>, label: &str, nth: usize) -> egui_kittest::Node<'a> {
    let on_row = |row: f32| {
        let mut combos: Vec<_> = harness.get_all_by_role(Role::ComboBox).filter(|node| (node.rect().center().y - row).abs() < 6.0).collect();
        combos.sort_by(|a, b| a.rect().min.x.total_cmp(&b.rect().min.x));
        combos
    };
    let rows = harness.get_all_by_label(label).map(|node| node.rect().center().y);
    let mut combos = rows.map(on_row).find(|combos| combos.len() > nth).unwrap();
    combos.remove(nth)
}

#[test]
fn a_file_type_offers_only_the_bit_depths_it_holds() {
    let mut harness = harness(2, LAPTOP);
    assert!(combo_on_row(&harness, "File type", 1).accesskit_node().is_disabled(), "the template sets the bit depth");

    combo_on_row(&harness, "File type", 0).click();
    harness.run_steps(2);
    harness.get_by_label("targa").click();
    harness.run_steps(2);
    let file_type = draft(&mut harness).settings.parameters.file_type.clone();
    assert_eq!((file_type.format.as_str(), file_type.bit_depth), ("tga", 8));
    assert!(!combo_on_row(&harness, "File type", 1).accesskit_node().is_disabled());

    combo_on_row(&harness, "File type", 1).click();
    harness.run_steps(2);
    assert!(harness.query_by_label("16 bits").is_none(), "targa holds 8 bits only");
}

#[test]
fn the_size_list_starts_at_128() {
    let mut harness = harness(2, LAPTOP);
    combo_on_row(&harness, "Size", 0).click();
    harness.run_steps(2);
    harness.get_by_label("Maximum size for all texture sets");
    harness.get_by_label("128 x 128").click();
    harness.run_steps(2);
    assert_eq!(draft(&mut harness).settings.parameters.size, 128);
}

#[test]
fn eight_bits_can_be_dithered() {
    let mut harness = harness(2, LAPTOP);
    combo_on_row(&harness, "File type", 0).click();
    harness.run_steps(2);
    harness.get_by_label("png").click();
    harness.run_steps(2);
    combo_on_row(&harness, "File type", 1).click();
    harness.run_steps(2);
    harness.get_by_label("8 bits + dithering").click();
    harness.run_steps(2);
    let file_type = draft(&mut harness).settings.parameters.file_type.clone();
    assert!(file_type.dithering && file_type.bit_depth == 8);
}

#[test]
fn padding_offers_dilation_with_diffusion() {
    let mut harness = harness(2, LAPTOP);
    combo_on_row(&harness, "Padding", 0).click();
    harness.run_steps(2);
    harness.get_by_label("Dilation + diffusion").click();
    harness.run_steps(2);
    assert_eq!(draft(&mut harness).settings.parameters.padding, "diffusion");
    let row = harness.get_all_by_label("Padding").map(|node| node.rect().center().y).find(|y| *y < 400.0).unwrap();
    let distance = harness.get_all_by_role(Role::SpinButton).find(|node| (node.rect().center().y - row).abs() < 6.0).unwrap();
    assert!(!distance.accesskit_node().is_disabled(), "diffusion starts beyond a set distance");
}

#[test]
fn global_settings_ticks_the_shader_parameters() {
    let mut harness = harness(2, LAPTOP);
    harness.get_by_label("Export shader parameters").click();
    harness.run_steps(2);
    assert!(draft(&mut harness).settings.shader_parameters);
}

#[test]
fn global_settings_ticks_the_usd_asset() {
    let mut harness = harness(2, LAPTOP);
    harness.get_by_label("Export USD asset").click();
    harness.run_steps(2);
    assert!(draft(&mut harness).settings.usd);
}

fn dialog_rect(harness: &Harness<'static, TextureCraftApp>) -> egui::Rect {
    harness.get_all_by_role(Role::Window).find(|node| node.accesskit_node().label().as_deref() == Some("Export textures")).unwrap().rect()
}

#[test]
fn the_window_keeps_its_size_on_every_tab() {
    for size in [LAPTOP, egui::vec2(1280.0, 480.0)] {
        let mut harness = harness(MANY_SETS, size);
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
        for tab in ["Settings", "Output templates", "List of exports"] {
            let inside = dialog_rect(&harness);
            harness.get_all_by_label(tab).find(|node| inside.contains_rect(node.rect())).unwrap().click();
            harness.run_steps(5);
            let before = dialog_rect(&harness);
            harness.run_steps(120);
            let after = dialog_rect(&harness);
            assert_eq!(before, after, "{tab} in a {size:?} window: the window keeps its size");
            let change = (after.size() - inside.size()).abs();
            assert!(change.x < 1.0 && change.y < 1.0, "{tab}: every tab has the same window size, {:?} vs {:?}", after.size(), inside.size());
            assert!(screen.contains_rect(after), "{tab}: {after:?} leaves the {size:?} window");
        }
    }
}

#[test]
fn the_output_templates_tab_holds_still() {
    let mut harness = harness(2, LAPTOP);
    harness.get_by_label("Output templates").click();
    harness.run_steps(5);
    let before = harness.get_by_label("Input maps").rect();
    harness.run_steps(120);
    assert_eq!(before, harness.get_by_label("Input maps").rect(), "the columns don't creep");
}

#[test]
fn the_naming_pattern_renames_the_input_maps() {
    let mut harness = harness(2, LAPTOP);
    harness.get_by_label("Output templates").click();
    harness.run_steps(3);
    harness.get_by_label("Metallic");
    assert!(harness.query_by_label("Base metalness").is_none());
    harness.state_mut().ui.export.openpbr_names = true;
    harness.run_steps(3);
    harness.get_by_label("Base metalness");
    harness.get_by_label("Naming pattern");
}

#[test]
fn an_empty_template_cannot_be_saved_or_exported() {
    let mut harness = harness(2, LAPTOP);
    harness.get_by_label("Output templates").click();
    harness.run_steps(2);
    let enabled = |harness: &Harness<'static, TextureCraftApp>, label: &str| {
        !harness.get_all_by_label(label).find(|node| node.accesskit_node().role() == Role::Button).unwrap().accesskit_node().is_disabled()
    };
    assert!(enabled(&harness, "Save settings") && enabled(&harness, "Export"));
    let app = harness.state_mut();
    texturecraft_engine::execute(&mut app.session, "export.preset.save", &serde_json::json!({"preset": {"name": "empty", "maps": []}})).unwrap();
    app.ui.export.preset = "empty".into();
    harness.run_steps(3);
    assert!(!enabled(&harness, "Save settings") && !enabled(&harness, "Export"));
}

#[test]
fn the_template_list_is_alphabetical() {
    let mut harness = harness(2, LAPTOP);
    harness.get_by_label("Output templates").click();
    harness.run_steps(2);
    let y = |name: &str| harness.get_by_label(name).rect().top();
    assert!(y("Arnold (Standard Surface)") < y("CryEngine (Illum)") && y("CryEngine (Illum)") < y("Unity URP (Metallic)"));
}

#[test]
fn export_runs_on_the_list_of_exports_with_its_console() {
    let mut harness = harness(2, LAPTOP);
    let dir = std::env::temp_dir().join(format!("tc_export_dialog_run_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    draft(&mut harness).settings.dir = dir.display().to_string();
    harness.get_all_by_label("Export").find(|node| node.accesskit_node().role() == Role::Button).unwrap().click();
    harness.run_steps(2);
    assert_eq!(harness.state().ui.export.tab, 2, "the export shows on List of exports");
    harness.get_by_label("Cancel Export");

    for _ in 0..20 {
        harness.run_steps(1);
        if harness.state().export_run.as_ref().is_some_and(|run| run.finished) {
            break;
        }
    }
    let run = harness.state().export_run.as_ref().unwrap();
    assert!(run.finished && run.log.last().is_some_and(|(_, success)| *success), "{:?}", run.log);
    assert!(std::fs::read_dir(&dir).unwrap().count() > 0, "the files are on disk");
    harness.run_steps(2);
    harness.get_by_label("Ball");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn cancel_export_stops_between_texture_sets() {
    let mut harness = harness(8, LAPTOP);
    let dir = std::env::temp_dir().join(format!("tc_export_dialog_cancel_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    draft(&mut harness).settings.dir = dir.display().to_string();
    harness.get_all_by_label("Export").find(|node| node.accesskit_node().role() == Role::Button).unwrap().click();
    // The click starts the run; the next frame exports the first set and shows Cancel Export
    harness.run_steps(2);
    harness.get_by_label("Cancel Export").click();
    harness.run_steps(3);
    let run = harness.state().export_run.as_ref().unwrap();
    assert!(run.finished && run.cancelled);
    let (done, total) = run.export.progress();
    assert!(done < total, "it stopped before the last set ({done} of {total})");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_app_never_opens_with_the_export_window_open() {
    let mut state = texturecraft_ui_egui::state::UiState::default();
    state.windows.export = true;
    let saved = serde_json::to_string(&state).unwrap();
    let restored: texturecraft_ui_egui::state::UiState = serde_json::from_str(&saved).unwrap();
    assert!(!restored.windows.export);
}

#[test]
fn the_window_moves_by_its_title_bar() {
    // An app window with room around the dialog: it is kept inside, so a move past an edge stops there
    let mut harness = harness(2, egui::vec2(1600.0, 960.0));
    let before = dialog_rect(&harness);
    let grab = egui::pos2(before.center().x, before.top() + 12.0);
    let button = |pos, pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
    harness.hover_at(grab);
    harness.event(button(grab, true));
    harness.run_steps(1);
    let to = grab + egui::vec2(-80.0, 40.0);
    harness.hover_at(to);
    harness.run_steps(1);
    harness.event(button(to, false));
    harness.run_steps(2);
    let after = dialog_rect(&harness);
    assert!((after.center() - before.center() - egui::vec2(-80.0, 40.0)).length() < 2.0, "{before:?} -> {after:?}");
}

#[test]
fn every_output_card_keeps_room_for_its_file_name() {
    let mut harness = harness(2, LAPTOP);
    harness.state_mut().ui.export.preset = "Unity HDRP (Mask Map)".into();
    harness.get_by_label("Output templates").click();
    harness.run_steps(3);
    let names: Vec<_> = harness.get_all_by_role(Role::TextInput).filter(|node| node.value().is_some_and(|value| value.starts_with('$'))).map(|node| node.rect().width()).collect();
    assert_eq!(names.len(), 3, "one name per output");
    assert!(names.iter().all(|width| *width > 150.0 && (width - names[0]).abs() < 1.0), "the R+G+B+A card's name is as wide as the others: {names:?}");
}

#[test]
fn an_output_map_gets_its_own_size() {
    let mut harness = harness(2, LAPTOP);
    let app = harness.state_mut();
    let mut mine = app.session.export_preset("Unity HDRP (Mask Map)").unwrap();
    mine.name = "mine".into();
    texturecraft_engine::execute(&mut app.session, "export.preset.save", &serde_json::json!({"preset": mine})).unwrap();
    app.ui.export.preset = "mine".into();
    harness.get_by_label("Output templates").click();
    harness.run_steps(3);
    harness.get_all_by_role(Role::ComboBox).find(|node| node.value().as_deref() == Some("Global")).unwrap().click();
    harness.run_steps(2);
    harness.get_by_label("512").click();
    harness.run_steps(3);
    let saved = harness.state().session.export_preset("mine").unwrap();
    assert_eq!(saved.maps.iter().map(|map| map.options.size).collect::<Vec<_>>(), [Some(512), None, None], "only the first map has its own size");
}
