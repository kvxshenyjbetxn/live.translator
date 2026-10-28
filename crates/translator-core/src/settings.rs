use serde::{Deserialize, Serialize};
use std::{collections::HashSet, fs, io::Write, path::Path};

#[derive(Clone, Serialize)]
pub struct Language {
    pub code: &'static str,
    pub name: &'static str,
}
// https://ai.google.dev/gemini-api/docs/live-api/live-translate#supported-languages
pub const LANGUAGES: &[Language] = &[
    Language {
        code: "uk",
        name: "Українська",
    },
    Language {
        code: "az",
        name: "Азербайджанська",
    },
    Language {
        code: "ak",
        name: "Акан",
    },
    Language {
        code: "sq",
        name: "Албанська",
    },
    Language {
        code: "am",
        name: "Амхарська",
    },
    Language {
        code: "en",
        name: "Англійська",
    },
    Language {
        code: "ar",
        name: "Арабська",
    },
    Language {
        code: "af",
        name: "Африкаанс",
    },
    Language {
        code: "eu",
        name: "Баскська",
    },
    Language {
        code: "bn",
        name: "Бенгальська",
    },
    Language {
        code: "be",
        name: "Білоруська",
    },
    Language {
        code: "my",
        name: "Бірманська (М'янма)",
    },
    Language {
        code: "bg",
        name: "Болгарська",
    },
    Language {
        code: "vi",
        name: "В'єтнамська",
    },
    Language {
        code: "hy",
        name: "Вірменська",
    },
    Language {
        code: "gl",
        name: "Галісійська",
    },
    Language {
        code: "hi",
        name: "Гінді",
    },
    Language {
        code: "el",
        name: "Грецька",
    },
    Language {
        code: "ka",
        name: "Грузинська",
    },
    Language {
        code: "gu",
        name: "Гуджараті",
    },
    Language {
        code: "da",
        name: "Данська",
    },
    Language {
        code: "et",
        name: "Естонська",
    },
    Language {
        code: "zu",
        name: "Зулуська",
    },
    Language {
        code: "he",
        name: "Іврит",
    },
    Language {
        code: "id",
        name: "Індонезійська",
    },
    Language {
        code: "is",
        name: "Ісландська",
    },
    Language {
        code: "es",
        name: "Іспанська",
    },
    Language {
        code: "it",
        name: "Італійська",
    },
    Language {
        code: "kk",
        name: "Казахська",
    },
    Language {
        code: "kn",
        name: "Каннада",
    },
    Language {
        code: "ca",
        name: "Каталанська",
    },
    Language {
        code: "zh-Hans",
        name: "Китайська (спрощена)",
    },
    Language {
        code: "zh-Hant",
        name: "Китайська (традиційна)",
    },
    Language {
        code: "rw",
        name: "Кіньяруанда",
    },
    Language {
        code: "ko",
        name: "Корейська",
    },
    Language {
        code: "km",
        name: "Кхмерська",
    },
    Language {
        code: "lo",
        name: "Лаоська",
    },
    Language {
        code: "lv",
        name: "Латиська",
    },
    Language {
        code: "lt",
        name: "Литовська",
    },
    Language {
        code: "mk",
        name: "Македонська",
    },
    Language {
        code: "ms",
        name: "Малайська",
    },
    Language {
        code: "ml",
        name: "Малаялам",
    },
    Language {
        code: "mr",
        name: "Маратхі",
    },
    Language {
        code: "mn",
        name: "Монгольська",
    },
    Language {
        code: "ne",
        name: "Непальська",
    },
    Language {
        code: "nl",
        name: "Нідерландська",
    },
    Language {
        code: "de",
        name: "Німецька",
    },
    Language {
        code: "no",
        name: "Норвезька",
    },
    Language {
        code: "pa",
        name: "Панджабі",
    },
    Language {
        code: "fa",
        name: "Перська",
    },
    Language {
        code: "pl",
        name: "Польська",
    },
    Language {
        code: "pt-BR",
        name: "Португальська (Бразилія)",
    },
    Language {
        code: "pt-PT",
        name: "Португальська (Португалія)",
    },
    Language {
        code: "ru",
        name: "Російська",
    },
    Language {
        code: "ro",
        name: "Румунська",
    },
    Language {
        code: "sr",
        name: "Сербська",
    },
    Language {
        code: "si",
        name: "Сингальська",
    },
    Language {
        code: "sd",
        name: "Сіндхі",
    },
    Language {
        code: "sk",
        name: "Словацька",
    },
    Language {
        code: "sl",
        name: "Словенська",
    },
    Language {
        code: "sw",
        name: "Суахілі",
    },
    Language {
        code: "su",
        name: "Сунданська",
    },
    Language {
        code: "th",
        name: "Тайська",
    },
    Language {
        code: "ta",
        name: "Тамільська",
    },
    Language {
        code: "te",
        name: "Телугу",
    },
    Language {
        code: "tr",
        name: "Турецька",
    },
    Language {
        code: "hu",
        name: "Угорська",
    },
    Language {
        code: "uz",
        name: "Узбецька",
    },
    Language {
        code: "ur",
        name: "Урду",
    },
    Language {
        code: "fil",
        name: "Філіппінська",
    },
    Language {
        code: "fi",
        name: "Фінська",
    },
    Language {
        code: "fr",
        name: "Французька",
    },
    Language {
        code: "ha",
        name: "Хауса",
    },
    Language {
        code: "hr",
        name: "Хорватська",
    },
    Language {
        code: "cs",
        name: "Чеська",
    },
    Language {
        code: "sv",
        name: "Шведська",
    },
    Language {
        code: "jv",
        name: "Яванська",
    },
    Language {
        code: "ja",
        name: "Японська",
    },
];

