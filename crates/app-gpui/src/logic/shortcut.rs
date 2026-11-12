//! Shortcut recorder key mapping (port of `src/lib/shortcut.ts`). Used by the Settings page.
#![allow(dead_code)]

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
}
