//! X11: while the pointer is off the app's windows the pointer is grabbed, so a left click there comes to the app instead of the
//! window under it, and the one root-window pixel under it is read; over the app's windows the grab lets go and clicks stay the app's;
//! the grab shows the eyedropper pointer, so the pointer is the eyedropper over every other window too

use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::time::Duration;

use x11rb::connection::Connection;
use x11rb::protocol::Event;
use x11rb::protocol::render::{ConnectionExt as _, CreatePictureAux, PictType};
use x11rb::protocol::xproto::{ConnectionExt, CreateGCAux, Cursor, EventMask, GrabMode, GrabStatus, ImageFormat, ImageOrder, Window};
use x11rb::rust_connection::RustConnection;

use super::pick_shared::over_windows;
use super::x11_pixel::{PixelLayout, premultiplied_argb};
use super::{CursorImage, PickOutcome, PickShared};

/// How often the pointer's place is checked against the app's windows
const POLL: Duration = Duration::from_millis(15);
/// The X pointer button a left click presses
const LEFT_BUTTON: u8 = 1;
/// The pointer size where the desktop names none (`XCURSOR_SIZE`), in pixels
const DEFAULT_POINTER_PX: u32 = 24;
/// The largest pointer sent in one request, in pixels
const MAX_POINTER_PX: u32 = 64;
/// The colour depth of a pointer picture with alpha
const ARGB_DEPTH: u8 = 32;

/// Waits for a left click off the app's windows until it lands or `shared` stops the pick; `None` when there is no X server
pub(super) fn run(shared: &Arc<PickShared>, outcomes: &Sender<PickOutcome>, ctx: &egui::Context) -> Option<()> {
    let (connection, screen) = x11rb::connect(None).ok()?;
    let root = connection.setup().roots.get(screen)?.root;
    let layout = pixel_layout(&connection, screen)?;
    let pointer_shape = eyedropper_pointer(&connection, root).unwrap_or(x11rb::NONE);
    let mut grabbed = false;
    let mut pressed = false;
    while pressed || !shared.stopped() {
        let pointer = connection.query_pointer(root).ok()?.reply().ok()?;
        let windows = shared.windows();
        let outside = !windows.is_empty() && !over_windows(&windows, pointer.root_x.into(), pointer.root_y.into());
        if outside && !grabbed && !shared.stopped() {
            grabbed = grab(&connection, root, pointer_shape);
        } else if !outside && grabbed && !pressed {
            grabbed = !ungrab(&connection);
        }

        while let Ok(Some(event)) = connection.poll_for_event() {
            match event {
                Event::ButtonPress(press) if grabbed && press.detail == LEFT_BUTTON && !pressed => {
                    let picked = root_pixel(&connection, root, layout, press.root_x, press.root_y);
                    let _ = outcomes.send(picked.map_or(PickOutcome::Unavailable, PickOutcome::Picked));
                    ctx.request_repaint();
                    pressed = true;
                }
                Event::ButtonRelease(release) if pressed && release.detail == LEFT_BUTTON => {
                    ungrab(&connection);
                    return Some(());
                }
                _ => {}
            }
        }
        std::thread::sleep(POLL);
    }
    if grabbed {
        ungrab(&connection);
    }
    Some(())
}

/// Takes the pointer, showing `pointer_shape` over every window while it is held
fn grab(connection: &RustConnection, root: Window, pointer_shape: Cursor) -> bool {
    let mask = EventMask::BUTTON_PRESS | EventMask::BUTTON_RELEASE;
    let Ok(cookie) = connection.grab_pointer(false, root, mask, GrabMode::ASYNC, GrabMode::ASYNC, x11rb::NONE, pointer_shape, x11rb::CURRENT_TIME) else {
        return false;
    };

    cookie.reply().is_ok_and(|reply| reply.status == GrabStatus::SUCCESS)
}

/// Lets go of the pointer; whether the server took the request
fn ungrab(connection: &RustConnection) -> bool {
    connection.ungrab_pointer(x11rb::CURRENT_TIME).is_ok() && connection.flush().is_ok()
}

/// The eyedropper as an X pointer with its hotspot on the tip, where the server can show one (freed with the connection)
fn eyedropper_pointer(connection: &RustConnection, root: Window) -> Option<Cursor> {
    connection.render_query_version(0, 5).ok()?.reply().ok()?;
    let formats = connection.render_query_pict_formats().ok()?.reply().ok()?;
    let argb = formats.formats.iter().find(|format| {
        let direct = format.direct;
        format.type_ == PictType::DIRECT
            && format.depth == ARGB_DEPTH
            && [direct.alpha_shift, direct.red_shift, direct.green_shift, direct.blue_shift] == [24, 16, 8, 0]
            && [direct.alpha_mask, direct.red_mask, direct.green_mask, direct.blue_mask] == [0xff; 4]
    })?;
    let size = std::env::var("XCURSOR_SIZE").ok().and_then(|size| size.trim().parse().ok()).unwrap_or(DEFAULT_POINTER_PX);
    let image = CursorImage::eyedropper(size.min(MAX_POINTER_PX));
    let side = u16::try_from(image.size).ok()?;
    let [hot_x, hot_y] = image.hotspot.map(|edge| u16::try_from(edge).unwrap_or_default());
    let least_significant_first = connection.setup().image_byte_order == ImageOrder::LSB_FIRST;

    let pixmap = connection.generate_id().ok()?;
    connection.create_pixmap(ARGB_DEPTH, pixmap, root, side, side).ok()?;
    let gc = connection.generate_id().ok()?;
    connection.create_gc(gc, pixmap, &CreateGCAux::new()).ok()?;
    let data = premultiplied_argb(&image.rgba, least_significant_first);
    connection.put_image(ImageFormat::Z_PIXMAP, pixmap, gc, side, side, 0, 0, 0, ARGB_DEPTH, &data).ok()?;
    let picture = connection.generate_id().ok()?;
    connection.render_create_picture(picture, pixmap, argb.id, &CreatePictureAux::new()).ok()?;
    let cursor = connection.generate_id().ok()?;
    // Checked: a pointer the server refused would make every grab fail
    let made = connection.render_create_cursor(cursor, picture, hot_x, hot_y).ok()?.check().is_ok();
    let _ = connection.render_free_picture(picture);
    let _ = connection.free_gc(gc);
    let _ = connection.free_pixmap(pixmap);
    made.then_some(cursor)
}

/// How the root window's pixels are laid out on `screen`
fn pixel_layout(connection: &RustConnection, screen: usize) -> Option<PixelLayout> {
    let setup = connection.setup();
    let root = setup.roots.get(screen)?;
    let format = setup.pixmap_formats.iter().find(|format| format.depth == root.root_depth)?;
    let visual = root.allowed_depths.iter().flat_map(|depth| depth.visuals.iter()).find(|visual| visual.visual_id == root.root_visual)?;
    Some(PixelLayout {
        bits_per_pixel: format.bits_per_pixel,
        least_significant_first: setup.image_byte_order == ImageOrder::LSB_FIRST,
        masks: [visual.red_mask, visual.green_mask, visual.blue_mask],
    })
}

/// The colour of the one root-window pixel at `(x, y)` (the root spans every monitor, in physical pixels)
fn root_pixel(connection: &RustConnection, root: Window, layout: PixelLayout, x: i16, y: i16) -> Option<[u8; 3]> {
    let image = connection.get_image(ImageFormat::Z_PIXMAP, root, x, y, 1, 1, u32::MAX).ok()?.reply().ok()?;
    layout.decode(&image.data)
}
