//! The second Assets panel, opened by the folder button: the same panel with its own query

use egui::Ui;

use crate::TextureCraftApp;
use crate::shelf_query::ShelfQuery;

/// Swap `query` with the first Assets panel's query in `ui`
fn swap_query(ui: &mut crate::state::UiState, query: &mut ShelfQuery) {
    std::mem::swap(&mut ui.shelf_filter, &mut query.filter);
    std::mem::swap(&mut ui.shelf_tab, &mut query.tab);
    std::mem::swap(&mut ui.shelf_path, &mut query.path);
    std::mem::swap(&mut ui.shelf_favorites_only, &mut query.favorites_only);
}

/// The second Assets panel's body: the Assets panel drawn with its own query
pub(crate) fn body(app: &mut TextureCraftApp, ui: &mut Ui) {
    let mut query = std::mem::take(&mut app.ui.shelf_second);
    swap_query(&mut app.ui, &mut query);
    ui.push_id("assets_second", |ui| crate::shelf::body(app, ui));
    swap_query(&mut app.ui, &mut query);
    app.ui.shelf_second = query;
}
