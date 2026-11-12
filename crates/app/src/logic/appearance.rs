//! Accent normalisation and readable colours (port of `src/lib/appearance.ts`).

pub const DEFAULT_ACCENT: &str = "#c5e89d";

/// Accepts only opaque `#rrggbb`; returns it lower-cased.
pub fn normalize_accent(value: &str) -> Option<String> {
    let hex = value.trim();
    let bytes = hex.as_bytes();
    (bytes.len() == 7 && bytes[0] == b'#' && bytes[1..].iter().all(u8::is_ascii_hexdigit))
        .then(|| hex.to_ascii_lowercase())
}

/// Parses a normalised `#rrggbb` into 0..=255 channels.
pub fn channels(hex: &str) -> [u8; 3] {
    let channel = |index: usize| u8::from_str_radix(&hex[index..index + 2], 16).unwrap_or(0);
    [channel(1), channel(3), channel(5)]
}

/// `#000000` or `#ffffff`, whichever has the better contrast on the accent.
pub fn accent_text(hex: &str) -> &'static str {
    let [r, g, b] = channels(hex).map(|c| {
        let c = f64::from(c) / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    });
    let luminance = r * 0.2126 + g * 0.7152 + b * 0.0722;
    if (luminance + 0.05) / 0.05 >= 1.05 / (luminance + 0.05) {
        "#000000"
    } else {
        "#ffffff"
    }
}

/// `color-mix(in srgb, a <percent>%, b)` on opaque colours.
pub fn mix(a: [u8; 3], percent: f32, b: [u8; 3]) -> [u8; 3] {
    let weight = percent / 100.0;
    let blend =
        |x: u8, y: u8| (f32::from(x) * weight + f32::from(y) * (1.0 - weight)).round() as u8;
    [blend(a[0], b[0]), blend(a[1], b[1]), blend(a[2], b[2])]
}

/// The `--focus` colour: the accent itself, lightened when a white label is needed on it.
pub fn focus_color(accent: &str) -> [u8; 3] {
    let rgb = channels(accent);
    if accent_text(accent) == "#ffffff" {
        mix(rgb, 45.0, [255, 255, 255])
    } else {
        rgb
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_validation_accepts_only_opaque_full_rgb_colors() {
        assert_eq!(normalize_accent(" #12ABef ").as_deref(), Some("#12abef"));
        for value in ["red", "#fff", "#12345678", "#gg0000", "", "#12345é"] {
            assert_eq!(normalize_accent(value), None, "{value}");
        }
    }

    #[test]
    fn black_white_and_middle_accents_have_readable_button_text() {
        assert_eq!(accent_text("#000000"), "#ffffff");
        assert_eq!(accent_text("#ffffff"), "#000000");
        assert_eq!(accent_text("#c5e89d"), "#000000");
    }

    #[test]
    fn black_accent_gets_a_visible_focus_colour() {
        assert_eq!(focus_color("#000000"), [140, 140, 140]);
        assert_eq!(focus_color("#c5e89d"), [0xc5, 0xe8, 0x9d]);
    }

    #[test]
    fn srgb_mix_blends_channels() {
        assert_eq!(mix([255, 255, 255], 50.0, [0, 0, 0]), [128, 128, 128]);
    }
}
