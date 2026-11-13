//! The picture a slot card shows of a fill channel's source

use egui::{Align2, Color32, CornerRadius, FontId, Painter, Rect, TextureHandle, TextureId, pos2};
use egui_phosphor::regular as ic;
use texturecraft_engine::compose::ThumbTarget;
use texturecraft_engine::core::Source;

use crate::TextureCraftApp;
use crate::fill_material_panel::shown_color;
use crate::theme::Tokens;

/// The picture a slot shows of its source
pub(crate) enum Thumbnail {
    Swatch(Color32),
    Texture(TextureHandle),
    /// A picture of the layer stack (an anchor point's), drawn over the field colour where the stack has nothing
    Stack(TextureId),
    Glyph(&'static str),
}

/// The picture of `source`: its uniform colour, the procedural's or image's thumbnail, the anchored stack, a glyph for what has none
pub(crate) fn thumbnail(app: &mut TextureCraftApp, ctx: &egui::Context, source: &Source) -> Thumbnail {
    match source {
        Source::Uniform(value) => Thumbnail::Swatch(shown_color(*value)),
        Source::Procedural { pattern, .. } => app.thumbs.procedural(ctx, pattern.kind).map_or(Thumbnail::Glyph(ic::WAVE_SINE), Thumbnail::Texture),
        Source::Image { name, .. } => match app.session.images.get(name).cloned() {
            Some(image) => Thumbnail::Texture(app.thumbs.image(ctx, name, &image)),
            None => Thumbnail::Glyph(ic::IMAGE),
        },
        // The stack the anchor point keeps, as the Layers panel shows that point (the glyph while it renders)
        Source::Anchor { anchor, channel } => app
            .layer_panel
            .texture(ctx, &app.session, ThumbTarget::Stage { effect: *anchor, channel: *channel })
            .map_or(Thumbnail::Glyph(ic::ANCHOR), Thumbnail::Stack),
    }
}

/// Paints `thumbnail` into a slot's square
pub(crate) fn paint_thumbnail(painter: &Painter, rect: Rect, thumbnail: &Thumbnail, tokens: &Tokens) {
    match thumbnail {
        Thumbnail::Swatch(color) => {
            painter.rect_filled(rect, CornerRadius::ZERO, *color);
        }
        Thumbnail::Texture(texture) => {
            painter.image(texture.id(), rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        }
        Thumbnail::Stack(texture) => {
            painter.rect_filled(rect, CornerRadius::ZERO, tokens.field);
            painter.image(*texture, rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        }
        Thumbnail::Glyph(glyph) => {
            painter.rect_filled(rect, CornerRadius::ZERO, tokens.field);
            painter.text(rect.center(), Align2::CENTER_CENTER, *glyph, FontId::proportional(14.0), tokens.text_dim);
        }
    }
}
