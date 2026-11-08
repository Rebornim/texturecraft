//! The dock host: every side panel (assets, texture set list, layers, texture set settings,
//! properties, display/shader settings, history, log) is a tab that can be docked, tabbed,
//! resized, floated and closed around the viewport. craft-ui's shared docking
//! (`craft_ui::docking`) draws the frames, tab strips, floating windows and drop previews and
//! owns the gestures; this module names the panels, builds the default arrangement, applies
//! the dock's changes and hosts the panel bodies. The bodies live in `dock.rs`, `shelf.rs` and
//! `windows.rs`.
//!
//! The viewport is the central canvas: it stays docked, never takes tabs and is the only panel
//! that cannot close or float. The layout persists with `UiState` as a craft-ui `Workspace`;
//! a layout that does not load or validate becomes the default.

use craft_ui::docking::{
    Action, Column, DockArea, DockContent, DockStyle, Group, Layout, Location, Node, PanelLimits, PanelState, Permissions, Placement, Side,
};
use craft_ui::layout::{SplitAxis, SplitSize};
use egui::{Id, Margin, RichText, Ui, vec2};
use egui_phosphor::regular as ic;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::TextureCraftApp;
use crate::i18n::tr;
use crate::theme::Tokens;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Panel {
    Viewport,
    Assets,
    // Saved layouts name the bake panels by their earlier names
    #[serde(rename = "BakeMaps")]
    MeshMapSettings,
    #[serde(rename = "BakeSettings")]
    CommonSettings,
    MeshMapBakers,
    TextureSetList,
    Layers,
    TextureSetSettings,
    Properties,
    Display,
    Shader,
    History,
    Log,
}

/// Window ▸ Views items and the panels they show or hide.
pub const VIEWS: [(&str, Panel); 12] = [
    ("ui.view.assets", Panel::Assets),
    ("ui.view.bakeMaps", Panel::MeshMapSettings),
    ("ui.view.bakeSettings", Panel::CommonSettings),
    ("ui.view.display", Panel::Display),
    ("ui.view.history", Panel::History),
    ("ui.view.layers", Panel::Layers),
    ("ui.view.log", Panel::Log),
    ("ui.view.meshMapBakers", Panel::MeshMapBakers),
    ("ui.view.properties", Panel::Properties),
    ("ui.view.shader", Panel::Shader),
    ("ui.view.textureSetList", Panel::TextureSetList),
    ("ui.view.textureSetSettings", Panel::TextureSetSettings),
];

/// The panel a Window ▸ Views item toggles.
pub fn view_panel(id: &str) -> Option<Panel> {
    VIEWS.iter().find(|(v, _)| *v == id).map(|(_, p)| *p)
}

impl Panel {
    pub(crate) fn available_in(self, mode: crate::state::Mode) -> bool {
        match self {
            Self::MeshMapSettings | Self::CommonSettings | Self::MeshMapBakers => mode == crate::state::Mode::Baking,
            Self::Assets | Self::Layers | Self::Properties | Self::TextureSetSettings => mode != crate::state::Mode::Baking,
            _ => true,
        }
    }

    pub const ALL: [Panel; 13] = [
        Panel::Viewport,
        Panel::Assets,
        Panel::MeshMapSettings,
        Panel::CommonSettings,
        Panel::MeshMapBakers,
        Panel::TextureSetList,
        Panel::Layers,
        Panel::TextureSetSettings,
        Panel::Properties,
        Panel::Display,
        Panel::Shader,
        Panel::History,
        Panel::Log,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Panel::Viewport => "Viewport",
            Panel::Assets => "Assets",
            Panel::MeshMapSettings => "Mesh Map Settings",
            Panel::CommonSettings => "Common Settings",
            Panel::MeshMapBakers => "Mesh Map Bakers",
            Panel::TextureSetList => "Texture Sets",
            Panel::Layers => "Layers",
            Panel::TextureSetSettings => "Texture Set Settings",
            Panel::Properties => "Properties",
            Panel::Display => "Display Settings",
            Panel::Shader => "Shader Settings",
            Panel::History => "History",
            Panel::Log => "Log",
        }
    }

    /// The panel's icon on the rail, for the panels that live there when no other place holds them
    fn rail_icon(self) -> Option<&'static str> {
        RAIL_PANELS.iter().find(|(panel, _)| *panel == self).map(|(_, icon)| *icon)
    }

    /// The tallest the panel's flyout or floating window grows to fit its content
    fn floating_height(self) -> Option<f32> {
        match self {
            Panel::TextureSetSettings | Panel::Display | Panel::Shader => Some(SETTINGS_FLYOUT_MAX_HEIGHT),
            Panel::History | Panel::Log => Some(LIST_FLYOUT_HEIGHT),
            _ => None,
        }
    }
}

