//! How the system lets a drag hold its pointer

/// How a drag holds the pointer on this platform
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Hold {
    /// The pointer moves, and is put back where the press began after every frame (Windows, X11)
    PutBack,

    /// The system keeps the pointer still and reports raw motion (macOS, Wayland, which cannot move a pointer)
    Locked,

    /// The pointer cannot be held (the browser): drags read it as it moves
    Free,
}

impl Hold {
    /// The way this platform holds a pointer; Wayland is told apart from X11 the way the window system picks it
    pub(crate) fn of_platform() -> Hold {
        if cfg!(target_arch = "wasm32") {
            Hold::Free
        } else if cfg!(target_os = "macos") {
            Hold::Locked
        } else if cfg!(target_os = "windows") || std::env::var_os("WAYLAND_DISPLAY").is_none() {
            Hold::PutBack
        } else {
            Hold::Locked
        }
    }
}
