//! Wayland: an app cannot read the screen or see clicks on other windows, so once the pointer leaves the app's windows the
//! desktop's own colour picker (the xdg-desktop-portal Screenshot.PickColor call) takes the click and reports the colour

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::time::Duration;

use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

use super::{PickOutcome, PickShared};

/// How often the thread checks whether the pointer has left the app
const POLL: Duration = Duration::from_millis(30);
const PORTAL: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
/// The token that names this app's request, so its answer can be awaited before the call is made
const TOKEN: &str = "texturecraft_eyedropper";
/// The portal's answer code for a pick the user made
const RESPONSE_SUCCESS: u32 = 0;

/// Waits for the pointer to leave the app's windows, then lets the desktop's picker take one click
pub(super) fn run(shared: &Arc<PickShared>, outcomes: &Sender<PickOutcome>, ctx: &egui::Context) {
    while !shared.pointer_outside() {
        if shared.stopped() {
            return;
        }

        std::thread::sleep(POLL);
    }
    let outcome = pick_color().unwrap_or(PickOutcome::Unavailable);
    let _ = outcomes.send(outcome);
    ctx.request_repaint();
}

fn pick_color() -> Option<PickOutcome> {
    let connection = Connection::session().ok()?;
    let sender = connection.unique_name()?.trim_start_matches(':').replace('.', "_");
    let request_path = format!("{PORTAL_PATH}/request/{sender}/{TOKEN}");
    let request = Proxy::new(&connection, PORTAL, request_path, "org.freedesktop.portal.Request").ok()?;
    let mut responses = request.receive_signal("Response").ok()?;

    let screenshot = Proxy::new(&connection, PORTAL, PORTAL_PATH, "org.freedesktop.portal.Screenshot").ok()?;
    let options = HashMap::from([("handle_token", Value::from(TOKEN))]);
    let _handle: OwnedObjectPath = screenshot.call("PickColor", &("", options)).ok()?;

    let answer = responses.next()?;
    let (code, results): (u32, HashMap<String, OwnedValue>) = answer.body().deserialize().ok()?;
    if code != RESPONSE_SUCCESS {
        return Some(PickOutcome::Cancelled);
    }

    let (red, green, blue): (f64, f64, f64) = results.get("color")?.try_clone().ok()?.try_into().ok()?;
    Some(PickOutcome::Picked([red, green, blue].map(to_byte)))
}

/// A portal colour component (sRGB, 0 to 1) as 8 bits
fn to_byte(component: f64) -> u8 {
    if component.is_nan() {
        return 0;
    }

    (component.clamp(0.0, 1.0) * 255.0).round() as u8
}