#[derive(Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum ShortcutMode {
    #[default]
    ActiveApplication,
    LastCaptured,
}

pub const MAX_FAVORITE_SOURCES: usize = 32;
pub const MAX_HIDDEN_SOURCES: usize = 32;
pub const MAX_SOURCE_ID_LEN: usize = 512;
pub const MAX_SOURCE_NAME_LEN: usize = 128;
/// Limit for the pretty JSON written to disk; loading keeps the wider 64 KiB guard above.
pub const MAX_SERIALIZED_BYTES: usize = 60 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceRef {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub schema_version: u32,
    pub api_key: String,
    pub target_language: String,
    pub theme: String,
    pub accent_color: String,
    pub interface_language: String,
    pub original_volume: f32,
    pub translation_volume: f32,
    pub selected_source_id: Option<String>,
    pub capture_shortcut: Option<String>,
    pub shortcut_mode: ShortcutMode,
    pub last_captured_source_id: Option<String>,
    pub favorite_sources: Vec<SourceRef>,
    pub hidden_sources: Vec<SourceRef>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: 1,
            api_key: String::new(),
            target_language: "uk".into(),
            theme: "default".into(),
            accent_color: "#c5e89d".into(),
            interface_language: "uk".into(),
            original_volume: 0.15,
            translation_volume: 0.7,
            selected_source_id: None,
            capture_shortcut: None,
            shortcut_mode: ShortcutMode::ActiveApplication,
            last_captured_source_id: None,
            favorite_sources: Vec::new(),
            hidden_sources: Vec::new(),
        }
    }
}
impl Settings {
    /// Rejects settings that cannot be saved, including ones whose serialized form exceeds
    /// [`MAX_SERIALIZED_BYTES`]. Loading uses [`Settings::validate_structure`] instead so files
    /// written before this guard stay readable under the existing 64 KiB load limit.
    pub fn validate(&self) -> Result<(), SettingsError> {
        self.validate_structure()?;
        let bytes = serde_json::to_vec_pretty(self).map_err(|_| SettingsError::Validation)?;
        if bytes.len() > MAX_SERIALIZED_BYTES {
            return Err(SettingsError::TooLarge);
        }
        Ok(())
    }
    fn validate_structure(&self) -> Result<(), SettingsError> {
        if self.schema_version != 1 {
            return Err(SettingsError::Schema);
        }
        if !LANGUAGES
            .iter()
            .any(|language| language.code == self.target_language)
            || !matches!(self.theme.as_str(), "default" | "amoled")
            || !matches!(self.interface_language.as_str(), "uk" | "en")
            || !(self.accent_color.len() == 7
                && self.accent_color.starts_with('#')
                && self.accent_color.as_bytes()[1..]
                    .iter()
                    .all(u8::is_ascii_hexdigit))
            || !source_refs_valid(&self.favorite_sources, MAX_FAVORITE_SOURCES)
            || !source_refs_valid(&self.hidden_sources, MAX_HIDDEN_SOURCES)
            || !valid_gain(self.original_volume)
            || !valid_gain(self.translation_volume)
            || self.api_key.len() > 4096
            || self
                .selected_source_id
                .as_ref()
                .is_some_and(|id| id.len() > 4096)
            || self
                .last_captured_source_id
                .as_ref()
                .is_some_and(|id| id.is_empty() || id.len() > 4096)
            || self
                .capture_shortcut
                .as_ref()
                .is_some_and(|key| key.is_empty() || key.len() > 128)
        {
            return Err(SettingsError::Validation);
        }
        Ok(())
    }
}
pub fn valid_gain(gain: f32) -> bool {
    gain.is_finite() && (0.0..=1.0).contains(&gain)
}