/// The rail beside the dock, top to bottom: a panel the layout has nowhere else waits there as an icon
const RAIL_PANELS: [(Panel, &str); 5] = [
    (Panel::TextureSetSettings, ic::STACK),
    (Panel::Display, ic::MONITOR),
    (Panel::Shader, ic::SLIDERS_HORIZONTAL),
    (Panel::History, ic::CLOCK_COUNTER_CLOCKWISE),
    (Panel::Log, ic::FILE_TEXT),
];

/// Width of a rail panel's flyout
const RAIL_FLYOUT_WIDTH: f32 = 320.0;

/// Settings panels' flyouts fit their content up to this height, then scroll
const SETTINGS_FLYOUT_MAX_HEIGHT: f32 = 640.0;

/// History and Log fill their flyout, so it has one height
const LIST_FLYOUT_HEIGHT: f32 = 400.0;

/// Where bake mode's first floating panel opens, and how far down each next one opens
const BAKE_FLOAT_ORIGIN: egui::Pos2 = egui::pos2(960.0, 90.0);
const BAKE_FLOAT_CASCADE_STEP: f32 = 50.0;

/// Size of other floating panels
const DEFAULT_FLOATING_SIZE: egui::Vec2 = vec2(300.0, 380.0);

/// Width of the rail's icon column, as the old icon rail
const RAIL_WIDTH: f32 = 38.0;

/// The saved panel arrangement: craft-ui's workspace over TextureCraft's panels.
pub type Workspace = craft_ui::docking::Workspace<Panel>;

/// The viewport never moves, floats, closes or takes tabs; panels dock beside it.
pub fn permissions(panel: &Panel) -> Permissions {
    if *panel == Panel::Viewport { Permissions { accepts_splits: true, ..Permissions::PROTECTED } } else { Permissions::default() }
}

fn tabs(panels: &[Panel]) -> Node<Panel> {
    Node::Tabs { panels: panels.to_vec(), active: 0 }
}

fn split(axis: SplitAxis, size: SplitSize, first: Node<Panel>, second: Node<Panel>) -> Node<Panel> {
    Node::Split { axis, size, first: Box::new(first), second: Box::new(second) }
}

/// The default: assets left of the viewport, then texture sets, layers (tabbed with texture set settings) and properties, then the rail; ratios fit 1600x960
pub fn default_workspace() -> Workspace {
    use Panel::*;
    let column = split(
        SplitAxis::Vertical,
        SplitSize::Ratio(0.125),
        tabs(&[TextureSetList]),
        split(SplitAxis::Vertical, SplitSize::Ratio(0.57), tabs(&[Layers, TextureSetSettings]), tabs(&[Properties])),
    );
    let root =
        split(SplitAxis::Horizontal, SplitSize::Ratio(0.115), tabs(&[Assets]), split(SplitAxis::Horizontal, SplitSize::Ratio(0.78), tabs(&[Viewport]), column));
    let mut workspace = Workspace::new(Layout { root: Some(root), floating: Vec::new() });
    return_to_rail(&mut workspace);
    for panel in [MeshMapSettings, CommonSettings, MeshMapBakers] {
        workspace.hidden.push((
            panel,
            Location {
                floating: None,
                anchor: Some(TextureSetList),
                placement: Placement::Tab { before: None },
                size: None,
                stack: None,
                sibling: Vec::new(),
                group: None,
            },
        ));
    }
    workspace
}

