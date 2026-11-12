//! Serial settings writes with immutable snapshots (port of `src/lib/autosave.ts`).
//!
//! The store is a synchronous state machine; `AppState` drives the timer and the asynchronous
//! write. Snapshots are JSON strings, so an old completion can never replace a newer edit.

use translator_core::settings::Settings;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveState {
    Loading,
    Saved,
    Saving,
    Unsaved,
    Error,
}

pub const SAVE_DELAY_MS: u64 = 350;
pub const NOT_LOADED: &str = "Налаштування ще не завантажено.";
const SAVE_FAILED: &str = "Не вдалося зберегти налаштування. Перевірте доступ до папки й повторіть.";

/// Backend save failures are arbitrary text that can echo a path or the file it tried to write, so
/// only these fixed, user-actionable refusals are shown; everything else is masked.
pub const ACTIONABLE_REFUSALS: [&str; 2] = [
    "Некоректні налаштування. Перевірте мову, гучність і список програм.",
    "Список програм завеликий для збереження. Приберіть частину обраних або прихованих програм.",
];

pub fn save_failure(reason: &str) -> String {
    if ACTIONABLE_REFUSALS.contains(&reason) {
        reason.into()
    } else {
        SAVE_FAILED.into()
    }
}

/// What a settings edit changed in the store.
#[derive(Debug, PartialEq, Eq)]
pub struct Update {
    pub state: Option<SaveState>,
    /// Whether the debounce timer must be (re)armed; any earlier timer is always cancelled.
    pub arm_timer: bool,
}

#[derive(Default)]
pub struct AutosaveStore {
    desired: Option<String>,
    saved: Option<String>,
    writing: bool,
}

fn snapshot(settings: &Settings) -> String {
    serde_json::to_string(settings).unwrap_or_default()
}

impl AutosaveStore {
    pub fn init(&mut self, settings: &Settings) -> SaveState {
        let value = snapshot(settings);
        self.desired = Some(value.clone());
        self.saved = Some(value);
        SaveState::Saved
    }

    pub fn is_initialized(&self) -> bool {
        self.desired.is_some()
    }

    /// `None` when nothing changed or the initial load has not happened yet.
    pub fn update(&mut self, settings: &Settings) -> Option<Update> {
        let current = self.desired.as_ref()?;
        let next = snapshot(settings);
        if &next == current {
            return None;
        }
        let unchanged_from_disk = self.saved.as_ref() == Some(&next) && !self.writing;
        self.desired = Some(next);
        if unchanged_from_disk {
            return Some(Update {
                state: Some(SaveState::Saved),
                arm_timer: false,
            });
        }
        Some(Update {
            state: Some(if self.writing {
                SaveState::Saving
            } else {
                SaveState::Unsaved
            }),
            arm_timer: true,
        })
    }

    /// True while the desired snapshot differs from what is on disk.
    pub fn is_dirty(&self) -> bool {
        self.desired != self.saved
    }

    pub fn is_writing(&self) -> bool {
        self.writing
    }

    /// Claims the single writer; `false` when a drain is already running.
    pub fn start_drain(&mut self) -> bool {
        if self.writing {
            return false;
        }
        self.writing = true;
        true
    }

    /// The next snapshot to write (its JSON key and the settings), or `None` when disk is current.
    pub fn next_write(&mut self) -> Option<(String, Settings)> {
        if self.desired == self.saved {
            return None;
        }
        let key = self.desired.clone()?;
        let settings = serde_json::from_str(&key).ok()?;
        Some((key, settings))
    }

    /// Records a successful write of the snapshot `next_write` returned.
    pub fn wrote(&mut self, key: String) {
        self.saved = Some(key);
    }

    /// Ends the drain and returns the resulting state.
    pub fn end_drain(&mut self, failed: bool) -> SaveState {
        self.writing = false;
        if failed {
            SaveState::Error
        } else {
            SaveState::Saved
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_volume(volume: f32) -> Settings {
        Settings {
            original_volume: volume,
            ..Settings::default()
        }
    }

    #[test]
    fn does_not_track_edits_before_the_initial_load() {
        let mut store = AutosaveStore::default();
        assert_eq!(store.update(&with_volume(0.5)), None);
        assert!(!store.is_dirty());
        store.init(&Settings::default());
        assert!(store.update(&with_volume(0.2)).unwrap().arm_timer);
    }

    #[test]
    fn rapid_edits_write_only_the_latest_value() {
        let mut store = AutosaveStore::default();
        store.init(&Settings::default());
        store.update(&with_volume(0.2));
        store.update(&with_volume(0.4));
        assert!(store.start_drain());
        let (key, write) = store.next_write().unwrap();
        assert_eq!(write.original_volume, 0.4);
        store.wrote(key);
        assert!(store.next_write().is_none());
        assert_eq!(store.end_drain(false), SaveState::Saved);
    }

    #[test]
    fn edits_during_a_write_are_serialized_and_picked_up_by_the_same_drain() {
        let mut store = AutosaveStore::default();
        store.init(&Settings::default());
        store.update(&with_volume(0.2));
        assert!(store.start_drain());
        assert!(!store.start_drain(), "only one writer at a time");
        let (first_key, _) = store.next_write().unwrap();
        let during = store.update(&with_volume(0.8)).unwrap();
        assert_eq!(during.state, Some(SaveState::Saving));
        store.wrote(first_key);
        let (second_key, second) = store.next_write().unwrap();
        assert_eq!(second.original_volume, 0.8);
        store.wrote(second_key);
        assert!(store.next_write().is_none());
        assert_eq!(store.end_drain(false), SaveState::Saved);
    }

    #[test]
    fn failed_save_stays_unsaved_and_retry_writes_the_current_value() {
        let mut store = AutosaveStore::default();
        store.init(&Settings::default());
        store.update(&with_volume(0.3));
        assert!(store.start_drain());
        assert!(store.next_write().is_some());
        assert_eq!(store.end_drain(true), SaveState::Error);
        assert!(store.is_dirty());
        store.update(&with_volume(0.9));
        assert!(store.start_drain());
        let (key, write) = store.next_write().unwrap();
        assert_eq!(write.original_volume, 0.9);
        store.wrote(key);
        assert_eq!(store.end_drain(false), SaveState::Saved);
        assert!(!store.is_dirty());
    }

    #[test]
    fn reverting_to_the_saved_value_cancels_the_pending_write() {
        let mut store = AutosaveStore::default();
        store.init(&Settings::default());
        store.update(&with_volume(0.9));
        let back = store.update(&Settings::default()).unwrap();
        assert_eq!(back.state, Some(SaveState::Saved));
        assert!(!back.arm_timer);
    }

    #[test]
    fn actionable_refusals_survive_and_other_text_is_masked() {
        for refusal in ACTIONABLE_REFUSALS {
            assert_eq!(save_failure(refusal), refusal);
        }
        let masked = save_failure(
            r#"write failed near {"apiKey":"AIza-test-only"} in /Users/test/settings.json"#,
        );
        assert_eq!(masked, SAVE_FAILED);
        assert!(!masked.contains("AIza") && !masked.contains("apiKey"));
    }
}
