use serde::Serialize;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use translator_core::{
    session::{SessionManager, SessionPhase},
    settings::Settings,
};

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub active: Option<String>,
    pub error: Option<String>,
    pub integration_error: Option<String>,
}
#[derive(Default)]
pub struct Control {
    pub status: Mutex<Status>,
    gate: Mutex<PressGate>,
    suspended: AtomicBool,
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
pub fn parse(value: &str) -> Result<Shortcut, String> {
    let shortcut: Shortcut = value.parse().map_err(|_| "Некоректна гаряча клавіша.")?;
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
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
struct Native<'a>(&'a AppHandle);
impl Registry for Native<'_> {
    fn register(&mut self, key: &str) -> Result<(), String> {
        if self
            .0
            .try_state::<tauri_plugin_global_shortcut::GlobalShortcut<tauri::Wry>>()
            .is_none()
        {
            return Err("Глобальні клавіші недоступні на цій системі.".into());
        }
        self.0.global_shortcut().register(parse(key)?).map_err(|_| "Не вдалося зареєструвати клавішу: можливо, вона зайнята системою або іншою програмою.".into())
    }
    fn unregister(&mut self, key: &str) -> Result<(), String> {
        self.0
            .global_shortcut()
            .unregister(parse(key)?)
            .map_err(|_| "Не вдалося вимкнути попередню гарячу клавішу.".into())
    }
}
pub fn configure(
    app: &AppHandle,
    settings: &Settings,
    persist: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    let control = app.state::<Control>();
    let old = control
        .status
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .active
        .clone();
    let desired = settings
        .capture_shortcut
        .as_ref()
        .map(|key| parse(key).map(|key| key.to_string()))
        .transpose();
    let desired = match desired {
        Ok(value) => value,
        Err(error) => {
            control
                .status
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .error = Some(error.clone());
            return Err(error);
        }
    };
    let was_suspended = control.suspended.swap(true, Ordering::SeqCst);
    let result = replace(
        &mut Native(app),
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
            status.active = app
                .try_state::<tauri_plugin_global_shortcut::GlobalShortcut<tauri::Wry>>()
                .and_then(|plugin| {
                    [old.as_ref(), desired.as_ref()]
                        .into_iter()
                        .flatten()
                        .find(|key| plugin.is_registered(key.as_str()))
                        .cloned()
                });
            status.error = Some(message.clone());
        }
    }
    control.suspended.store(was_suspended, Ordering::SeqCst);
    result
}
pub fn initialize(app: &AppHandle) {
    let plugin = tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, shortcut, event| {
            let control = app.state::<Control>();
            let active = control
                .status
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .active
                .clone();
            if active
                .as_deref()
                .and_then(|key| parse(key).ok())
                .is_none_or(|key| key.id() != shortcut.id())
            {
                return;
            }
            let pressed = event.state == ShortcutState::Pressed;
            if !control
                .gate
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .accept(pressed, control.suspended.load(Ordering::SeqCst))
            {
                return;
            }
            if !app.state::<crate::lifecycle::CloseState>().is_ready() {
                return;
            }
            let phase = app.state::<SessionManager>().status().phase;
            if phase == SessionPhase::Stopping {
                return;
            }
            let running = matches!(phase, SessionPhase::Starting | SessionPhase::Running);
            let mut action = super::Action::new(if running { "stop" } else { "shortcut" }, None);
            if !running {
                // Always snapshot: the renderer may have an unsaved mode change.
                // Last-captured mode ignores this value/error rather than guessing.
                match super::foreground::current() {
                    Ok(source) => action.value = Some(source),
                    Err(error) => action.error = Some(error),
                }
            }
            super::emit(app, action);
        })
        .build();
    if app.plugin(plugin).is_err() {
        app.state::<Control>()
            .status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .integration_error =
            Some("Глобальні клавіші недоступні; використовуйте вікно або трей.".into());
    }
}
#[tauri::command]
pub fn shortcut_status(control: State<'_, Control>) -> Status {
    control
        .status
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}
#[tauri::command]
pub fn suspend_shortcut(suspended: bool, control: State<'_, Control>) {
    control.suspended.store(suspended, Ordering::SeqCst);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unavailable_stored_binding_does_not_block_other_settings_but_replacement_reverts() {
        assert_eq!(fallback(Some("old"), Some("old"), None), Some("old".into()));
        assert_eq!(
            fallback(Some("old"), Some("new"), Some("old")),
            Some("old".into())
        );
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
