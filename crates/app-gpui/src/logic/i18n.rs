//! Interface translation with template matching (port of `src/lib/i18n.ts`).

use regex::Regex;
use std::{collections::HashMap, sync::OnceLock};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Locale {
    #[default]
    Uk,
    En,
}

impl Locale {
    pub fn from_code(code: &str) -> Self {
        if code == "en" {
            Locale::En
        } else {
            Locale::Uk
        }
    }
}

struct Template {
    key: &'static str,
    names: Vec<String>,
    pattern: Regex,
}

struct Catalog {
    messages: HashMap<&'static str, &'static str>,
    templates: Vec<Template>,
}

fn catalog() -> &'static Catalog {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        let raw: &'static str = include_str!("../../assets/messages.json");
        let messages: HashMap<&'static str, &'static str> =
            serde_json::from_str(raw).expect("valid translation catalog");
        let placeholder = Regex::new(r"\{(\w+)\}").expect("placeholder pattern");
        let templates = messages
            .keys()
            .copied()
            .filter(|key| placeholder.is_match(key))
            .map(|key| {
                let mut names = Vec::new();
                let mut expression = String::from("^");
                let mut last = 0;
                for found in placeholder.captures_iter(key) {
                    let whole = found.get(0).expect("match");
                    expression.push_str(&regex::escape(&key[last..whole.start()]));
                    expression.push_str("(.+?)");
                    names.push(found[1].to_string());
                    last = whole.end();
                }
                expression.push_str(&regex::escape(&key[last..]));
                expression.push('$');
                Template {
                    key,
                    names,
                    pattern: Regex::new(&expression).expect("template pattern"),
                }
            })
            .collect();
        Catalog {
            messages,
            templates,
        }
    })
}

fn fill(text: &str, values: &[(String, String)]) -> String {
    static PLACEHOLDER: OnceLock<Regex> = OnceLock::new();
    let pattern = PLACEHOLDER.get_or_init(|| Regex::new(r"\{(\w+)\}").expect("placeholder"));
    pattern
        .replace_all(text, |found: &regex::Captures| {
            values
                .iter()
                .find(|(name, _)| name == &found[1])
                .map(|(_, value)| value.clone())
                .unwrap_or_else(|| found[0].to_string())
        })
        .into_owned()
}

/// Unknown external errors and application names are never mechanically translated.
pub fn translate(text: &str, locale: Locale, params: &[(&str, String)]) -> String {
    let catalog = catalog();
    let mut key = text;
    let mut values: Vec<(String, String)> = params
        .iter()
        .map(|(name, value)| (name.to_string(), value.clone()))
        .collect();
    if locale == Locale::En && !catalog.messages.contains_key(text) && params.is_empty() {
        if let Some(rest) = text.strip_prefix("Error: ") {
            return format!("Error: {}", translate(rest, locale, &[]));
        }
        for template in &catalog.templates {
            if let Some(found) = template.pattern.captures(text) {
                key = template.key;
                values = template
                    .names
                    .iter()
                    .enumerate()
                    .map(|(index, name)| {
                        (name.clone(), translate(&found[index + 1], locale, &[]))
                    })
                    .collect();
                break;
            }
        }
    }
    let result = if locale == Locale::En {
        catalog.messages.get(key).copied().unwrap_or(key)
    } else {
        key
    };
    fill(result, &values)
}

/// Shorthand for a plain translation without parameters.
pub fn t(text: &str, locale: Locale) -> String {
    translate(text, locale, &[])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switches_known_text_without_touching_ukrainian() {
        assert_eq!(t("Готовий до перекладу", Locale::En), "Ready to translate");
        assert_eq!(t("Готовий до перекладу", Locale::Uk), "Готовий до перекладу");
        assert_eq!(t("Мова перекладу", Locale::En), "Translation language");
        assert_eq!(t("Завантажуємо…", Locale::En), "Loading…");
        assert_eq!(t("Chrome", Locale::En), "Chrome");
    }

    #[test]
    fn translates_structured_provider_errors_but_preserves_opaque_errors() {
        assert!(translate(
            "Google тимчасово не може обробити запит (код 503). Це не обов'язково ліміт; повторіть пізніше.",
            Locale::En,
            &[]
        )
        .contains("(code 503)"));
        assert_eq!(
            t(
                "Збій з'єднання з Google: підключення — час очікування вичерпано. Спробуйте запустити знову.",
                Locale::En
            ),
            "Google connection failed: connection — timed out. Try starting again."
        );
        assert_eq!(
            t("WebSocket error: opaque external response", Locale::En),
            "WebSocket error: opaque external response"
        );
        assert_eq!(t("Немає аудіовиходу.", Locale::En), "No audio output.");
    }

    #[test]
    fn explicit_parameters_fill_placeholders() {
        assert_eq!(
            translate("Передано {value} с", Locale::Uk, &[("value", "1.0".into())]),
            "Передано 1.0 с"
        );
        assert_eq!(
            translate("Темп {value}×", Locale::En, &[("value", "1.08".into())]),
            "Speed 1.08×"
        );
    }

    #[test]
    fn autosave_refusals_stay_in_the_catalog() {
        for refusal in crate::logic::autosave::ACTIONABLE_REFUSALS {
            assert!(catalog().messages.contains_key(refusal), "{refusal}");
        }
    }

    fn rust_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                rust_files(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                out.push(path);
            }
        }
    }

    #[test]
    fn shared_catalog_covers_all_native_ukrainian_user_facing_string_literals() {
        let literal = Regex::new(r#""([^"
]*[А-Яа-яІіЇїЄєҐґ][^"
]*)""#).unwrap();
        let mut files = Vec::new();
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        rust_files(&manifest.join("src"), &mut files);
        rust_files(&manifest.join("../translator-core/src"), &mut files);
        assert!(files.len() > 15);
        let mut missing = Vec::new();
        for path in files {
            let text = std::fs::read_to_string(&path).unwrap();
            let production = text.split("#[cfg(test)]").next().unwrap_or("");
            for found in literal.captures_iter(production) {
                let value = &found[1];
                if value.starts_with("Live Translator · ") || value.contains("{}") {
                    continue;
                }
                if !catalog().messages.contains_key(value) {
                    missing.push(format!("{}: {value}", path.display()));
                }
            }
        }
        assert!(missing.is_empty(), "missing from messages.json:
{}", missing.join("
"));
    }
}
