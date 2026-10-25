use serde::{Deserialize, Serialize};
use std::{fs, io::Write, path::Path};

#[derive(Clone, Serialize)]
pub struct Language {
    pub code: &'static str,
    pub name: &'static str,
}
pub const LANGUAGES: &[Language] = &[
    Language {
        code: "uk",
        name: "Українська",
    },
    Language {
        code: "en",
        name: "English",
    },
    Language {
        code: "pl",
        name: "Polski",
    },
    Language {
        code: "de",
        name: "Deutsch",
    },
];

#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub schema_version: u32,
    pub api_key: String,
    pub target_language: String,
    pub original_volume: f32,
    pub translation_volume: f32,
    pub selected_source_id: Option<String>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: 1,
            api_key: String::new(),
            target_language: "uk".into(),
            original_volume: 0.15,
            translation_volume: 0.7,
            selected_source_id: None,
        }
    }
}
impl Settings {
    pub fn validate(&self) -> Result<(), SettingsError> {
        if self.schema_version != 1 {
            return Err(SettingsError::Schema);
        }
        if !LANGUAGES
            .iter()
            .any(|language| language.code == self.target_language)
            || !valid_gain(self.original_volume)
            || !valid_gain(self.translation_volume)
            || self.api_key.len() > 4096
            || self
                .selected_source_id
                .as_ref()
                .is_some_and(|id| id.len() > 4096)
        {
            return Err(SettingsError::Validation);
        }
        Ok(())
    }
}
pub fn valid_gain(gain: f32) -> bool {
    gain.is_finite() && (0.0..=1.0).contains(&gain)
}

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("Непідтримувана версія налаштувань. Файл не змінено.")]
    Schema,
    #[error("Некоректні налаштування. Перевірте мову та гучність.")]
    Validation,
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
            settings.validate()?;
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
