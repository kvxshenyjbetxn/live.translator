//! Global shortcut registration and press handling (port of
//! `src-tauri/src/desktop/shortcut.rs` onto `global-hotkey`).

use super::{foreground, lifecycle::CloseState};
use crate::logic::desktop::{ActionKind, DesktopAction};
use futures::channel::mpsc::UnboundedSender;
use global_hotkey::{
    hotkey::{HotKey, Modifiers},
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
};
use std::{
    collections::HashSet,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
use translator_core::session::{SessionManager, SessionPhase};

#[derive(Clone, Default)]
pub struct Status {
    pub active: Option<String>,
    pub error: Option<String>,
    pub integration_error: Option<String>,
}

/// State shared with the global-hotkey event thread.
#[derive(Default)]
pub struct Control {
    pub status: Mutex<Status>,
    gate: Mutex<PressGate>,
    suspended: AtomicBool,
}

impl Control {
    pub fn status(&self) -> Status {
        self.status.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn active(&self) -> Option<String> {
        self.status().active
    }

    pub fn set_suspended(&self, suspended: bool) {
        self.suspended.store(suspended, Ordering::SeqCst);
    }
}

#[derive(Default)]
struct PressGate {
    down: bool,
}

impl PressGate {
    fn accept(&mut self, pressed: bool, suspended: bool) -> bool {
        let trigger = pressed && !self.down && !suspended;
        self.down = pressed;
        trigger
    }
}

pub fn parse(value: &str) -> Result<HotKey, String> {
    let shortcut: HotKey = value.parse().map_err(|_| "Некоректна гаряча клавіша.")?;
    if !shortcut
        .mods
        .intersects(Modifiers::CONTROL | Modifiers::ALT | Modifiers::SUPER | Modifiers::SHIFT)
    {
        return Err("Додайте модифікатор до гарячої клавіші.".into());
    }
    Ok(shortcut)
}

/// Preserve an unchanged but unavailable stored binding; a rejected replacement
/// instead reverts to the actual working registration without losing other edits.
pub fn fallback(
    previous: Option<&str>,
    requested: Option<&str>,
    active: Option<&str>,
) -> Option<String> {
    if previous == requested {
        requested.map(str::to_owned)
    } else {
        active.map(str::to_owned)
    }
}

/// What a settings save reports back for shortcut reconciliation.
pub struct SaveResult {
    pub capture_shortcut: Option<String>,
    pub shortcut_error: Option<String>,
}

trait Registry {
    fn register(&mut self, key: &str) -> Result<(), String>;
    fn unregister(&mut self, key: &str) -> Result<(), String>;
}

fn replace(
    registry: &mut impl Registry,
    old: Option<&str>,
    new: Option<&str>,
    persist: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    if old == new {
        return persist();
    }
    // Reserve replacement before releasing the old binding. Actions are suspended
    // for this transaction; failed persistence restores the previous registration.
    if let Some(new) = new {
        registry.register(new)?;
    }
    if let Some(old) = old {
        if let Err(error) = registry.unregister(old) {
            if let Some(new) = new {
                registry.unregister(new)?;
            }
            return Err(error);
        }
    }
    if let Err(error) = persist() {
        let restored = old.map(|old| registry.register(old)).transpose();
        let removed = new.map(|new| registry.unregister(new)).transpose();
        restored?;
        removed?;
        return Err(error);
    }
    Ok(())
}

struct Native<'a> {
    manager: Option<&'a GlobalHotKeyManager>,
    registered: &'a mut HashSet<String>,
}

impl Registry for Native<'_> {
    fn register(&mut self, key: &str) -> Result<(), String> {
        let Some(manager) = self.manager else {
            return Err("Глобальні клавіші недоступні на цій системі.".into());
        };
        manager.register(parse(key)?).map_err(|_| {
            "Не вдалося зареєструвати клавішу: можливо, вона зайнята системою або іншою програмою."
                .to_string()
        })?;
        self.registered.insert(key.to_string());
        Ok(())
    }

    fn unregister(&mut self, key: &str) -> Result<(), String> {
        let Some(manager) = self.manager else {
            return Err("Не вдалося вимкнути попередню гарячу клавішу.".into());
        };
        manager
            .unregister(parse(key)?)
            .map_err(|_| "Не вдалося вимкнути попередню гарячу клавішу.".to_string())?;
        self.registered.remove(key);
        Ok(())
    }
}

/// Owns the OS registration; must live and be used on the thread that created it (the UI thread).
pub struct Shortcuts {
    pub control: Arc<Control>,
    manager: Option<GlobalHotKeyManager>,
    registered: HashSet<String>,
}