/// Where a closed panel reopens as its own floating window at `rect` (`[x, y, width, height]`)
fn floating_location(rect: [f32; 4]) -> Location<Panel> {
    Location { floating: Some(rect), anchor: None, placement: Placement::Tab { before: None }, size: None, stack: None, sibling: Vec::new(), group: None }
}

/// Put every rail panel the layout has nowhere into the rail, adding the rail when the layout has none
fn return_to_rail(workspace: &mut Workspace) {
    let missing: Vec<Panel> = RAIL_PANELS.iter().map(|(panel, _)| *panel).filter(|panel| !workspace.layout.contains(panel)).collect();
    if missing.is_empty() {
        return;
    }

    let groups = missing.iter().map(|panel| Group::new(vec![*panel], 0));
    let mut next = workspace.clone();
    match next.layout.root.as_mut().and_then(rail_column_mut) {
        Some(column) => {
            column.groups.extend(groups);
            // Rail panels keep the rail's order; groups the user docked there go after them
            column
                .groups
                .sort_by_key(|group| group.panels.first().and_then(|panel| RAIL_PANELS.iter().position(|(rail, _)| rail == panel)).unwrap_or(usize::MAX));
        }
        None => {
            let column = Column { iconic: true, iconic_width: Some(RAIL_WIDTH), ..Column::new(groups.collect(), RAIL_FLYOUT_WIDTH) };
            let rail = Node::Dock { side: Side::End, columns: vec![column] };
            next.layout.root = next.layout.root.take().map(|root| split(SplitAxis::Horizontal, SplitSize::FixedSecond(RAIL_WIDTH), root, rail));
        }
    }
    next.hidden.retain(|(panel, _)| !missing.contains(panel));

    if validate(&next).is_ok() {
        *workspace = next;
    } else {
        log::warn!("the rail panels {missing:?} could not return to the rail");
    }
}

/// The rail: the first icon-only column of the dock along the window's end edge
fn rail_column(node: &Node<Panel>) -> Option<&Column<Panel>> {
    match node {
        Node::Split { second, .. } => rail_column(second),
        Node::Dock { side: Side::End, columns } => columns.iter().find(|column| column.iconic),
        _ => None,
    }
}

fn rail_column_mut(node: &mut Node<Panel>) -> Option<&mut Column<Panel>> {
    match node {
        Node::Split { second, .. } => rail_column_mut(second),
        Node::Dock { side: Side::End, columns } => columns.iter_mut().find(|column| column.iconic),
        _ => None,
    }
}

/// craft-ui's checks plus the viewport's: a saved or hand-edited file cannot hide, float, group
/// or collapse it.
pub fn validate(workspace: &Workspace) -> Result<(), String> {
    workspace.validate().map_err(|error| error.to_string())?;
    let layout = &workspace.layout;
    if layout.floating.iter().any(|window| window.contains(&Panel::Viewport)) {
        return Err("The viewport cannot float".into());
    }
    if workspace.hidden.iter().any(|(panel, _)| *panel == Panel::Viewport) {
        return Err("The viewport cannot be hidden".into());
    }
    let mut found = false;
    let mut nodes: Vec<&Node<Panel>> = layout.root.iter().collect();
    while let Some(node) = nodes.pop() {
        match node {
            Node::Split { first, second, .. } => {
                nodes.push(first);
                nodes.push(second);
            }
            Node::Tabs { panels, .. } if panels.contains(&Panel::Viewport) => {
                if panels.len() != 1 {
                    return Err("The viewport cannot share a tab group".into());
                }
                found = true;
            }
            Node::Stack { entries } if entries.iter().any(|entry| entry.panel == Panel::Viewport) => {
                return Err("The viewport cannot be collapsed".into());
            }
            Node::Dock { columns, .. } if columns.iter().flat_map(|column| &column.groups).any(|group| group.panels.contains(&Panel::Viewport)) => {
                return Err("The viewport cannot be in a dock".into());
            }
            _ => {}
        }
    }
    if found { Ok(()) } else { Err("The panel layout must contain the viewport".into()) }
}

