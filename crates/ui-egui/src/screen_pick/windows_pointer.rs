//! Windows: while an eyedropper pick waits for a click outside the app, the system's pointers are the eyedropper over every app
//! and monitor. A keeper process (this program started with `KEEPER_ARG`) swaps them and puts the user's own back when the pick
//! ends, and also when the app goes away for any reason: the pipe it waits on closes even when the app is killed, so the system
//! pointer is never left changed. The keeper reads its message queue first thing: Windows shows its "app starting" pointer
//! (an arrow with a busy circle) over every app from a program's start until the program first waits for input, up to seven
//! seconds, and a keeper that never asks for input kept that pointer over the eyedropper

use std::io::{BufRead, BufReader, Read, Write};
use std::os::windows::process::CommandExt;
use std::process::{Child, ChildStdin, Command, Stdio};

use winsafe::guard::DestroyCursorGuard;
use winsafe::prelude::Handle;
use winsafe::{HINSTANCE, IdOcrStr, MSG, SIZE, co};

use super::CursorImage;

/// The argument that starts this program as the pointer keeper
pub const KEEPER_ARG: &str = "--eyedropper-pointer";

/// The keeper's line once the eyedropper shows
const SHOWN: &str = "shown";

/// A process started without a console window
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

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

/// The eyedropper as the system pointer, until this is dropped
pub(super) struct SystemPointer {
    keeper: Child,
    pipe: Option<ChildStdin>,
}

impl SystemPointer {
    /// Starts the keeper and waits until the eyedropper shows; `None` leaves the system pointer as it is
    pub(super) fn show() -> Option<Self> {
        let program = std::env::current_exe().ok()?;
        let mut keeper = Command::new(program)
            .arg(KEEPER_ARG)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .ok()?;
        let pipe = keeper.stdin.take();
        let trace = super::windows_trace::Trace::start(keeper.id());
        let said = keeper.stdout.take().and_then(|said| BufReader::new(said).lines().next()).and_then(Result::ok);
        if let Some(trace) = trace {
            trace.shown(said.is_some());
        }
        let pointer = SystemPointer { keeper, pipe };
        (said.as_deref() == Some(SHOWN)).then_some(pointer)
    }
}

impl Drop for SystemPointer {
    fn drop(&mut self) {
        // The closed pipe tells the keeper to put the user's pointers back
        self.pipe = None;
        let _ = self.keeper.wait();
    }
}

/// The keeper process's work and exit code: the eyedropper as the system pointer until the app's pipe closes
pub fn keep_pointer() -> i32 {
    // Before anything else: ends the system's "app starting" pointer for this program
    wait_for_input_once();
    let size = u32::try_from(winsafe::GetSystemMetrics(co::SM::CXCURSOR)).unwrap_or_default();
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
    let mut said = std::io::stdout();
    let _ = writeln!(said, "{SHOWN}").and_then(|()| said.flush());

    // Ends when the app closes the pipe: the pick ended, or the app is gone
    let _ = std::io::stdin().read_to_end(&mut Vec::new());
    for (pointer, mut original) in originals {
        if original.SetSystemCursor(pointer).is_ok() {
            let _ = original.leak();
        }
    }
    0
}

/// Looks at this thread's (empty) message queue once, which tells Windows the program started and now waits for input
fn wait_for_input_once() {
    let _ = winsafe::PeekMessage(&mut MSG::default(), None, 0, 0, co::PM::NOREMOVE);
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