/// Structural checks only: counts, non-empty values, UTF-8 byte lengths, unique ids within a list.
fn source_refs_valid(records: &[SourceRef], max_count: usize) -> bool {
    let mut ids = HashSet::with_capacity(records.len());
    records.len() <= max_count
        && records.iter().all(|record| {
            !record.id.is_empty()
                && record.id.len() <= MAX_SOURCE_ID_LEN
                && !record.name.is_empty()
                && record.name.len() <= MAX_SOURCE_NAME_LEN
                && ids.insert(record.id.as_str())
        })
}

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("Непідтримувана версія налаштувань. Файл не змінено.")]
    Schema,
    #[error("Некоректні налаштування. Перевірте мову, гучність і список програм.")]
    Validation,
    #[error("Список програм завеликий для збереження. Приберіть частину обраних або прихованих програм.")]
    TooLarge,
    #[error("Файл налаштувань пошкоджено. Його не буде перезаписано.")]
    Corrupt,
    #[error("Не вдалося прочитати або зберегти налаштування. Перевірте доступ до папки.")]
    Io,
}
pub fn load_settings(dir: &Path) -> Result<Settings, SettingsError> {
    match fs::read(dir.join("settings.json")) {
        Ok(bytes) => {
            if bytes.len() > 65536 {
                return Err(SettingsError::Corrupt);
            }
            let settings: Settings =
                serde_json::from_slice(&bytes).map_err(|_| SettingsError::Corrupt)?;
            settings.validate_structure()?;
            Ok(settings)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Settings::default()),
        Err(_) => Err(SettingsError::Io),
    }
}
pub fn save_settings(dir: &Path, settings: &Settings) -> Result<(), SettingsError> {
    settings.validate()?;
    load_settings(dir)?;
    fs::create_dir_all(dir).map_err(|_| SettingsError::Io)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))
            .map_err(|_| SettingsError::Io)?;
    }
    let bytes = serde_json::to_vec_pretty(settings).map_err(|_| SettingsError::Validation)?;
    let mut file = tempfile::NamedTempFile::new_in(dir).map_err(|_| SettingsError::Io)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.as_file()
            .set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|_| SettingsError::Io)?;
    }
    file.write_all(&bytes)
        .and_then(|_| file.write_all(b"\n"))
        .and_then(|_| file.as_file().sync_all())
        .map_err(|_| SettingsError::Io)?;
    file.persist(dir.join("settings.json"))
        .map_err(|_| SettingsError::Io)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn supported_languages_match_provider_and_roundtrip_settings() {
        let expected: HashSet<&str> = "af ak sq am ar hy az eu be bn bg my ca zh-Hans zh-Hant hr cs da nl en et fil fi fr gl ka de el gu ha he hi hu is id it ja jv kn kk km rw ko lo lv lt mk ms ml mr mn ne no fa pl pt-BR pt-PT pa ro ru sr sd si sk sl es su sw sv ta te th tr uk ur uz vi zu".split_whitespace().collect();
        let actual: HashSet<&str> = LANGUAGES.iter().map(|language| language.code).collect();
        assert_eq!(expected.len(), 78);
        assert_eq!(LANGUAGES.len(), 78);
        assert_eq!(actual, expected);
        assert_eq!(LANGUAGES[0].code, "uk");
        let translations: serde_json::Value =
            serde_json::from_str(include_str!("../../../src/lib/messages.json")).unwrap();
        let dir = tempfile::tempdir().unwrap();
        for language in LANGUAGES {
            assert!(
                translations[language.name]
                    .as_str()
                    .is_some_and(|name| !name.is_empty()),
                "missing English name for {}",
                language.code
            );
            let settings = Settings {
                target_language: language.code.into(),
                ..Settings::default()
            };
            settings.validate().unwrap();
            save_settings(dir.path(), &settings).unwrap();
            assert!(
                load_settings(dir.path()).unwrap() == settings,
                "{}",
                language.code
            );
        }
        for unsupported in ["xx", "nb", "zh", "pt"] {
            let settings = Settings {
                target_language: unsupported.into(),
                ..Settings::default()
            };
            assert!(matches!(
                settings.validate(),
                Err(SettingsError::Validation)
            ));
        }
    }
    fn record(id: &str, name: &str) -> SourceRef {
        SourceRef {
            id: id.into(),
            name: name.into(),
        }
    }
    #[test]
    fn source_records_default_roundtrip_and_keep_exact_ids() {
        let legacy: Settings = serde_json::from_str(r#"{"targetLanguage":"en"}"#).unwrap();
        assert!(legacy.favorite_sources.is_empty() && legacy.hidden_sources.is_empty());
        let dir = tempfile::tempdir().unwrap();
        let custom = Settings {
            favorite_sources: vec![record("app:/Applications/A.app", "A")],
            hidden_sources: vec![record("app:/Applications/Gone.app", "Gone")],
            ..Settings::default()
        };
        save_settings(dir.path(), &custom).unwrap();
        let saved = serde_json::to_value(load_settings(dir.path()).unwrap()).unwrap();
        assert_eq!(
            saved["hiddenSources"][0]["id"],
            "app:/Applications/Gone.app"
        );
        assert_eq!(saved["hiddenSources"][0]["name"], "Gone");
    }

    #[test]
    fn oversized_legacy_file_stays_loadable_but_cannot_be_saved() {
        let dir = tempfile::tempdir().unwrap();
        let quoted = |n: usize| record(&format!("app:/{n:02}{}", "\"".repeat(470)), "A");
        let legacy = Settings {
            favorite_sources: (0..32).map(quoted).collect(),
            hidden_sources: (32..64).map(quoted).collect(),
            ..Settings::default()
        };
        let bytes = serde_json::to_vec_pretty(&legacy).unwrap();
        // Written before the 60 KiB save guard: readable by the existing 64 KiB load limit.
        assert!(MAX_SERIALIZED_BYTES < bytes.len() && bytes.len() <= 65536);
        let path = dir.path().join("settings.json");
        let mut file = bytes.clone();
        file.push(b'\n');
        fs::write(&path, &file).unwrap();
        let loaded = load_settings(dir.path()).unwrap();
        assert_eq!(loaded.hidden_sources, legacy.hidden_sources);
        assert!(matches!(loaded.validate(), Err(SettingsError::TooLarge)));
        assert!(save_settings(dir.path(), &loaded).is_err());
        assert_eq!(fs::read(&path).unwrap(), file);
    }

    #[test]
    fn a_hidden_favorite_may_appear_in_both_lists() {
        let dir = tempfile::tempdir().unwrap();
        let both = Settings {
            favorite_sources: vec![record("app:/A.app", "A")],
            hidden_sources: vec![record("app:/A.app", "A")],
            ..Settings::default()
        };
        save_settings(dir.path(), &both).unwrap();
        assert!(load_settings(dir.path()).unwrap() == both);
    }

    #[test]
    fn source_records_reject_bounds_escaped_size_and_duplicates_without_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        save_settings(dir.path(), &Settings::default()).unwrap();
        let original = fs::read(dir.path().join("settings.json")).unwrap();
        let many = Settings {
            favorite_sources: (0..33)
                .map(|n| record(&format!("app:/F{n}.app"), "F"))
                .collect(),
            ..Settings::default()
        };
        let long_id = Settings {
            favorite_sources: vec![record(&format!("app:/{}", "x".repeat(509)), "A")],
            ..Settings::default()
        };
        let long_name = Settings {
            hidden_sources: vec![record("app:/A.app", &"n".repeat(129))],
            ..Settings::default()
        };
        let duplicate = Settings {
            hidden_sources: vec![record("app:/A.app", "A"), record("app:/A.app", "A")],
            ..Settings::default()
        };
        let empty_id = Settings {
            favorite_sources: vec![record("", "A")],
            ..Settings::default()
        };
        // 505-byte ids of quotes expand 2× in JSON: only a serialized-size check catches this.
        let quoted = |n: usize| record(&format!("app:/{n:02}{}", "\"".repeat(498)), "A");
        let escaped = Settings {
            favorite_sources: (0..32).map(quoted).collect(),
            hidden_sources: (32..64).map(quoted).collect(),
            ..Settings::default()
        };
        assert!(serde_json::to_vec_pretty(&escaped).unwrap().len() > MAX_SERIALIZED_BYTES);
        for invalid in [many, long_id, long_name, duplicate, empty_id, escaped] {
            assert!(save_settings(dir.path(), &invalid).is_err());
            assert_eq!(
                fs::read(dir.path().join("settings.json")).unwrap(),
                original
            );
        }
    }
    #[test]
    fn legacy_settings_gain_safe_shortcut_defaults() {
        let value: Settings = serde_json::from_str(r#"{"targetLanguage":"en"}"#).unwrap();
        let json = serde_json::to_value(&value).unwrap();
        assert_eq!(json["shortcutMode"], "activeApplication");
        assert!(json["captureShortcut"].is_null());
        assert!(json["lastCapturedSourceId"].is_null());
        let dir = tempfile::tempdir().unwrap();
        let extended: Settings = serde_json::from_value(serde_json::json!({
            "captureShortcut":"Control+Shift+KeyT", "shortcutMode":"lastCaptured",
            "lastCapturedSourceId":"app:/Test.app"
        }))
        .unwrap();
        save_settings(dir.path(), &extended).unwrap();
        let saved = serde_json::to_value(load_settings(dir.path()).unwrap()).unwrap();
        assert_eq!(saved["captureShortcut"], "Control+Shift+KeyT");
        assert_eq!(saved["lastCapturedSourceId"], "app:/Test.app");
    }
    #[test]
    fn appearance_defaults_roundtrip_and_validation() {
        let legacy: Settings =
            serde_json::from_str(r#"{"targetLanguage":"en","apiKey":"test-only"}"#).unwrap();
        let value = serde_json::to_value(&legacy).unwrap();
        assert_eq!(value["theme"], "default");
        assert_eq!(value["accentColor"], "#c5e89d");
        assert_eq!(value["interfaceLanguage"], "uk");
        let dir = tempfile::tempdir().unwrap();
        let mut value = value;
        value["theme"] = "amoled".into();
        value["accentColor"] = "#102aEF".into();
        value["interfaceLanguage"] = "en".into();
        let custom: Settings = serde_json::from_value(value.clone()).unwrap();
        save_settings(dir.path(), &custom).unwrap();
        assert!(load_settings(dir.path()).unwrap() == custom);
        let original = fs::read(dir.path().join("settings.json")).unwrap();
        for (key, invalid) in [
            ("theme", "light"),
            ("interfaceLanguage", "xx"),
            ("accentColor", "red"),
            ("accentColor", "#fff"),
            ("accentColor", "#ffffff00"),
            ("accentColor", "#gggggg"),
        ] {
            let mut invalid_value = value.clone();
            invalid_value[key] = invalid.into();
            if let Ok(settings) = serde_json::from_value::<Settings>(invalid_value) {
                assert!(
                    save_settings(dir.path(), &settings).is_err(),
                    "{key}={invalid}"
                );
            }
            assert_eq!(
                fs::read(dir.path().join("settings.json")).unwrap(),
                original
            );
        }
    }
    #[test]
    fn missing_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let s = load_settings(dir.path()).unwrap();
        assert_eq!(s.schema_version, 1);
        assert_eq!(s.target_language, "uk");
        assert_eq!((s.original_volume, s.translation_volume), (0.15, 0.7));
        assert!(s.api_key.is_empty() && s.selected_source_id.is_none());
    }
    #[test]
    fn roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let s = Settings {
            api_key: "test-not-a-real-key".into(),
            selected_source_id: Some("app:/Applications/Test.app".into()),
            ..Settings::default()
        };
        save_settings(dir.path(), &s).unwrap();
        assert!(load_settings(dir.path()).unwrap() == s);
    }
    #[test]
    fn invalid_values_preserve_old_file() {
        let dir = tempfile::tempdir().unwrap();
        let s = Settings::default();
        save_settings(dir.path(), &s).unwrap();
        let original = std::fs::read(dir.path().join("settings.json")).unwrap();
        for gain in [f32::NAN, f32::INFINITY, -0.01, 1.01] {
            let invalid = Settings {
                original_volume: gain,
                ..s.clone()
            };
            assert!(save_settings(dir.path(), &invalid).is_err());
        }
        for invalid in [
            Settings {
                schema_version: 2,
                ..s.clone()
            },
            Settings {
                target_language: "xx".into(),
                ..s.clone()
            },
            Settings {
                api_key: "x".repeat(4097),
                ..s.clone()
            },
            Settings {
                selected_source_id: Some("x".repeat(4097)),
                ..s.clone()
            },
        ] {
            assert!(save_settings(dir.path(), &invalid).is_err());
        }
        assert_eq!(
            std::fs::read(dir.path().join("settings.json")).unwrap(),
            original
        );
    }
    #[test]
    fn corrupt_or_future_schema_not_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        for original in [b"broken".as_slice(), br#"{"schemaVersion":9}"#.as_slice()] {
            std::fs::write(&path, original).unwrap();
            assert!(load_settings(dir.path()).is_err());
            assert!(save_settings(dir.path(), &Settings::default()).is_err());
            assert_eq!(std::fs::read(&path).unwrap(), original);
        }
    }
    #[cfg(unix)]
    #[test]
    fn unix_private_modes() {
        use std::os::unix::fs::PermissionsExt;
        let parent = tempfile::tempdir().unwrap();
        let dir = parent.path().join("app");
        save_settings(&dir, &Settings::default()).unwrap();
        assert_eq!(
            std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::metadata(dir.join("settings.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    #[test]
    fn write_error() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("file");
        std::fs::write(&file, "keep").unwrap();
        assert!(save_settings(&file, &Settings::default()).is_err());
        assert_eq!(std::fs::read(file).unwrap(), b"keep");
    }
}
