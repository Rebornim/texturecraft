//! The file an output writes, small, with each of its channels: shown while one of its channel boxes is hovered

use std::hash::{Hash, Hasher};
use std::sync::Arc;

use egui::{Color32, ColorImage, RichText, TextureOptions, Ui, vec2};
use serde_json::Value;
use texturecraft_engine::formats::{Dest, MapPreview};

use super::preview_pictures::PreviewPictures;
use super::preview_text::PreviewText;
use crate::TextureCraftApp;
use crate::theme::Tokens;

/// Texels across a preview: enough to read the packing, cheap to sample
const PREVIEW_SIDE: u32 = 150;
const PICTURE_WIDTH: f32 = 150.0;
const CHANNEL_SIZE: f32 = 34.0;
/// Corner radius of the file and channel pictures
const PICTURE_CORNER_RADIUS: u8 = 3;
/// Size of a channel's source name and of the file details line
const SOURCE_TEXT_SIZE: f32 = 11.0;
const DETAILS_TEXT_SIZE: f32 = 11.5;

/// None while the texture set writes no file for the output
pub(super) type Preview = Option<Arc<PreviewPictures>>;

/// Output `output_index`'s pictures for the export params `params`; made again when the params or the painting change
pub(super) fn preview(app: &mut TextureCraftApp, ctx: &egui::Context, params: &Value, output_index: usize) -> Preview {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    params.to_string().hash(&mut hasher);
    app.session.revision.hash(&mut hasher);
    app.session.caches.iter().map(|cache| cache.revision).for_each(|revision| revision.hash(&mut hasher));
    let key = hasher.finish();
    let id = egui::Id::unique(("exp_map_preview", output_index));
    if let Some((cached_key, cached)) = ctx.data(|data| data.get_temp::<(u64, Preview)>(id))
        && cached_key == key
    {
        return cached;
    }

    let made = app.session.export_preview(params, output_index, PREVIEW_SIDE).ok().flatten().map(|preview| Arc::new(pictures(ctx, &preview, output_index)));
    ctx.data_mut(|data| data.insert_temp(id, (key, made.clone())));
    made
}

fn pictures(ctx: &egui::Context, preview: &MapPreview, output_index: usize) -> PreviewPictures {
    let (width, height) = (preview.width as usize, preview.height as usize);
    let component_count = preview.channels.len().max(1);
    let value = |texel: usize, component: usize| preview.pixels.get(texel * component_count + component).copied().unwrap_or(0);
    let color = preview.channels.starts_with(&[Dest::R, Dest::G, Dest::B]);
    let file: Vec<Color32> = (0..width * height)
        .map(|texel| if color { Color32::from_rgb(value(texel, 0), value(texel, 1), value(texel, 2)) } else { Color32::from_gray(value(texel, 0)) })
        .collect();
    let load = |name: String, image: ColorImage| ctx.load_texture(name, image, TextureOptions::LINEAR);

    let channels = match preview.channels.len() {
        0 | 1 => Vec::new(),
        _ => (0..component_count)
            .zip(&preview.channels)
            .map(|(component, dest)| {
                (
                    *dest,
                    load(
                        format!("exp_preview_{output_index}_{component}"),
                        ColorImage::from_gray_iter([width, height], (0..width * height).map(|texel| value(texel, component))),
                    ),
                )
            })
            .collect(),
    };

    PreviewPictures {
        file: load(format!("exp_preview_{output_index}"), ColorImage::new([width, height], file)),
        channels,
        aspect: height as f32 / width.max(1) as f32,
    }
}

/// The hover contents: the file name, the file, its channels with their sources, then its size and type
pub(super) fn show(ui: &mut Ui, pictures: &Preview, text: &PreviewText) {
    let tk = Tokens::get(ui.ctx());
    let (Some(pictures), Some(file)) = (pictures, &text.file) else {
        ui.label(RichText::new(crate::i18n::tr("Not exported: no source in this texture set")).color(tk.text_dim));
        return;
    };

    ui.label(RichText::new(file).color(tk.text_strong));
    ui.horizontal_top(|ui| {
        ui.add(egui::Image::new(&pictures.file).fit_to_exact_size(vec2(PICTURE_WIDTH, PICTURE_WIDTH * pictures.aspect)).corner_radius(PICTURE_CORNER_RADIUS));
        if !pictures.channels.is_empty() {
            ui.vertical(|ui| {
                for (dest, texture) in &pictures.channels {
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::Image::new(texture)
                                .fit_to_exact_size(vec2(CHANNEL_SIZE, CHANNEL_SIZE * pictures.aspect))
                                .corner_radius(PICTURE_CORNER_RADIUS),
                        );
                        let source = text.sources.iter().find(|(source_dest, _)| source_dest == dest).map_or("", |(_, name)| name.as_str());
                        // A grey channel reads "Gr", as its box does
                        let letter = if *dest == Dest::L { "Gr".to_string() } else { format!("{dest:?}") };
                        ui.label(RichText::new(letter).color(tk.text_strong));
                        ui.label(RichText::new(source).size(SOURCE_TEXT_SIZE).color(tk.text_dim));
                    });
                }
            });
        }
    });
    ui.label(RichText::new(&text.details).size(DETAILS_TEXT_SIZE).color(tk.text_dim));
}
