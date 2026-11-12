//! X11: while the pointer is off the app's windows the pointer is grabbed, so a left click there comes to the app instead of the
//! window under it, and the one root-window pixel under it is read; over the app's windows the grab lets go and clicks stay the app's

use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::time::Duration;

use x11rb::connection::Connection;
use x11rb::protocol::Event;
use x11rb::protocol::xproto::{ConnectionExt, EventMask, GrabMode, GrabStatus, ImageFormat, ImageOrder, Window};
use x11rb::rust_connection::RustConnection;

use super::pick_shared::over_windows;
use super::x11_pixel::PixelLayout;
use super::{PickOutcome, PickShared};

/// How often the pointer's place is checked against the app's windows
const POLL: Duration = Duration::from_millis(15);
/// The X pointer button a left click presses
const LEFT_BUTTON: u8 = 1;

/// Waits for a left click off the app's windows until it lands or `shared` stops the pick; `None` when there is no X server
pub(super) fn run(shared: &Arc<PickShared>, outcomes: &Sender<PickOutcome>, ctx: &egui::Context) -> Option<()> {
    let (connection, screen) = x11rb::connect(None).ok()?;
    let root = connection.setup().roots.get(screen)?.root;
    let layout = pixel_layout(&connection, screen)?;
    let mut grabbed = false;
    let mut pressed = false;
    while pressed || !shared.stopped() {
        let pointer = connection.query_pointer(root).ok()?.reply().ok()?;
        let windows = shared.windows();
        let outside = !windows.is_empty() && !over_windows(&windows, pointer.root_x.into(), pointer.root_y.into());
        if outside && !grabbed && !shared.stopped() {
            grabbed = grab(&connection, root);
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

fn grab(connection: &RustConnection, root: Window) -> bool {
    let mask = EventMask::BUTTON_PRESS | EventMask::BUTTON_RELEASE;
    let Ok(cookie) = connection.grab_pointer(false, root, mask, GrabMode::ASYNC, GrabMode::ASYNC, x11rb::NONE, x11rb::NONE, x11rb::CURRENT_TIME) else {
        return false;
    };
    cookie.reply().is_ok_and(|reply| reply.status == GrabStatus::SUCCESS)
}

/// Lets go of the pointer; whether the server took the request
fn ungrab(connection: &RustConnection) -> bool {
    connection.ungrab_pointer(x11rb::CURRENT_TIME).is_ok() && connection.flush().is_ok()
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