// ---------------------------------------------------------------------------------------------
// The live dock

/// Width of the bake layout's texture set and mesh map bakers column, in points
const BAKERS_COLUMN_WIDTH: f32 = 200.0;
/// Width of the bake layout's common and mesh map settings column, in points
const SETTINGS_COLUMN_WIDTH: f32 = 330.0;
/// Height of the log under the bake layout's columns, in points
const BAKING_LOG_HEIGHT: f32 = 160.0;
/// Share of the first bake column the texture set list takes above the mesh map bakers
const TEXTURE_SET_LIST_SHARE: f32 = 0.4;

/// The dock layout (part of `UiState`; serialized as a craft-ui `Workspace`).
#[derive(Clone, Debug)]
pub struct Dock {
    workspace: Workspace,
    /// A panel to bring in front of the other floating windows on the next frame.
    raise: Option<Panel>,
    /// A saved layout could not be read, so this is the painting default in its place
    replaced_unreadable: bool,
}

impl Default for Dock {
    fn default() -> Self {
        Dock { workspace: default_workspace(), raise: None, replaced_unreadable: false }
    }
}

impl PartialEq for Dock {
    fn eq(&self, other: &Self) -> bool {
        self.workspace == other.workspace
    }
}

impl Serialize for Dock {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.workspace.serialize(s)
    }
}

impl<'de> Deserialize<'de> for Dock {
    /// Never fails: a layout that cannot be read or fails the viewport's rules is the default.
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(Dock::from_value(Value::deserialize(d)?))
    }
}

impl Dock {
    /// The dock for a saved layout; the default, marked as a replacement, when it is not a valid workspace
    pub fn from_value(value: Value) -> Dock {
        let unreadable = Dock { replaced_unreadable: true, ..Dock::default() };
        let default = default_workspace();
        let loaded = serde_json::from_value::<Workspace>(value).ok();
        let Some(mut workspace) = loaded.filter(|w| validate(w).is_ok()) else { return unreadable };

        workspace.reconcile(|_| true, permissions, &default);
        if validate(&workspace).is_err() {
            return unreadable;
        }

        Dock { workspace, raise: None, replaced_unreadable: false }
    }

    /// Whether a saved layout could not be read and this dock is the painting default instead
    pub(crate) fn replaced_unreadable(&self) -> bool {
        self.replaced_unreadable
    }

    /// Texture sets over the bakers, the two settings tabs, the log under both, then the viewport
    pub(crate) fn for_baking() -> Self {
        let mut workspace = default_workspace();
        for panel in [Panel::Assets, Panel::Layers, Panel::TextureSetSettings, Panel::Properties] {
            if let Ok(location) = workspace.layout.location(&panel) {
                workspace.hidden.push((panel, location));
            }
        }

        let bake_panels = [Panel::MeshMapSettings, Panel::CommonSettings, Panel::MeshMapBakers, Panel::Log];
        workspace.hidden.retain(|(panel, _)| !bake_panels.contains(panel));
        // Bake mode has no rail: the rail's other panels open as floating windows, cascaded
        for (slot, panel) in [Panel::Display, Panel::Shader, Panel::History].into_iter().enumerate() {
            let top = BAKE_FLOAT_ORIGIN.y + BAKE_FLOAT_CASCADE_STEP * slot as f32;
            workspace.hidden.push((panel, floating_location([BAKE_FLOAT_ORIGIN.x, top, DEFAULT_FLOATING_SIZE.x, DEFAULT_FLOATING_SIZE.y])));
        }

        let gutter_width = f32::from(crate::theme::GUTTER);
        let list_column = split(SplitAxis::Vertical, SplitSize::Ratio(TEXTURE_SET_LIST_SHARE), tabs(&[Panel::TextureSetList]), tabs(&[Panel::MeshMapBakers]));
        let settings_column = tabs(&[Panel::CommonSettings, Panel::MeshMapSettings]);
        let columns = split(SplitAxis::Horizontal, SplitSize::FixedFirst(BAKERS_COLUMN_WIDTH), list_column, settings_column);
        let columns_over_log = split(SplitAxis::Vertical, SplitSize::FixedSecond(BAKING_LOG_HEIGHT), columns, tabs(&[Panel::Log]));
        let columns_width = BAKERS_COLUMN_WIDTH + gutter_width + SETTINGS_COLUMN_WIDTH;
        let root = split(SplitAxis::Horizontal, SplitSize::FixedFirst(columns_width), columns_over_log, tabs(&[Panel::Viewport]));
        workspace.layout = Layout { root: Some(root), floating: Vec::new() };
        Dock { workspace, raise: None, replaced_unreadable: false }
    }

