//! A paint tool's Properties: one page of sections under jump tabs, Brush · Alpha · (Stencil) · Material, below a brush preview that stays put

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::json;
use texturecraft_engine::execute;
use texturecraft_ui_egui::TextureCraftApp;

use crate::{BAKE_RESOLUTION, FRAME_SECONDS, STARTUP_FRAMES, TEXTURE_RESOLUTION};

/// Short enough that the tool's sections scroll
const WINDOW: egui::Vec2 = egui::vec2(1400.0, 800.0);

/// A second at 60 frames: a jump scrolls with egui's scroll animation
const SCROLL_FRAMES: usize = 60;

fn paint_harness(tool: &str) -> Harness<'static, TextureCraftApp> {
    let mut session = texturecraft_engine::sample_session(TEXTURE_RESOLUTION, BAKE_RESOLUTION).unwrap();
    execute(&mut session, "tool.select", &json!({"tool": tool})).unwrap();
    let mut harness = Harness::builder()
        .with_size(WINDOW)
        .with_step_dt(FRAME_SECONDS)
        .with_context_setup(texturecraft_ui_egui::framework::install_headless)
        .build_eframe(|cc| TextureCraftApp::new(cc, session));
    harness.state_mut().ui.prefs.language = "en".into();
    harness.run_steps(STARTUP_FRAMES);
    harness
}

/// egui reports a tab's selection as its toggled state
fn tab_selected(harness: &Harness<'_, TextureCraftApp>, tab: &str) -> bool {
    harness.get_by_role_and_label(Role::Tab, tab).accesskit_node().toggled() == Some(egui::accesskit::Toggled::True)
}

#[test]
fn the_tabs_jump_within_one_page_of_the_tools_sections() {
    let mut harness = paint_harness("paint");
    for tab in ["Brush", "Alpha", "Stencil", "Material"] {
        assert!(harness.query_by_role_and_label(Role::Tab, tab).is_some(), "the {tab} tab");
    }
    assert!(tab_selected(&harness, "Brush"), "the page opens on its first section");
    let flow_before = harness.get_by_label("Flow").rect().top();

    harness.get_by_role_and_label(Role::Tab, "Material").click();
    harness.run_steps(SCROLL_FRAMES);

    assert!(tab_selected(&harness, "Material"), "the clicked section is the one in view");
    assert!(!tab_selected(&harness, "Brush"));
    assert!(harness.get_by_label("Flow").rect().top() < flow_before, "the Brush rows scrolled up out of the way, still on the page");
}

#[test]
fn every_brush_tool_has_a_stencil_tab_and_polygon_fill_none() {
    for tool in ["paint", "eraser", "projection", "smudge", "clone"] {
        let harness = paint_harness(tool);

        assert!(harness.query_by_role_and_label(Role::Tab, "Stencil").is_some(), "{tool}");
    }

    let harness = paint_harness("polygonFill");
    assert!(harness.query_by_role_and_label(Role::Tab, "Stencil").is_none(), "polygon fill paints no strokes");
}

#[test]
fn the_brush_preview_stays_under_the_tabs_while_the_sections_scroll() {
    let mut harness = paint_harness("paint");
    let preview_before = harness.get_by_label("Brush preview").rect();
    let flow_before = harness.get_by_label("Flow").rect().top();

    harness.get_by_role_and_label(Role::Tab, "Material").click();
    harness.run_steps(SCROLL_FRAMES);

    assert!(harness.get_by_label("Flow").rect().top() < flow_before, "the sections scrolled");
    assert_eq!(harness.get_by_label("Brush preview").rect(), preview_before, "the preview stayed where it was");
    assert!(preview_before.top() >= harness.get_by_role_and_label(Role::Tab, "Material").rect().bottom(), "under the tabs");
}
