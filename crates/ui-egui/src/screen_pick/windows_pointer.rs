//! Windows: while an eyedropper pick waits for a click outside the app, the system's pointers are the eyedropper over every app
//! and monitor. A keeper process (this program started with `KEEPER_ARG`) swaps them and puts the user's own back when the pick
//! ends, and also when the app goes away for any reason: it watches the app's process, so the system pointer is never left
//! changed. The keeper is started with the system's start-up feedback off: Windows otherwise shows its "app starting" pointer
//! (an arrow with a busy circle) over every app for up to seven seconds after a program starts, until the program first calls
//! `GetMessage`, which a keeper never does, and that pointer showed instead of the eyedropper

use std::sync::atomic::{AtomicU64, Ordering};

use winsafe::guard::{CloseHandleGuard, CloseHandlePiGuard, DestroyCursorGuard};
use winsafe::prelude::Handle;
use winsafe::{HEVENT, HINSTANCE, HPROCESS, IdOcrStr, SIZE, STARTUPINFO, co};

use super::CursorImage;
use super::keeper_args::KeeperArgs;

/// How long the app waits for the keeper to put the user's pointers back, in milliseconds
const RESTORE_WAIT_MS: u32 = 2_000;
/// How often the keeper looks whether the app still runs, in milliseconds
const WATCH_MS: u32 = 50;

/// The system pointers other apps show; the animated busy pointers stay as they are (a copy would keep one frame)
const POINTERS: [co::OCR; 11] = [
    co::OCR::NORMAL,
    co::OCR::IBEAM,
    co::OCR::CROSS,
    co::OCR::UP,
    co::OCR::SIZENWSE,
    co::OCR::SIZENESW,
    co::OCR::SIZEWE,
    co::OCR::SIZENS,
    co::OCR::SIZEALL,
    co::OCR::NO,
    co::OCR::HAND,
];

/// Each pick's own stop event
static NEXT_PICK: AtomicU64 = AtomicU64::new(0);

/// The eyedropper as the system pointer, until this is dropped
pub(super) struct SystemPointer {
    keeper: CloseHandlePiGuard,
    stop: CloseHandleGuard<HEVENT>,
}

impl SystemPointer {
    /// Starts the keeper for a window at `dpi` dots per inch; `None` leaves the system pointer as it is
    pub(super) fn show(dpi: u32) -> Option<Self> {
        let program = std::env::current_exe().ok()?;
        let parent = std::process::id();
        let stop_name = format!("Local\\texturecraft-eyedropper-{parent}-{}", NEXT_PICK.fetch_add(1, Ordering::Relaxed));
        let stop = HEVENT::CreateEvent(None, true, false, Some(&stop_name)).ok()?;
        let command_line = KeeperArgs { parent, stop: stop_name, dpi }.command_line(program.to_str()?);
        let mut startup = STARTUPINFO::default();
        // No "app starting" pointer for the keeper: it shows no window and never asks for input
        startup.dwFlags = co::STARTF::FORCEOFFFEEDBACK;
        let keeper = winsafe::CreateProcess(None, Some(&command_line), None, None, false, co::CREATE::NO_WINDOW, &[], None, &mut startup);
        Some(SystemPointer { keeper: keeper.ok()?, stop })
    }
}

impl Drop for SystemPointer {
    fn drop(&mut self) {
        // Waited for, so the next pick's keeper never takes the eyedropper for the user's own pointer
        let _ = self.stop.SetEvent();
        let _ = self.keeper.hProcess.WaitForSingleObject(Some(RESTORE_WAIT_MS));
    }
}

/// The keeper process's work and exit code: the eyedropper as the system pointer until the pick ends or the app is gone
pub fn keep_pointer() -> i32 {
    let args: Vec<String> = std::env::args().skip(2).collect();
    let Some(keeper) = KeeperArgs::parse(&args) else { return 2 };
    let Ok(app) = HPROCESS::OpenProcess(co::PROCESS::SYNCHRONIZE, false, keeper.parent) else { return 1 };
    let Ok(stop) = HEVENT::NULL.OpenEvent(co::EVENT_RIGHTS::SYNCHRONIZE, false, &keeper.stop) else { return 1 };
    if ended(stop.WaitForSingleObject(Some(0))) {
        return 0;
    }
    let size = keeper.pointer_px(winsafe::GetSystemMetricsForDpi(co::SM::CXCURSOR, keeper.dpi).ok());
    let Some(eyedropper) = load(&CursorImage::eyedropper(size)) else { return 1 };
    let originals: Vec<_> = POINTERS.iter().filter_map(|&pointer| Some((pointer, current(pointer)?))).collect();
    for (pointer, _) in &originals {
        if let Ok(mut copy) = eyedropper.CopyCursor()
            && copy.SetSystemCursor(*pointer).is_ok()
        {
            // The system owns the copy now
            let _ = copy.leak();
        }
    }

    // Ends when the app sets the stop event (the pick ended) or its process ends (closed, killed, crashed)
    while !ended(stop.WaitForSingleObject(Some(0))) && !ended(app.WaitForSingleObject(Some(WATCH_MS))) {}
    for (pointer, mut original) in originals {
        if original.SetSystemCursor(pointer).is_ok() {
            let _ = original.leak();
        }
    }
    0
}

/// Whether a wait says its object is signalled (or can't be waited on, which ends the keeper just as well)
fn ended(wait: winsafe::SysResult<co::WAIT>) -> bool {
    !matches!(wait, Ok(co::WAIT::TIMEOUT))
}

/// A copy of the system pointer `pointer` as it is now
fn current(pointer: co::OCR) -> Option<DestroyCursorGuard> {
    let mut shared = HINSTANCE::NULL.LoadImageCursor(IdOcrStr::Ocr(pointer), SIZE::default(), co::LR::SHARED | co::LR::DEFAULTSIZE).ok()?;
    let copy = shared.CopyCursor().ok();
    // A shared pointer belongs to the system and is never destroyed
    let _ = shared.leak();
    copy
}

/// `image` as a Windows pointer (loaded from a `.cur` file, the one safe way to make a coloured pointer)
fn load(image: &CursorImage) -> Option<DestroyCursorGuard> {
    let file = std::env::temp_dir().join(format!("texturecraft-eyedropper-{}.cur", std::process::id()));
    std::fs::write(&file, image.cur_file()).ok()?;
    let loaded = HINSTANCE::NULL.LoadImageCursor(IdOcrStr::from_str(&file.to_string_lossy()), SIZE::default(), co::LR::LOADFROMFILE);
    let _ = std::fs::remove_file(&file);
    loaded.ok()
}