    pub fn workspace(&self) -> &Workspace {
        &self.workspace
    }

    pub fn is_open(&self, p: Panel) -> bool {
        self.workspace.layout.contains(&p)
    }

    /// Open and the front tab of its group.
    pub fn is_visible(&self, p: Panel) -> bool {
        self.workspace.layout.panel_state(&p) == PanelState::Front
    }

    /// Hide a visible panel, otherwise bring it up; a panel on the rail opens its flyout
    pub fn toggle(&mut self, p: Panel) {
        if self.is_visible(p) && !self.is_on_rail(p) { self.close(p) } else { self.focus(p) }
    }

    /// Whether `p` waits on the rail as an icon
    pub fn is_on_rail(&self, p: Panel) -> bool {
        self.workspace.layout.root.as_ref().and_then(rail_column).is_some_and(|column| column.groups.iter().any(|group| group.panels.contains(&p)))
    }

    pub fn close(&mut self, p: Panel) {
        if let Err(error) = self.workspace.set_visible(p, false, None, permissions) {
            log::warn!("panel {p:?} was not closed: {error}");
        }
    }

    /// Open `p` (where it was when closed, else where it lives by default) and bring it to the
    /// front.
    pub fn focus(&mut self, p: Panel) {
        let mut next = self.workspace.clone();
        match next.set_visible(p, true, None, permissions) {
            Ok(()) if validate(&next).is_ok() => {
                self.workspace = next;
                self.raise = Some(p);
            }
            Ok(()) => log::warn!("panel {p:?} was not opened: it would break the viewport's rules"),
            Err(error) => log::warn!("panel {p:?} was not opened: {error}"),
        }
    }

