//! Windows: a timing line per eyedropper pick for checking the pointer keeper on a real desktop

use std::io::Write;

use winsafe::{HPROCESS, co};

/// Where `TEXTURECRAFT_EYEDROPPER_TRACE` names a file, one line per pick: when the busy pointer ended, when the eyedropper showed
pub(super) struct Trace {
    file: std::path::PathBuf,
    started: std::time::Instant,
    idle_ms: Option<u128>,
}

impl Trace {
    pub(super) fn start(keeper: u32) -> Option<Self> {
        let file = std::env::var_os("TEXTURECRAFT_EYEDROPPER_TRACE")?.into();
        let started = std::time::Instant::now();
        let idle_ms = input_idle(keeper, 10_000).then(|| started.elapsed().as_millis());
        Some(Trace { file, started, idle_ms })
    }

    pub(super) fn shown(self, shown: bool) {
        let idle = self.idle_ms.map_or_else(|| "never".to_owned(), |ms| format!("{ms} ms"));
        let shown = if shown { format!("{} ms", self.started.elapsed().as_millis()) } else { "never".to_owned() };
        let line = format!("eyedropper keeper: app-starting pointer ended {idle}, eyedropper shown {shown}\n");
        if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(&self.file) {
            let _ = file.write_all(line.as_bytes());
        }
    }
}

/// Whether process `id` waits for input (the point where Windows drops its "app starting" pointer) within `timeout_ms`
fn input_idle(id: u32, timeout_ms: u32) -> bool {
    HPROCESS::OpenProcess(co::PROCESS::SYNCHRONIZE | co::PROCESS::QUERY_LIMITED_INFORMATION, false, id)
        .is_ok_and(|process| matches!(process.WaitForInputIdle(timeout_ms), Ok(winsafe::SuccessTimeout::Success)))
}
