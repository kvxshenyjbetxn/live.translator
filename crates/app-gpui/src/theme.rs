//! Colors and sizes taken from `src/style.css` and `src/App.svelte`.

use gpui::{rgb, Hsla, Rgba};

pub fn background() -> Rgba {
    rgb(0x191c1d)
}
pub fn sidebar() -> Rgba {
    rgb(0x151a18)
}
pub fn surface() -> Rgba {
    rgb(0x1d2320)
}
pub fn text() -> Rgba {
    rgb(0xedf1eb)
}
pub fn muted() -> Rgba {
    rgb(0x929d96)
}
pub fn line() -> Rgba {
    rgb(0x303936)
}
pub fn accent() -> Rgba {
    rgb(0xc5e89d)
}
pub fn eyebrow() -> Rgba {
    rgb(0x94a48c)
}
pub fn privacy_note() -> Rgba {
    rgb(0x8f9d8c)
}
pub fn idle_dot() -> Rgba {
    rgb(0x8b9782)
}
/// `color-mix(in srgb, var(--accent) 10%, transparent)` used for the live-dot glow.
pub fn accent_glow() -> Hsla {
    let c = accent();
    Rgba { a: 0.10, ..c }.into()
}

pub const FONT_FAMILY: &str = "Segoe UI";

// `.main-column main` padding and max-width.
pub const MAIN_PADDING_TOP: f32 = 30.0;
pub const MAIN_PADDING_X: f32 = 34.0;
pub const MAIN_PADDING_BOTTOM: f32 = 24.0;
pub const MAIN_MAX_WIDTH: f32 = 830.0;

// `.pane-body` height is `calc(100vh - 300px)` with `min-height: 280px`.
pub const PANE_BODY_VIEWPORT_OFFSET: f32 = 300.0;
pub const PANE_BODY_MIN_HEIGHT: f32 = 280.0;
// `nearBottom` threshold from Transcripts.svelte.
pub const NEAR_BOTTOM_PX: f32 = 32.0;
