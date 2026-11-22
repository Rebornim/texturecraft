//! What the filter tests share: their sessions, adding, setting and refusing a filter, and reading the composite

use texturecraft_engine::Session;
use texturecraft_engine::core::Channel;
use texturecraft_engine::geom::glam::Vec2;
use texturecraft_engine::geom::primitives;

/// The texture set side the filter tests paint on
pub(crate) const SIZE: u32 = 128;

/// The viewport's side in pixels when a test reads the texel under its center
const VIEWPORT_SIDE: f32 = 400.0;

/// A sphere on a `SIZE` texture set, rendered once
pub(crate) fn session() -> Session {
    let mut s = sized_session(SIZE);
    s.update();
    s
}

/// A sphere on a texture set of side `size`, not yet rendered
pub(crate) fn sized_session(size: u32) -> Session {
    Session::new_project("t", primitives::sphere(1.0, 48, 24), "builtin:sphere", None, size).unwrap()
}

/// A session whose camera looks straight at the sphere, so the viewport center reads one texel (see [`center`])
pub(crate) fn facing_session() -> Session {
    let mut s = session();
    s.viewport = Vec2::splat(VIEWPORT_SIDE);
    s.camera.yaw = 0.0;
    s.camera.pitch = 0.0;
    s.update();
    s
}

/// The UV under the viewport center
pub(crate) fn center_uv(s: &mut Session) -> Vec2 {
    s.update();
    s.pick(Vec2::splat(VIEWPORT_SIDE / 2.0), s.viewport).unwrap().uv
}

/// Composited value of `channel` under the viewport center
pub(crate) fn center(s: &mut Session, channel: Channel) -> [f32; 3] {
    let uv = center_uv(s);
    s.caches[0].composite.value(channel, (uv.x * SIZE as f32) as u32, (uv.y * SIZE as f32) as u32)
}
