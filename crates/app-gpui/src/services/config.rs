//! Settings file access (port of the settings commands in `src-tauri/src/commands.rs`).

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
use translator_core::settings::{self, Settings};

/// Same identifier as the Tauri bundle so existing users keep their settings.
const BUNDLE_IDENTIFIER: &str = "dev.live-translator.desktop";
const UNAVAILABLE: &str = "Налаштування тимчасово недоступні.";
const OPEN_FAILED: &str = "Не вдалося відкрити папку налаштувань.";

/// All settings file access is serialized by one lock.
#[derive(Clone)]
pub struct ConfigStore {
    pub directory: PathBuf,
    lock: Arc<Mutex<()>>,
}

impl ConfigStore {
    pub fn new() -> Self {
        Self::at(
            dirs::config_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(BUNDLE_IDENTIFIER),
        )
    }

    pub fn at(directory: PathBuf) -> Self {
        Self {
            directory,
            lock: Arc::new(Mutex::new(())),
        }
    }

    pub fn load(&self) -> Result<Settings, String> {
        let _guard = self.lock.lock().map_err(|_| UNAVAILABLE)?;
        settings::load_settings(&self.directory).map_err(|e| e.to_string())
    }

    /// Validates the new settings and reads the stored file (the previous state) in one locked step.
    pub fn validate_against_stored(&self, settings: &Settings) -> Result<Settings, String> {
        let _guard = self.lock.lock().map_err(|_| UNAVAILABLE)?;
        settings.validate().map_err(|e| e.to_string())?;
        settings::load_settings(&self.directory).map_err(|e| e.to_string())
    }

    pub fn save(&self, settings: &Settings) -> Result<(), String> {
        let _guard = self.lock.lock().map_err(|_| UNAVAILABLE)?;
        settings::save_settings(&self.directory, settings).map_err(|e| e.to_string())
    }

    /// Only the app-selected config directory can be opened, never a caller-supplied path.
    pub fn open_directory(&self) -> Result<(), String> {
        std::fs::create_dir_all(&self.directory).map_err(|_| OPEN_FAILED)?;
        open::that(&self.directory).map_err(|_| OPEN_FAILED.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directory_matches_the_tauri_app_config_directory() {
        let store = ConfigStore::new();
        assert_eq!(
            store.directory.file_name().and_then(|name| name.to_str()),
            Some("dev.live-translator.desktop")
        );
        assert_eq!(store.directory.parent(), dirs::config_dir().as_deref());
    }

    #[test]
    fn missing_file_loads_defaults_and_saves_round_trip() {
        let dir = std::env::temp_dir().join(format!("live-translator-test-{}", std::process::id()));
        let store = ConfigStore::at(dir.clone());
        assert!(store.load().unwrap() == Settings::default());
        let changed = Settings {
            original_volume: 0.4,
            ..Settings::default()
        };
        store.save(&changed).unwrap();
        assert_eq!(store.load().unwrap().original_volume, 0.4);
        let invalid = Settings {
            target_language: "xx".into(),
            ..Settings::default()
        };
        assert!(store.validate_against_stored(&invalid).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}
