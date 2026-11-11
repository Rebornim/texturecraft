//! The generator slot atop a generator's properties: its orb and name, opening a picker that swaps its kind

use egui::{Sense, Ui, vec2};
use serde_json::json;
use texturecraft_engine::core::{Generator, GeneratorKind};

use crate::TextureCraftApp;
use crate::i18n::tr;
use crate::picker_look::PickerLook;
use crate::resource_picker;
use crate::resource_picker_item::ResourcePickerItem;
use crate::resource_slot::{ResourceSlot, SLOT_HEIGHT};
use crate::shelf_preview::ShelfPreview;
use crate::tile_look::TileLook;

/// The slot of generator effect `effect`: a pick runs `effect.set {generator}`
pub(crate) fn show(app: &mut TextureCraftApp, ui: &mut Ui, effect: u64, generator: &Generator) {
    let ctx = ui.ctx().clone();
    let current = generator.kind;
    let thumbnail = app.thumbs.generator(&ctx, current);
    let name = tr(current.label());

    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), SLOT_HEIGHT), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, name));
    let popup = ui.make_persistent_id(("generator_picker", effect));
    if response.clicked() {
        resource_picker::toggle(&ctx, popup);
    }

    let highlighted = response.hovered() || egui::Popup::is_id_open(&ctx, popup);
    let slot = ResourceSlot { highlighted, focused: response.has_focus(), thumbnail: thumbnail.as_ref(), caption: tr("Generator"), name };
    slot.paint(ui, rect);

    let random: Vec<&'static str> = GeneratorKind::ALL.into_iter().filter(|kind| kind.includes_randomness()).map(GeneratorKind::id).collect();
    let look = PickerLook { tile: TileLook::LARGE_THUMBNAIL, outline_current: false, headers: false, random: &random };
    let items = |thumbs: &mut crate::thumbs::Thumbs| {
        let mut items: Vec<ResourcePickerItem> = GeneratorKind::ALL
            .into_iter()
            .filter(|kind| kind.in_picker())
            .map(|kind| ResourcePickerItem { id: kind.id(), label: tr(kind.label()), group: "", thumbnail: thumbs.generator(&ctx, kind) })
            .collect();
        items.sort_by_key(|a| a.label.to_lowercase());
        items
    };
    let preview = |id: &str| ShelfPreview::Generator(GeneratorKind::parse(id).unwrap_or(current));
    let picked = resource_picker::show_with_look(&response, popup, &mut app.thumbs, (Some(current.id()), items), preview, &look);
    if let Some(kind) = picked.filter(|kind| *kind != current.id()) {
        app.run("effect.set", json!({"effect": effect, "generator": kind}));
    }
}
