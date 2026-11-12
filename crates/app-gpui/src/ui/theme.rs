//! Colours and metrics from `src/style.css`, resolved for the current theme and accent.

use crate::logic::appearance::{accent_text, channels, focus_color, mix, normalize_accent, DEFAULT_ACCENT};
use gpui::{rgb, rgba, Hsla};
use translator_core::settings::Settings;

#[cfg(target_os = "macos")]
pub const FONT_FAMILY: &str = ".SystemUIFont";
#[cfg(not(target_os = "macos"))]
pub const FONT_FAMILY: &str = "Segoe UI";

/// Windows at most this wide use the `max-width: 800px` rules.
pub const COMPACT_WIDTH: f32 = 800.0;

fn hex(value: u32) -> Hsla {
    rgb(value).into()
}

fn from_channels(color: [u8; 3]) -> Hsla {
    hex(u32::from(color[0]) << 16 | u32::from(color[1]) << 8 | u32::from(color[2]))
}

/// CSS custom properties of `:root` / `:root[data-theme=amoled]`.
#[derive(Clone, Copy)]
pub struct Palette {
    pub background: Hsla,
    pub sidebar: Hsla,
    pub surface: Hsla,
    pub input: Hsla,
    pub button: Hsla,
    pub hover: Hsla,
    pub text: Hsla,
    pub muted: Hsla,
    pub line: Hsla,
    pub accent: Hsla,
    pub accent_text: Hsla,
    pub focus: Hsla,
    pub panel: Hsla,
    /// `color-mix(in srgb, var(--accent) 12%, var(--surface))`
    pub selected: Hsla,
    /// `color-mix(in srgb, var(--accent) 35%, var(--line))`
    pub accent_border: Hsla,
    /// Live-dot glow: accent at 10% over transparent.
    pub accent_glow: Hsla,
}

impl Palette {
    pub fn from_settings(settings: &Settings) -> Self {
        let accent = normalize_accent(&settings.accent_color).unwrap_or_else(|| DEFAULT_ACCENT.into());
        let accent_rgb = channels(&accent);
        let amoled = settings.theme == "amoled";
        let (surface, line): ([u8; 3], [u8; 3]) = if amoled {
            ([0x08, 0x08, 0x08], [0x2b, 0x2b, 0x2b])
        } else {
            ([0x1d, 0x23, 0x20], [0x30, 0x39, 0x36])
        };
        let glow = from_channels(accent_rgb).opacity(0.10);
        Self {
            background: hex(if amoled { 0x000000 } else { 0x191c1d }),
            sidebar: hex(if amoled { 0x000000 } else { 0x151a18 }),
            surface: from_channels(surface),
            input: hex(if amoled { 0x111111 } else { 0x222926 }),
            button: hex(if amoled { 0x151515 } else { 0x252d28 }),
            hover: hex(if amoled { 0x242424 } else { 0x333f31 }),
            text: hex(if amoled { 0xf1f1f1 } else { 0xedf1eb }),
            muted: hex(if amoled { 0xa0a0a0 } else { 0x929d96 }),
            line: from_channels(line),
            accent: from_channels(accent_rgb),
            accent_text: if accent_text(&accent) == "#ffffff" {
                hex(0xffffff)
            } else {
                hex(0x000000)
            },
            focus: from_channels(focus_color(&accent)),
            panel: hex(if amoled { 0x0c0c0c } else { 0x212725 }),
            selected: from_channels(mix(accent_rgb, 12.0, surface)),
            accent_border: from_channels(mix(accent_rgb, 35.0, line)),
            accent_glow: glow,
        }
    }
}

// Fixed colours that do not depend on the theme.
pub fn notice_border() -> Hsla {
    hex(0x414c39)
}
pub fn error_border() -> Hsla {
    hex(0x755342)
}
pub fn error_text() -> Hsla {
    hex(0xedc2ad)
}
pub fn error_background() -> Hsla {
    hex(0x33271f)
}
/// `.error-text`, `.save-error`, `.check-error`
pub fn error_accent() -> Hsla {
    hex(0xedb498)
}
pub fn issue_background() -> Hsla {
    hex(0x2b271e)
}
pub fn issue_border() -> Hsla {
    hex(0x62523a)
}
pub fn issue_text() -> Hsla {
    hex(0xdfcba9)
}
pub fn idle_dot() -> Hsla {
    hex(0x8b9782)
}
pub fn overlay() -> Hsla {
    rgba(0x0000_0099).into()
}