    /// One change from the dock area's gestures. A change that is rejected leaves the layout as
    /// it was.
    fn apply(&mut self, action: Action<Panel>) {
        let mut next = self.workspace.clone();
        match next.apply(action, permissions) {
            Ok(_) if validate(&next).is_ok() => self.workspace = next,
            Ok(_) => log::warn!("a panel move would break the viewport's rules"),
            Err(error) => log::warn!("a panel move was not applied: {error}"),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Drawing

fn area_id() -> Id {
    Id::unique("texturecraft-docking")
}

/// The dock look: every panel group is a card on the window's frame colour, its tab strip
/// without hard lines, the shown tab a pill.
fn style(ui: &Ui, t: &Tokens) -> DockStyle {
    let mut s = DockStyle::from_ui(ui);
    s.gap = f32::from(crate::theme::GUTTER);
    // Smallest panel extent a divider drag leaves.
    s.min_pane = 60.0;
    s.background = t.panel;
    s.tab_background = t.panel;
    s.active_background = t.button;
    s.text = t.text_strong;
    s.inactive_text = t.text_dim;
    s.border = egui::Stroke::NONE;
    s.accent = t.accent;
    s.floating_background = t.panel;
    s.floating_stroke = egui::Stroke::new(1.0, t.button_border);
    s.font = egui::FontId::proportional(12.5);
    s.min_tab_width = 54.0;
    s.iconic_width = RAIL_WIDTH;
    // The rail keeps its width: no dragging its edge to widen it
    s.iconic_max_width = RAIL_WIDTH;
    // The rail is the only dock column and stays icons only: no bar to expand it or drag it
    s.column_bar_height = 0.0;
    s.tab_height = 30.0;
    // Panels arrange in a split tree around the viewport; floating title bars only move windows.
    s.new_dock_zones = false;
    s.title_bar_docks = false;
    s
}

/// Draw the dock (viewport and panels) into `ui`.
pub fn show(app: &mut TextureCraftApp, ui: &mut Ui) {
    for panel in Panel::ALL {
        if !panel.available_in(app.ui.mode) && app.ui.dock.is_open(panel) {
            app.ui.dock.close(panel);
        }
    }

    // A rail panel closed where it was docked goes back to the rail; bake mode has no rail
    if app.ui.mode != crate::state::Mode::Baking {
        return_to_rail(&mut app.ui.dock.workspace);
    }

    let context = ui.ctx().clone();
    let tokens = Tokens::get(&context);
    let style = style(ui, &tokens);
    let layout = app.ui.dock.workspace.layout.clone();
    // The rail's flyout closes on a click outside it, as a menu does
    let mut area = DockArea::new(area_id()).auto_collapse_iconic(true).input_blocked(context.memory(|memory| memory.top_modal_layer()).is_some());
    if let Some(panel) = app.ui.dock.raise.take() {
        area = area.raise_panel(panel);
    }
    let output = area.show_content(ui, &layout, &style, &mut Panels { app, tk: tokens });
    #[cfg(not(target_arch = "wasm32"))]
    crate::menus::forwarded_shortcuts(app, &context, output.forwarded_events, output.forwarded_modifiers.unwrap_or_default());
    if let Some(error) = &output.error {
        log::warn!("docking: {error}");
    }
    // A layout changed by a panel takes priority over gestures from the old layout
    if app.ui.dock.workspace.layout != layout {
        craft_ui::docking::cancel_drag::<Panel>(&context, area_id());
        return;
    }
    for action in output.actions {
        app.ui.dock.apply(action);
    }
}

/// A vertical scroll area whose parent sees its whole content height, so a flyout fitted to the content grows to show all of it
pub(crate) fn fitting_scroll_area(ui: &mut Ui, id: &str, body: impl FnOnce(&mut Ui)) {
    let output = egui::ScrollArea::vertical().id_salt(id).auto_shrink([false, true]).show(ui, body);
    let hidden = output.content_size.y - output.inner_rect.height();
    if hidden > 0.0 {
        ui.allocate_space(vec2(0.0, hidden));
    }
}

/// The panels' contents, names and limits for the dock area.
struct Panels<'a> {
    app: &'a mut TextureCraftApp,
    tk: Tokens,
}

impl DockContent<Panel> for Panels<'_> {
    fn body(&mut self, ui: &mut Ui, panel: &Panel) {
        let app = &mut *self.app;
        if *panel == Panel::Viewport {
            ui.painter().rect_filled(ui.max_rect(), 0.0, self.tk.viewport);
            crate::viewport::central(app, ui);
            return;
        }
        egui::Frame::new().inner_margin(Margin::symmetric(8, 4)).show(ui, |ui| match panel {
            Panel::MeshMapSettings => crate::bake_mode::mesh_map_settings_panel(app, ui),
            Panel::Assets => crate::shelf::body(app, ui),
            Panel::CommonSettings => crate::bake_mode::common_settings_panel(app, ui),
            Panel::MeshMapBakers => crate::bake_mode::mesh_map_bakers_panel(app, ui),
            Panel::TextureSetList if app.ui.mode == crate::state::Mode::Baking => crate::bake_mode::texture_sets(app, ui),
            Panel::TextureSetList => crate::dock::texture_set_list(app, ui),
            Panel::Layers => crate::dock::layers(app, ui),
            Panel::TextureSetSettings => crate::dock::texture_set_settings(app, ui),
            Panel::Properties => crate::dock::properties(app, ui),
            // The other panels scroll their own lists.
            Panel::Display => fitting_scroll_area(ui, "dock-display", |ui| crate::windows::display(app, ui)),
            Panel::Shader => fitting_scroll_area(ui, "dock-shader", |ui| crate::windows::shader(app, ui)),
            Panel::History => crate::windows::history(app, ui),
            Panel::Log => crate::windows::log(app, ui),
            Panel::Viewport => {}
        });
    }

    fn paint_icon(&mut self, painter: &egui::Painter, rect: egui::Rect, panel: &Panel, color: egui::Color32) -> bool {
        let Some(icon) = panel.rail_icon() else { return false };

        painter.text(rect.center(), egui::Align2::CENTER_CENTER, icon, egui::FontId::proportional(rect.height()), color);
        true
    }

    fn logical_label(&self, panel: &Panel) -> String {
        match panel {
            Panel::Properties => crate::dock::properties_title(self.app),
            p => tr(p.name()).to_string(),
        }
    }

    fn label(&mut self, _ui: &Ui, _panel: &Panel, logical: &str, font: &egui::FontId, color: egui::Color32) -> egui::WidgetText {
        RichText::new(logical).font(font.clone()).color(color).into()
    }

    fn permissions(&self, panel: &Panel) -> Permissions {
        permissions(panel)
    }

    fn limits(&self, panel: &Panel) -> PanelLimits {
        let min = if *panel == Panel::Viewport { vec2(200.0, 150.0) } else { vec2(120.0, 60.0) };
        PanelLimits { min, ..PanelLimits::default() }
    }

    fn floating_size(&self, panel: &Panel) -> Option<egui::Vec2> {
        Some(panel.floating_height().map_or(DEFAULT_FLOATING_SIZE, |height| vec2(RAIL_FLYOUT_WIDTH, height)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn panels_of(w: &Workspace) -> Vec<Panel> {
        w.layout.panels().into_iter().copied().collect()
    }

    #[test]
    fn default_workspace_has_every_panel_and_is_valid() {
        let w = default_workspace();
        assert!(validate(&w).is_ok());
        for panel in Panel::ALL {
            assert!(w.layout.contains(&panel) || w.hidden.iter().any(|(p, _)| *p == panel), "{panel:?} is neither shown nor hidden");
        }
        assert_eq!(
            panels_of(&w),
            [
                Panel::Assets,
                Panel::Viewport,
                Panel::TextureSetList,
                Panel::Layers,
                Panel::TextureSetSettings,
                Panel::Properties,
                Panel::Display,
                Panel::Shader,
                Panel::History,
                Panel::Log
            ]
        );
    }

    #[test]
    fn default_dock_round_trips() {
        let dock = Dock::default();
        let json = serde_json::to_string(&dock).unwrap();
        let back: Dock = serde_json::from_str(&json).unwrap();
        assert_eq!(back, dock);
    }

    #[test]
    fn the_bake_dock_round_trips_with_the_log_under_both_settings_columns() {
        let dock = Dock::for_baking();
        assert!(validate(dock.workspace()).is_ok());
        assert_eq!(
            panels_of(dock.workspace()),
            [Panel::TextureSetList, Panel::MeshMapBakers, Panel::CommonSettings, Panel::MeshMapSettings, Panel::Log, Panel::Viewport]
        );
        assert!(dock.is_visible(Panel::CommonSettings) && !dock.is_visible(Panel::MeshMapSettings));

        let back: Dock = serde_json::from_str(&serde_json::to_string(&dock).unwrap()).unwrap();
        assert_eq!(back, dock);
    }

    #[test]
    fn unreadable_layouts_become_the_default() {
        for text in [
            // A layout in another shape.
            r#"{"main":{"tabs":{"tabs":["Viewport"]}},"windows":[]}"#,
            // Not a layout.
            r#"[1,2]"#,
            r#""dock""#,
            // A shared workspace with a panel this build does not know.
            r#"{"version":2,"layout":{"root":{"Tabs":{"panels":["Bogus"],"active":0}},"floating":[]}}"#,
            // A shared workspace from a newer format.
            r#"{"version":99,"layout":{"root":{"Tabs":{"panels":["Viewport"],"active":0}},"floating":[]}}"#,
            // A viewport that floats.
            r#"{"version":2,"layout":{"root":{"Tabs":{"panels":["Assets"],"active":0}},"floating":[{"panels":["Viewport"],"active":0,"rect":[0,0,200,200]}]}}"#,
        ] {
            let dock: Dock = serde_json::from_str(text).unwrap();
            assert_eq!(dock, Dock::default(), "{text}");
        }
    }

    #[test]
    fn the_viewport_stays_put() {
        let mut dock = Dock::default();
        dock.close(Panel::Viewport);
        assert!(dock.is_open(Panel::Viewport));
        let before = dock.clone();
        dock.apply(Action::Float { panel: Panel::Viewport, rect: [0.0, 0.0, 300.0, 300.0] });
        dock.apply(Action::Move { panel: Panel::Layers, anchor: Panel::Viewport, placement: Placement::Tab { before: None } });
        assert_eq!(dock, before);
    }

    #[test]
    fn closed_panels_come_back_where_they_were() {
        let mut dock = Dock::default();
        let before = dock.clone();
        for p in [Panel::Properties, Panel::Layers, Panel::Assets] {
            dock.close(p);
            assert!(!dock.is_open(p));
            // The places persist with the layout.
            dock = serde_json::from_str(&serde_json::to_string(&dock).unwrap()).unwrap();
            dock.focus(p);
            assert!(dock.is_visible(p), "{p:?}");
            assert_eq!(dock, before, "{p:?} came back elsewhere");
        }
    }

    #[test]
    fn rail_panels_closed_elsewhere_return_to_the_rail() {
        let mut dock = Dock::default();
        assert!(dock.is_on_rail(Panel::Display) && !dock.is_on_rail(Panel::TextureSetSettings));
        // A view's toggle opens a rail panel's flyout instead of closing it
        dock.toggle(Panel::Display);
        assert!(dock.is_on_rail(Panel::Display));

        dock.close(Panel::TextureSetSettings);
        return_to_rail(&mut dock.workspace);
        assert!(dock.is_on_rail(Panel::TextureSetSettings));
        assert!(validate(dock.workspace()).is_ok());
    }

    #[test]
    fn a_rail_panel_docked_elsewhere_leaves_the_rail() {
        let mut dock = Dock::default();
        dock.apply(Action::Move { panel: Panel::Display, anchor: Panel::Properties, placement: Placement::Tab { before: None } });
        assert!(!dock.is_on_rail(Panel::Display) && dock.is_visible(Panel::Display));

        dock.close(Panel::Display);
        return_to_rail(&mut dock.workspace);
        assert!(dock.is_on_rail(Panel::Display));
    }

    #[test]
    fn a_saved_layout_without_a_rail_gets_one() {
        let mut workspace = default_workspace();
        let Some(Node::Split { first, .. }) = workspace.layout.root.take() else { panic!("the rail splits off the root") };
        workspace.layout.root = Some(*first);
        assert!(validate(&workspace).is_ok(), "a layout from before the rail");

        return_to_rail(&mut workspace);
        assert_eq!(workspace, default_workspace());
    }

    #[test]
    fn bake_mode_opens_the_rail_panels_floating() {
        let mut dock = Dock::for_baking();
        dock.focus(Panel::Display);
        assert!(dock.workspace().layout.floating.iter().any(|window| window.contains(&Panel::Display)));
    }
}
