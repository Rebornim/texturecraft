//! The tool shelf's variant menus: a corner arrow opens a tool's variants, picking one sets the tool and its setting, and the
//! button shows the current variant whichever control changed it

use egui::accesskit::{Role, Toggled};
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{By, NodeT, Queryable, by};
use serde_json::json;
use texturecraft_engine::Tool;
use texturecraft_ui_egui::TextureCraftApp;
use texturecraft_ui_egui::state::Mode;

fn app_harness(tool: Tool) -> Harness<'static, TextureCraftApp> {
    let mut session = texturecraft_engine::sample_session(256, 64).unwrap();
    session.tool.tool = tool;
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1600.0, 960.0))
        .with_context_setup(texturecraft_ui_egui::framework::install_headless)
        .build_eframe(|context| TextureCraftApp::new(context, session));
    harness.run_steps(4);
    harness
}

fn shelf_button(harness: &Harness<'_, TextureCraftApp>, name: &str) -> Rect {
    harness.get(shelf(name)).rect()
}

/// The shelf button named `name`, with or without its shortcut after it
fn shelf(name: &str) -> By<'_> {
    let named = move |label: &str| label == name || label.strip_prefix(name).is_some_and(|rest| rest.starts_with(" ("));
    by().role(Role::Button).predicate(move |node| node.label().is_some_and(|label| named(&label)))
}

/// The variant the shelf button of the tool named `tool` shows: its description for screen readers
fn shown_variant(harness: &Harness<'_, TextureCraftApp>, tool: &str) -> Option<String> {
    harness.get(shelf(tool)).accesskit_node().description()
}