impl Shortcuts {
    /// Creates the manager and installs the press handler. Presses become desktop actions on
    /// `actions`; the foreground application is snapshotted at press time.
    pub fn initialize(
        session: Arc<SessionManager>,
        close: Arc<CloseState>,
        actions: UnboundedSender<DesktopAction>,
    ) -> Self {
        let control = Arc::new(Control::default());
        let manager = match GlobalHotKeyManager::new() {
            Ok(manager) => Some(manager),
            Err(_) => {
                control
                    .status
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .integration_error =
                    Some("Глобальні клавіші недоступні; використовуйте вікно або трей.".into());
                None
            }
        };
        let handler_control = control.clone();
        GlobalHotKeyEvent::set_event_handler(Some(move |event: GlobalHotKeyEvent| {
            let active = handler_control.active();
            if active
                .as_deref()
                .and_then(|key| parse(key).ok())
                .is_none_or(|key| key.id() != event.id)
            {
                return;
            }
            let pressed = event.state == HotKeyState::Pressed;
            let accepted = handler_control
                .gate
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .accept(pressed, handler_control.suspended.load(Ordering::SeqCst));
            if !accepted || !close.is_ready() {
                return;
            }
            let phase = session.status().phase;
            if phase == SessionPhase::Stopping {
                return;
            }
            let running = matches!(phase, SessionPhase::Starting | SessionPhase::Running);
            let mut action = DesktopAction::new(
                if running {
                    ActionKind::Stop
                } else {
                    ActionKind::Shortcut
                },
                None,
            );
            if !running {
                // Always snapshot: the UI may have an unsaved mode change.
                // Last-captured mode ignores this value/error rather than guessing.
                match foreground::current() {
                    Ok(source) => action.value = Some(source),
                    Err(error) => action.error = Some(error),
                }
            }
            let _ = actions.unbounded_send(action);
        }));
        Self {
            control,
            manager,
            registered: HashSet::new(),
        }
    }

    /// Registers `desired` (replacing the active binding) and runs `persist` inside the
    /// transaction; on failure the previous registration is restored.
    pub fn configure(
        &mut self,
        desired: Option<&str>,
        persist: impl FnOnce() -> Result<(), String>,
    ) -> Result<(), String> {
        let control = self.control.clone();
        let old = control.active();
        let desired = match desired.map(|key| parse(key).map(|key| key.to_string())).transpose() {
            Ok(value) => value,
            Err(error) => {
                control.status.lock().unwrap_or_else(|e| e.into_inner()).error = Some(error.clone());
                return Err(error);
            }
        };
        let was_suspended = control.suspended.swap(true, Ordering::SeqCst);
        let result = replace(
            &mut Native {
                manager: self.manager.as_ref(),
                registered: &mut self.registered,
            },
            old.as_deref(),
            desired.as_deref(),
            persist,
        );
        let mut status = control.status.lock().unwrap_or_else(|e| e.into_inner());
        match &result {
            Ok(()) => {
                status.active = desired.clone();
                status.error = None;
                if old != desired {
                    control.gate.lock().unwrap_or_else(|e| e.into_inner()).down = false;
                }
            }
            Err(message) => {
                // Rollback can itself fail (e.g. another process claimed the old key).
                // Report only a registration that still exists, never an assumed one.
                status.active = [old.as_ref(), desired.as_ref()]
                    .into_iter()
                    .flatten()
                    .find(|key| self.registered.contains(key.as_str()))
                    .cloned();
                status.error = Some(message.clone());
            }
        }
        control.suspended.store(was_suspended, Ordering::SeqCst);
        result
    }

    /// Whether saving `desired` needs an OS registration change (or must report a parse error).
    /// When it does not, the file write can happen off the UI thread.
    pub fn needs_registry(&self, desired: Option<&str>) -> bool {
        let wanted = match desired.map(|key| parse(key).map(|key| key.to_string())).transpose() {
            Ok(value) => value,
            Err(_) => return true,
        };
        wanted != self.control.active()
    }

    /// Records the outcome of a save that did not touch the OS registration.
    pub fn set_error(&self, error: Option<String>) {
        self.control.status.lock().unwrap_or_else(|e| e.into_inner()).error = error;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_stored_binding_does_not_block_other_settings_but_replacement_reverts() {
        assert_eq!(fallback(Some("old"), Some("old"), None), Some("old".into()));
        assert_eq!(fallback(Some("old"), Some("new"), Some("old")), Some("old".into()));
        assert_eq!(fallback(Some("old"), Some("new"), None), None);
    }

    #[test]
    fn shortcuts_require_modifier_and_non_modifier_key() {
        assert!(parse("KeyT").is_err());
        assert!(parse("Shift").is_err());
        assert!(parse("Control+Shift+KeyT").is_ok());
    }

    #[test]
    fn held_keys_recording_and_release_never_trigger_twice() {
        let mut gate = PressGate::default();
        assert!(gate.accept(true, false));
        assert!(!gate.accept(true, false));
        assert!(!gate.accept(false, false));
        assert!(!gate.accept(true, true));
        assert!(!gate.accept(true, false));
        assert!(!gate.accept(false, false));
        assert!(gate.accept(true, false));
    }

    #[derive(Default)]
    struct Fake {
        registered: Vec<String>,
        fail: Option<String>,
    }

    impl Registry for Fake {
        fn register(&mut self, key: &str) -> Result<(), String> {
            if self.fail.as_deref() == Some(key) {
                return Err("conflict".into());
            }
            self.registered.push(key.into());
            Ok(())
        }

        fn unregister(&mut self, key: &str) -> Result<(), String> {
            self.registered.retain(|s| s != key);
            Ok(())
        }
    }

    #[test]
    fn registration_and_storage_failures_keep_previous_binding() {
        let mut r = Fake {
            registered: vec!["old".into()],
            fail: Some("new".into()),
        };
        assert!(replace(&mut r, Some("old"), Some("new"), || Ok(())).is_err());
        assert_eq!(r.registered, vec!["old"]);
        r.fail = None;
        assert!(replace(&mut r, Some("old"), Some("new"), || Err("disk".into())).is_err());
        assert_eq!(r.registered, vec!["old"]);
        replace(&mut r, Some("old"), None, || Ok(())).unwrap();
        assert!(r.registered.is_empty());
    }
}
