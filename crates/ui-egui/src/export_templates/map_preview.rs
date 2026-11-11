//! The file an output writes, small, with each of its channels: shown while one of its channel boxes is hovered

use std::hash::{Hash, Hasher};
use std::sync::Arc;

use egui::{Color32, ColorImage, RichText, TextureHandle, TextureOptions, Ui, vec2};
use serde_json::Value;
use texturecraft_engine::formats::{Dest, MapPreview};

use crate::TextureCraftApp;
use crate::theme::Tokens;

/// Texels across a preview: enough to read the packing, cheap to sample
const PREVIEW_SIDE: u32 = 150;
const PICTURE_WIDTH: f32 = 150.0;
const CHANNEL_SIZE: f32 = 34.0;

/// One output's pictures: the file as it looks, then each of its channels in grey
pub(super) struct PreviewPictures {
    file: TextureHandle,
    channels: Vec<(Dest, TextureHandle)>,
    /// Height over width
    aspect: f32,
}

/// None while the texture set writes no file for the output
pub(super) type Preview = Option<Arc<PreviewPictures>>;

/// What a hovered channel box says about its output besides the pictures
pub(super) struct PreviewText {
    /// The file written, or None when the texture set writes none
    pub(super) file: Option<String>,
    /// "2048 x 2048 · png 8 bits"
    pub(super) details: String,
    /// Each channel's source, in the file's channel order
    pub(super) sources: Vec<(Dest, String)>,
}

/// Output `mi`'s pictures for the export params `params`; made again only when the params change
pub(super) fn preview(app: &mut TextureCraftApp, ctx: &egui::Context, params: &Value, mi: usize) -> Preview {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    params.to_string().hash(&mut hasher);
    let key = hasher.finish();
    let id = egui::Id::new(("exp_map_preview", mi));
    if let Some((cached_key, cached)) = ctx.data(|d| d.get_temp::<(u64, Preview)>(id))
        && cached_key == key
    {
        return cached;
    }

    let made = app.session.export_preview(params, mi, PREVIEW_SIDE).ok().flatten().map(|preview| Arc::new(pictures(ctx, &preview, mi)));
    ctx.data_mut(|d| d.insert_temp(id, (key, made.clone())));
    made
}

fn pictures(ctx: &egui::Context, preview: &MapPreview, mi: usize) -> PreviewPictures {
    let (width, height) = (preview.width as usize, preview.height as usize);
    let n = preview.channels.len().max(1);
    let value = |texel: usize, k: usize| preview.pixels.get(texel * n + k).copied().unwrap_or(0);
    let color = preview.channels.starts_with(&[Dest::R, Dest::G, Dest::B]);
    let file: Vec<Color32> = (0..width * height)
        .map(|texel| if color { Color32::from_rgb(value(texel, 0), value(texel, 1), value(texel, 2)) } else { Color32::from_gray(value(texel, 0)) })
        .collect();
    let load = |name: String, image: ColorImage| ctx.load_texture(name, image, TextureOptions::LINEAR);

    let channels = match preview.channels.len() {
        0 | 1 => Vec::new(),
        _ => (0..n)
            .zip(&preview.channels)
            .map(|(k, dest)| (*dest, load(format!("exp_preview_{mi}_{k}"), ColorImage::from_gray_iter([width, height], (0..width * height).map(|texel| value(texel, k))))))
            .collect(),
    };

    PreviewPictures { file: load(format!("exp_preview_{mi}"), ColorImage::new([width, height], file)), channels, aspect: height as f32 / width.max(1) as f32 }
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
        ui.add(egui::Image::new(&pictures.file).fit_to_exact_size(vec2(PICTURE_WIDTH, PICTURE_WIDTH * pictures.aspect)).corner_radius(3));
        if !pictures.channels.is_empty() {
            ui.vertical(|ui| {
                for (dest, texture) in &pictures.channels {
                    ui.horizontal(|ui| {
                        ui.add(egui::Image::new(texture).fit_to_exact_size(vec2(CHANNEL_SIZE, CHANNEL_SIZE * pictures.aspect)).corner_radius(3));
                        let source = text.sources.iter().find(|(d, _)| d == dest).map_or("", |(_, name)| name.as_str());
                        ui.label(RichText::new(format!("{dest:?}")).color(tk.text_strong));
                        ui.label(RichText::new(source).size(11.0).color(tk.text_dim));
                    });
                }
            });
        }
    });
    ui.label(RichText::new(&text.details).size(11.5).color(tk.text_dim));
}
