//! Windows: a low-level mouse hook on its own thread takes a left click outside the app's windows (the click never reaches the
//! window under it) and reads the one pixel under it from the desktop through GDI; clicks on the app's own windows pass on. The
//! system pointer is the eyedropper with its colour disc until the wait ends

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use winsafe::prelude::Handle;
use winsafe::{HHOOK, HINSTANCE, HWND, MSG, POINT, co};

use super::pick_shared::over_windows;
use super::windows_swatch::LiveSwatch;
use super::{PickOutcome, PickShared};

/// How often the hook thread wakes to see whether the pick ended in the app, in milliseconds
const STOP_CHECK_MS: u32 = 50;

/// The running pick the hook procedure reports to (a hook procedure has no context of its own)
struct HookState {
    /// Which pick this is: a pick that ends after the next one started leaves the next one's state alone
    generation: u64,
    shared: Arc<PickShared>,
    outcomes: Sender<PickOutcome>,
    ctx: egui::Context,
    /// The press it took: its release is taken too, so the window under it sees no half click
    pressed: bool,
    done: bool,
}

static HOOK: Mutex<Option<HookState>> = Mutex::new(None);
static GENERATION: AtomicU64 = AtomicU64::new(0);

/// Waits on this thread for a left click outside the app until it lands or `shared` stops the pick (`dpi` sizes the pointer)
pub(super) fn run(shared: &Arc<PickShared>, outcomes: Sender<PickOutcome>, ctx: egui::Context, dpi: u32) {
    let unavailable = outcomes.clone();
    let generation = GENERATION.fetch_add(1, Ordering::Relaxed);
    if let Ok(mut hook) = HOOK.lock() {
        *hook = Some(HookState { generation, shared: Arc::clone(shared), outcomes, ctx: ctx.clone(), pressed: false, done: false });
    }

    // Before the hook: while it is in place this thread must keep reading messages, or every mouse move on the desktop stalls
    let pointer = super::windows_pointer::SystemPointer::show(dpi);
    let Ok(module) = HINSTANCE::GetModuleHandle(None) else { return finish(&unavailable, &ctx) };
    let Ok(mut hook) = HHOOK::SetWindowsHookEx(co::WH::MOUSE_LL, mouse_hook, Some(&module), None) else {
        return finish(&unavailable, &ctx);
    };

    // A thread timer wakes GetMessage so the loop sees a stop and the disc follows; the hook procedure runs inside GetMessage
    let timer = HWND::NULL.SetTimer(1, STOP_CHECK_MS, None);
    let mut message = MSG::default();

    // The disc follows on a thread of its own: this one must answer the hook at once, or every mouse move stalls
    let disc = pointer.as_ref().map(|pointer| (pointer.shown_event.clone(), pointer.cell));
    std::thread::scope(|scope| {
        if let Some((shown_event, cell)) = &disc {
            let follow = std::thread::Builder::new().name("eyedropper disc".into());
            let _ = follow.spawn_scoped(scope, || LiveSwatch::follow(shown_event, *cell, shared, generation));
        }
        while waiting(shared, generation) && matches!(winsafe::GetMessage(&mut message, None, 0, 0), Ok(true)) {}
    });
    if let Ok(timer) = timer {
        let _ = HWND::NULL.KillTimer(timer);
    }
    let _ = hook.UnhookWindowsHookEx();
    if let Ok(mut hook) = HOOK.lock()
        && hook.as_ref().is_some_and(|state| state.generation == generation)
    {
        *hook = None;
    }
}

fn finish(outcomes: &Sender<PickOutcome>, ctx: &egui::Context) {
    let _ = outcomes.send(PickOutcome::Unavailable);
    ctx.request_repaint();
}

/// Whether pick `generation` still waits: for its click until the app ends the pick, then for the release of the press it took
pub(super) fn waiting(shared: &PickShared, generation: u64) -> bool {
    let Ok(hook) = HOOK.lock() else { return false };
    let Some(state) = hook.as_ref().filter(|state| state.generation == generation) else { return false };

    state.pressed || (!state.done && !shared.stopped())
}

extern "system" fn mouse_hook(code: i32, wparam: usize, lparam: isize) -> isize {
    // A negative code must go straight to the next hook
    if code >= 0 && take(wparam) {
        return 1;
    }

    winsafe::CallNextHookEx(code, wparam, lparam)
}

/// Whether the mouse message `message` is the pick's own: a left press outside the app (read, then taken) or its release
fn take(message: usize) -> bool {
    let Ok(mut hook) = HOOK.lock() else { return false };
    let Some(state) = hook.as_mut() else { return false };

    let message = u32::try_from(message).unwrap_or_default();
    if message == co::WM::LBUTTONUP.raw() && state.pressed {
        state.pressed = false;
        return true;
    }

    if message != co::WM::LBUTTONDOWN.raw() || state.done {
        return false;
    }

    let Ok(point) = winsafe::GetCursorPos() else { return false };
    if over_own_window(point) || over_windows(&state.shared.windows(), point.x, point.y) {
        return false;
    }

    let outcome = screen_pixel(point).map_or(PickOutcome::Unavailable, PickOutcome::Picked);
    let _ = state.outcomes.send(outcome);
    state.ctx.request_repaint();
    state.pressed = true;
    state.done = true;
    true
}

/// Whether the window under `point` belongs to this process (the app's main window, a floating panel, a dialog)
pub(super) fn over_own_window(point: POINT) -> bool {
    HWND::WindowFromPoint(point).is_some_and(|window| window.GetWindowThreadProcessId().1 == winsafe::GetCurrentProcessId())
}

/// The colour of the one desktop pixel at `point` (physical pixels across every monitor: the app is per-monitor DPI aware)
pub(super) fn screen_pixel(point: POINT) -> Option<[u8; 3]> {
    let monitor = xcap::Monitor::from_point(point.x, point.y).ok()?;
    let x = u32::try_from(point.x.checked_sub(monitor.x().ok()?)?).ok()?;
    let y = u32::try_from(point.y.checked_sub(monitor.y().ok()?)?).ok()?;
    let pixel = monitor.capture_region(x, y, 1, 1).ok()?;
    let [red, green, blue, _] = pixel.get_pixel_checked(0, 0)?.0;
    Some([red, green, blue])
}
