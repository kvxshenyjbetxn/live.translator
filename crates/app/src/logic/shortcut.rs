//! Shortcut recorder key mapping (port of `src/lib/shortcut.ts`). Used by the Settings page.

/// DOM-style physical key codes the recorder accepts.
fn supported_code(code: &str) -> bool {
    const NAMED: &[&str] = &[
        "Space", "Enter", "Tab", "Backspace", "Delete", "Insert", "Home", "End", "PageUp",
        "PageDown", "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "Minus", "Equal",
        "BracketLeft", "BracketRight", "Backslash", "Semicolon", "Quote", "Comma", "Period",
        "Slash", "Backquote",
    ];
    if NAMED.contains(&code) {
        return true;
    }
    if let Some(letter) = code.strip_prefix("Key") {
        return letter.len() == 1 && letter.as_bytes()[0].is_ascii_uppercase();
    }
    if let Some(digit) = code.strip_prefix("Digit") {
        return digit.len() == 1 && digit.as_bytes()[0].is_ascii_digit();
    }
    if let Some(number) = code.strip_prefix('F') {
        return !number.starts_with('0') && number.parse::<u8>().is_ok_and(|n| (1..=24).contains(&n));
    }
    false
}

pub struct Modifiers {
    pub ctrl: bool,
    pub meta: bool,
    pub alt: bool,
    pub shift: bool,
}

/// The stored shortcut string for a key press, or `None` when it cannot be a global shortcut.
pub fn shortcut_from_key(code: &str, modifiers: &Modifiers, repeat: bool) -> Option<String> {
    if repeat || !(modifiers.ctrl || modifiers.meta || modifiers.alt || modifiers.shift) {
        return None;
    }
    if !supported_code(code) {
        return None;
    }
    let mut parts = Vec::new();
    if modifiers.meta {
        parts.push("Super");
    }
    if modifiers.ctrl {
        parts.push("Control");
    }
    if modifiers.alt {
        parts.push("Alt");
    }
    if modifiers.shift {
        parts.push("Shift");
    }
    parts.push(code);
    Some(parts.join("+"))
}

/// Maps a GPUI key name (lower-case logical key) to the DOM-style physical code the stored
/// shortcut format uses; `None` for keys the recorder does not know.
pub fn code_from_key(key: &str) -> Option<String> {
    let named = match key {
        "space" => "Space",
        "enter" => "Enter",
        "tab" => "Tab",
        "backspace" => "Backspace",
        "delete" => "Delete",
        "insert" => "Insert",
        "home" => "Home",
        "end" => "End",
        "pageup" => "PageUp",
        "pagedown" => "PageDown",
        "up" => "ArrowUp",
        "down" => "ArrowDown",
        "left" => "ArrowLeft",
        "right" => "ArrowRight",
        "-" => "Minus",
        "=" => "Equal",
        "[" => "BracketLeft",
        "]" => "BracketRight",
        "\\" => "Backslash",
        ";" => "Semicolon",
        "'" => "Quote",
        "," => "Comma",
        "." => "Period",
        "/" => "Slash",
        "`" => "Backquote",
        _ => "",
    };
    if !named.is_empty() {
        return Some(named.to_string());
    }
    let mut chars = key.chars();
    if let (Some(only), None) = (chars.next(), chars.next()) {
        if only.is_ascii_lowercase() {
            return Some(format!("Key{}", only.to_ascii_uppercase()));
        }
        if only.is_ascii_digit() {
            return Some(format!("Digit{only}"));
        }
        return None;
    }
    let number = key.strip_prefix('f')?;
    (!number.starts_with('0') && number.parse::<u8>().is_ok_and(|n| (1..=24).contains(&n)))
        .then(|| key.to_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mods(ctrl: bool, shift: bool, alt: bool, meta: bool) -> Modifiers {
        Modifiers { ctrl, meta, alt, shift }
    }

    #[test]
    fn recorder_uses_physical_keys_and_portable_modifier_spelling() {
        assert_eq!(
            shortcut_from_key("KeyT", &mods(true, true, false, false), false).as_deref(),
            Some("Control+Shift+KeyT")
        );
        assert_eq!(
            shortcut_from_key("Space", &mods(false, false, true, true), false).as_deref(),
            Some("Super+Alt+Space")
        );
    }

    #[test]
    fn plain_modifier_only_unsupported_and_repeated_keys_cannot_become_shortcuts() {
        let none = mods(false, false, false, false);
        let ctrl = mods(true, false, false, false);
        assert_eq!(shortcut_from_key("KeyT", &none, false), None);
        assert_eq!(shortcut_from_key("ControlLeft", &ctrl, false), None);
        assert_eq!(shortcut_from_key("Unidentified", &ctrl, false), None);
        assert_eq!(shortcut_from_key("KeyT", &ctrl, true), None);
        assert!(shortcut_from_key("F24", &ctrl, false).is_some());
        assert!(shortcut_from_key("F25", &ctrl, false).is_none());
    }

    #[test]
    fn gpui_key_names_map_to_stored_codes() {
        let code = |key: &str| code_from_key(key);
        assert_eq!(code("t").as_deref(), Some("KeyT"));
        assert_eq!(code("7").as_deref(), Some("Digit7"));
        assert_eq!(code("f12").as_deref(), Some("F12"));
        assert_eq!(code("up").as_deref(), Some("ArrowUp"));
        assert_eq!(code("`").as_deref(), Some("Backquote"));
        assert_eq!(code("\\").as_deref(), Some("Backslash"));
        assert_eq!(code("f25"), None);
        assert_eq!(code("escape"), None);
        assert_eq!(code("shift"), None);
        assert_eq!(code("é"), None);
        // Every mapped code must be accepted by the recorder's own validation.
        let ctrl = mods(true, false, false, false);
        assert!(shortcut_from_key(&code("f24").unwrap(), &ctrl, false).is_some());
    }
}
