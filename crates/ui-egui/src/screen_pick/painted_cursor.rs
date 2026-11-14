//! The eyedropper pointer over the app's own window: the system's pointer is hidden there and the eyedropper picture is drawn
//! one screen pixel per picture pixel, its hotspot pixel on the screen pixel the pointer is in, with the colour disc beside it

use super::{CursorImage, SwatchSide, swatch_reach, swatch_unit};

/// The pointer cell's size in points (a system pointer's 32 pixels at 100 %)
const CURSOR_POINTS: f32 = 32.0;

/// What a pointer picture shows: its cell in pixels and the colour disc's colour and side, if any
type CursorLook = (u32, Option<([u8; 3], SwatchSide)>);

/// The eyedropper picture uploaded for one look, kept between frames
#[derive(Clone)]
struct CursorTexture {
    texture: egui::TextureHandle,
    look: CursorLook,
    size: u32,
    hotspot: [u32; 2],
}

/// Draws the eyedropper in place of the pointer over the app (and, given `area`, inside it), a disc of `swatch` (sRGB) by its tip
pub fn paint(ctx: &egui::Context, area: Option<egui::Rect>, swatch: Option<[u8; 3]>) {
    let Some(pointer) = ctx.pointer_hover_pos().filter(|pointer| area.is_none_or(|area| area.contains(*pointer))) else { return };

    ctx.set_cursor_icon(egui::CursorIcon::None);
    let pixels_per_point = ctx.pixels_per_point();
    let cell = (CURSOR_POINTS * pixels_per_point).round() as u32;
    let side = swatch.map(|color| (color, disc_side(ctx.content_rect(), pointer, pixels_per_point, cell)));
    let cursor = texture(ctx, (cell, side));
    let rect = placement(pointer, pixels_per_point, cursor.size, cursor.hotspot);
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Debug, egui::Id::unique("eyedropper_cursor")));
    let whole = egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0));
    painter.image(cursor.texture.id(), rect, whole, egui::Color32::WHITE);
}

/// Where a `size`-pixel pointer picture goes, in points, so its `hotspot` pixel covers the screen pixel `pointer` is in
pub(crate) fn placement(pointer: egui::Pos2, pixels_per_point: f32, size: u32, hotspot: [u32; 2]) -> egui::Rect {
    let pixel = (pointer.to_vec2() * pixels_per_point).floor();
    let min = (pixel - egui::vec2(hotspot[0] as f32, hotspot[1] as f32)) / pixels_per_point;
    egui::Rect::from_min_size(min.to_pos2(), egui::Vec2::splat(size as f32 / pixels_per_point))
}

/// The side of the tip whose disc stays inside the window `screen` (in points) for a pointer at `pointer`
fn disc_side(screen: egui::Rect, pointer: egui::Pos2, pixels_per_point: f32, cell: u32) -> SwatchSide {
    let unit = swatch_unit(cell);
    let tip = (pointer.to_vec2() * pixels_per_point).floor() + egui::vec2(0.5, 0.5);
    let bounds = [screen.left(), screen.top(), screen.right(), screen.bottom()].map(|edge| edge * pixels_per_point);
    SwatchSide::fitting([tip.x, tip.y], unit, swatch_reach(unit), bounds)
}

/// The eyedropper picture for `look`, uploaded again only when the look changes (the disc's colour, its side, the pixel size)
fn texture(ctx: &egui::Context, look: CursorLook) -> CursorTexture {
    let id = egui::Id::unique("eyedropper_cursor_texture");
    if let Some(cursor) = ctx.data(|data| data.get_temp::<CursorTexture>(id)).filter(|cursor| cursor.look == look) {
        return cursor;
    }

    let image = match look {
        (cell, Some((color, side))) => CursorImage::eyedropper_with_swatch(cell, color, side),
        (cell, None) => CursorImage::eyedropper(cell),
    };
    let pixels = egui::ColorImage::from_rgba_unmultiplied([image.size as usize; 2], &image.rgba);
    let texture = ctx.load_texture("eyedropper_cursor", pixels, egui::TextureOptions::NEAREST);
    let cursor = CursorTexture { texture, look, size: image.size, hotspot: image.hotspot };
    ctx.data_mut(|data| data.insert_temp(id, cursor.clone()));
    cursor
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hotspot_pixel_covers_the_pixel_the_pointer_is_in() {
        for (pointer, pixels_per_point) in [(egui::pos2(100.3, 40.9), 1.0), (egui::pos2(100.3, 40.9), 1.5), (egui::pos2(7.0, 3.25), 2.0)] {
            let image = CursorImage::eyedropper((CURSOR_POINTS * pixels_per_point).round() as u32);
            let rect = placement(pointer, pixels_per_point, image.size, image.hotspot);
            let texel = egui::vec2(1.0, 1.0) / pixels_per_point;
            let hot_min = rect.min + egui::vec2(image.hotspot[0] as f32, image.hotspot[1] as f32) * texel;
            let hot = egui::Rect::from_min_size(hot_min, texel);

            assert!(hot.contains(pointer), "{pixels_per_point}x: the tip's pixel {hot:?} holds the pointer {pointer:?}");
            let screen_pixel = (hot.min.to_vec2() * pixels_per_point).round();
            assert_eq!(screen_pixel, (pointer.to_vec2() * pixels_per_point).floor(), "{pixels_per_point}x: whole screen pixels");
        }
    }
}