/// A press and release at `position`, each in its own frame, as a hand clicks
fn click_at(harness: &mut Harness<'_, TextureCraftApp>, position: Pos2) {
    harness.hover_at(position);
    harness.run_steps(1);
    for pressed in [true, false] {
        harness.event(Event::PointerButton { pos: position, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
        harness.run_steps(1);
    }
    harness.run_steps(2);
}

/// A click on the arrow in the button's lower-right corner
fn click_arrow(harness: &mut Harness<'_, TextureCraftApp>, button: Rect) {
    click_at(harness, button.right_bottom() - vec2(3.0, 3.0));
}

/// The open menu's rows, by name, with whether each is the current variant
fn menu_rows(harness: &Harness<'_, TextureCraftApp>) -> Vec<(String, bool)> {
    harness
        .query_all(by().role(Role::RadioButton))
        .filter_map(|node| Some((node.accesskit_node().label()?, node.accesskit_node().toggled() == Some(Toggled::True))))
        .collect()
}

/// Whether a down caret is drawn inside `rect`
fn shows_caret_in(harness: &Harness<'_, TextureCraftApp>, rect: Rect) -> bool {
    harness.output().shapes.iter().any(|shape| {
        matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == egui_phosphor::regular::CARET_DOWN && rect.contains(text.visual_bounding_rect().center()))
    })
}

#[test]
fn picking_a_row_sets_the_tool_and_its_setting() {
    let mut harness = app_harness(Tool::Eraser);
    assert!(!harness.state().session.tool.brush.particles.enabled);

    let paint = shelf_button(&harness, "Paint");
    click_arrow(&mut harness, paint);
    assert_eq!(menu_rows(&harness), [("Paint".to_owned(), true), ("Physical paint".to_owned(), false)], "the arrow opens Paint's variants");
    assert_eq!(harness.state().session.tool.tool, Tool::Eraser, "opening the menu selects nothing");

    harness.get(by().role(Role::RadioButton).label("Physical paint")).click();
    harness.run_steps(3);
    let tool = &harness.state().session.tool;
    assert_eq!(tool.tool, Tool::Paint);
    assert!(tool.brush.particles.enabled, "the physical variant sheds particles");
    assert!(menu_rows(&harness).is_empty(), "picking closes the menu");

    let path = shelf_button(&harness, "Path");
    click_arrow(&mut harness, path);
    harness.get(by().role(Role::RadioButton).label("Filled path")).click();
    harness.run_steps(3);
    let tool = &harness.state().session.tool;
    assert_eq!(tool.tool, Tool::Path);
    assert!(tool.path.closed && tool.path.fill, "a filled path is closed and filled");

    let clone = shelf_button(&harness, "Clone");
    click_arrow(&mut harness, clone);
    harness.get(by().role(Role::RadioButton).label("Clone (absolute source)")).click();
    harness.run_steps(3);
    let tool = &harness.state().session.tool;
    assert_eq!(tool.tool, Tool::Clone);
    assert!(!tool.clone_aligned, "an absolute source stays where it was set");
}

#[test]
fn plain_click_selects_without_opening() {
    let mut harness = app_harness(Tool::Eraser);

    let paint = shelf_button(&harness, "Paint");
    click_at(&mut harness, paint.center());

    assert_eq!(harness.state().session.tool.tool, Tool::Paint);
    assert!(menu_rows(&harness).is_empty(), "a click on the icon selects the tool and opens nothing");

    // The arrow toggles: a second click on it closes the menu it opened
    click_arrow(&mut harness, paint);
    assert!(!menu_rows(&harness).is_empty());
    click_arrow(&mut harness, paint);
    assert!(menu_rows(&harness).is_empty());
}

#[test]
fn plain_click_keeps_the_variant() {
    let mut harness = app_harness(Tool::Eraser);
    harness.state_mut().session.tool.brush.particles.enabled = true;
    harness.run_steps(2);

    let paint = shelf_button(&harness, "Paint");
    click_at(&mut harness, paint.center());
    assert_eq!(harness.state().session.tool.tool, Tool::Paint);
    assert!(harness.state().session.tool.brush.particles.enabled, "selecting the tool keeps its variant");

    harness.event(Event::Key { key: Key::Num2, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    harness.run_steps(2);
    assert_eq!(harness.state().session.tool.tool, Tool::Eraser);
    assert!(harness.state().session.tool.brush.particles.enabled, "the tool's key keeps its variant too");
    assert_eq!(shown_variant(&harness, "Eraser").as_deref(), Some("Physical eraser"));
}

#[test]
fn menu_follows_the_old_controls() {
    let mut harness = app_harness(Tool::Paint);

    // What the top bar's particle toggle, the clone's Aligned setting and the path's Closed and Fill buttons do
    harness.state_mut().session.tool.brush.particles.enabled = true;
    harness.state_mut().run("tool.brush", json!({"cloneAligned": false}));
    let session = &mut harness.state_mut().session;
    session.set_path_closed(true);
    session.set_path_fill(true);
    session.tool.tool = Tool::Paint;
    harness.run_steps(2);

    let shown = [
        ("Paint", "Physical paint"),
        ("Eraser", "Physical eraser"),
        ("Projection", "Physical projection"),
        ("Clone", "Clone (absolute source)"),
        ("Path", "Filled path"),
    ];
    for (tool, variant) in shown {
        assert_eq!(shown_variant(&harness, tool).as_deref(), Some(variant), "the {tool} button shows its variant");
    }
    let paint = shelf_button(&harness, "Paint");
    click_arrow(&mut harness, paint);
    assert_eq!(menu_rows(&harness), [("Paint".to_owned(), false), ("Physical paint".to_owned(), true)], "the menu ticks the physical variant");
}

#[test]
fn baking_shelf_has_no_variant_arrows() {
    let mut harness = app_harness(Tool::Paint);
    harness.state_mut().session.tool.brush.particles.enabled = true;
    harness.run_steps(2);
    let paint = shelf_button(&harness, "Paint");
    assert!(shows_caret_in(&harness, paint), "painting shows the arrow");

    click_arrow(&mut harness, paint);
    assert!(!menu_rows(&harness).is_empty());
    harness.state_mut().ui.mode = Mode::Baking;
    harness.run_steps(3);

    let paint = shelf_button(&harness, "Paint");
    assert!(!shows_caret_in(&harness, paint), "the bake brushes have no variants");
    assert_ne!(shown_variant(&harness, "Paint").as_deref(), Some("Physical paint"), "nor show one");
    assert!(menu_rows(&harness).is_empty(), "leaving painting closes the menu");
    click_arrow(&mut harness, paint);
    assert!(menu_rows(&harness).is_empty());
}
