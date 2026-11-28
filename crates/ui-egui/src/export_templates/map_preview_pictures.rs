use egui::{Color32, ColorImage, TextureHandle, TextureOptions};
use texturecraft_engine::formats::{Dest, MapPreview};

/// An output map's file, small, then each of its channels in grey, as textures to draw
pub(super) struct MapPreviewPictures {
    pub(super) file: TextureHandle,
    /// Empty for a one-channel file, whose picture is its channel
    pub(super) channels: Vec<(Dest, TextureHandle)>,
    /// Height over width
    pub(super) aspect: f32,
}

impl MapPreviewPictures {
    /// The pictures of `preview`, the preview of output map `index`
    pub(super) fn new(ctx: &egui::Context, preview: &MapPreview, index: usize) -> MapPreviewPictures {
        let size = [preview.width as usize, preview.height as usize];
        let pixel_count = size[0] * size[1];
        let count = preview.channels.len().max(1);
        let value = |pixel: usize, channel: usize| preview.pixels.get(pixel * count + channel).copied().unwrap_or(0);
        let load = |name: String, image: ColorImage| ctx.load_texture(name, image, TextureOptions::LINEAR);

        // Keep alpha separate so the file preview shows the stored colour
        let color = preview.channels.starts_with(&[Dest::R, Dest::G, Dest::B]);
        let pixels: Vec<_> = (0..pixel_count)
            .map(|pixel| if color { Color32::from_rgb(value(pixel, 0), value(pixel, 1), value(pixel, 2)) } else { Color32::from_gray(value(pixel, 0)) })
            .collect();
        let grayscale = !color || pixels.iter().all(|pixel| pixel.r() == pixel.g() && pixel.g() == pixel.b());
        let file = load(format!("export_map_preview_{index}"), ColorImage::new(size, pixels));

        // A gray file texture can also draw each of its colour channels
        let channels = if preview.channels.len() > 1 {
            let channel = |(channel_index, dest): (usize, &Dest)| {
                if grayscale && *dest != Dest::A {
                    return (*dest, file.clone());
                }

                let image = ColorImage::from_gray_iter(size, (0..pixel_count).map(|pixel| value(pixel, channel_index)));
                (*dest, load(format!("export_map_preview_{index}_{channel_index}"), image))
            };
            preview.channels.iter().enumerate().map(channel).collect()
        } else {
            Vec::new()
        };

        MapPreviewPictures { file, channels, aspect: preview.height as f32 / preview.width.max(1) as f32 }
    }
}
