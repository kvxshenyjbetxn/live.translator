//! Tray / shortcut action decisions and menu synchronisation (port of `src/lib/desktop.ts`).

use translator_core::{
    session::SessionPhase,
    settings::{Settings, ShortcutMode},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionKind {
    Stop,
    Toggle,
    Shortcut,
    Refresh,
    Source,
    Language,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DesktopAction {
    pub kind: ActionKind,
    pub value: Option<String>,
    pub error: Option<String>,
}

impl DesktopAction {
    pub fn new(kind: ActionKind, value: Option<String>) -> Self {
        Self {
            kind,
            value,
            error: None,
        }
    }
}

/// The model the tray menu is built from.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MenuModel {
    pub revision: u64,
    pub interface_language: String,
    pub sources: Vec<MenuSource>,
    pub selected_source_id: Option<String>,
    pub target_language: String,
    pub can_select: bool,
    pub can_start: bool,
    pub can_stop: bool,
    /// An explicit start is waiting for a favorite application to appear.
    pub waiting: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MenuSource {
    pub id: String,
    pub name: String,
}

pub struct DesktopContext<'a> {
    pub loaded: bool,
    pub closing: bool,
    pub checking: bool,
    pub recording: bool,
    pub starting: bool,
    pub waiting: bool,
    pub phase: SessionPhase,
    pub settings: &'a Settings,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Decision {
    Ignore,
    Stop,
    Refresh,
    Start(String),
    Error(String),
    Language(String),
    Source(String),
}

const UNKNOWN_APPLICATION: &str =
    "Програма для захоплення невідома. Виберіть джерело або спочатку запустіть переклад із вікна.";

pub fn decide_desktop(action: &DesktopAction, c: &DesktopContext) -> Decision {
    if !c.loaded || c.closing || c.checking || c.recording || c.phase == SessionPhase::Stopping {
        return Decision::Ignore;
    }
    let busy = c.waiting
        || c.starting
        || matches!(c.phase, SessionPhase::Starting | SessionPhase::Running);
    if action.kind == ActionKind::Stop {
        return Decision::Stop;
    }
    if matches!(action.kind, ActionKind::Toggle | ActionKind::Shortcut) {
        if busy {
            return Decision::Stop;
        }
        let source_id: Option<String> = if action.kind == ActionKind::Toggle {
            c.settings.selected_source_id.clone()
        } else if c.settings.shortcut_mode == ShortcutMode::LastCaptured {
            c.settings.last_captured_source_id.clone()
        } else {
            if let Some(error) = &action.error {
                return Decision::Error(error.clone());
            }
            action.value.clone()
        };
        return match source_id.filter(|id| !id.is_empty()) {
            Some(id) => Decision::Start(id),
            None => Decision::Error(UNKNOWN_APPLICATION.into()),
        };
    }
    if busy {
        return Decision::Ignore;
    }
    if action.kind == ActionKind::Refresh {
        return Decision::Refresh;
    }
    match (&action.kind, action.value.as_ref().filter(|value| !value.is_empty())) {
        (ActionKind::Language, Some(value)) => Decision::Language(value.clone()),
        (ActionKind::Source, Some(value)) => Decision::Source(value.clone()),
        _ => Decision::Ignore,
    }
}

/// Coalesces semantic menu updates: an identical model is never written twice.
#[derive(Default)]
pub struct MenuSync {
    written: Option<MenuModel>,
}

impl MenuSync {
    /// Returns the model that must be written, or `None` when it equals the last written one.
    pub fn changed(&mut self, model: &MenuModel) -> Option<MenuModel> {
        if self.written.as_ref() == Some(model) {
            return None;
        }
        self.written = Some(model.clone());
        Some(model.clone())
    }

    /// Forget the last write so the next update is applied (a failed write must be retried).
    pub fn invalidate(&mut self) {
        self.written = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(settings: &Settings) -> DesktopContext<'_> {
        DesktopContext {
            loaded: true,
            closing: false,
            checking: false,
            recording: false,
            starting: false,
            waiting: false,
            phase: SessionPhase::Idle,
            settings,
        }
    }
    fn shortcut(value: Option<&str>, error: Option<&str>) -> DesktopAction {
        DesktopAction {
            kind: ActionKind::Shortcut,
            value: value.map(Into::into),
            error: error.map(Into::into),
        }
    }
    fn action(kind: ActionKind, value: Option<&str>) -> DesktopAction {
        DesktopAction::new(kind, value.map(Into::into))
    }
    fn last_captured(id: &str) -> Settings {
        Settings {
            shortcut_mode: ShortcutMode::LastCaptured,
            last_captured_source_id: Some(id.into()),
            ..Settings::default()
        }
    }

    #[test]
    fn active_shortcut_uses_its_snapshot_and_last_mode_uses_the_remembered_identity() {
        let defaults = Settings::default();
        assert_eq!(
            decide_desktop(&shortcut(Some("app:/Active.app"), None), &context(&defaults)),
            Decision::Start("app:/Active.app".into())
        );
        let last = last_captured("app:/Last.app");
        assert_eq!(
            decide_desktop(&shortcut(Some("app:/Active.app"), None), &context(&last)),
            Decision::Start("app:/Last.app".into())
        );
        let selected = Settings {
            selected_source_id: Some("app:/Selected.app".into()),
            ..Settings::default()
        };
        assert_eq!(
            decide_desktop(&action(ActionKind::Toggle, None), &context(&selected)),
            Decision::Start("app:/Selected.app".into())
        );
    }

    #[test]
    fn windows_snapshot_and_foreground_errors_preserve_last_captured_behavior() {
        let browser = r"exe:c:\apps\browser.exe";
        let last_id = r"exe:c:\apps\last.exe";
        let defaults = Settings::default();
        let last = last_captured(last_id);
        assert_eq!(
            decide_desktop(&shortcut(Some(browser), None), &context(&defaults)),
            Decision::Start(browser.into())
        );
        assert_eq!(
            decide_desktop(&shortcut(None, Some("unavailable")), &context(&last)),
            Decision::Start(last_id.into())
        );
        assert_eq!(
            decide_desktop(&shortcut(None, Some("unavailable")), &context(&defaults)),
            Decision::Error("unavailable".into())
        );
    }

    #[test]
    fn errors_and_a_missing_last_application_never_fall_back_to_the_selected_source() {
        let settings = Settings {
            selected_source_id: Some("app:/Selected.app".into()),
            ..Settings::default()
        };
        assert_eq!(
            decide_desktop(&shortcut(None, Some("self")), &context(&settings)),
            Decision::Error("self".into())
        );
        assert!(matches!(
            decide_desktop(&shortcut(None, None), &context(&settings)),
            Decision::Error(_)
        ));
        let last_mode = Settings {
            shortcut_mode: ShortcutMode::LastCaptured,
            ..settings
        };
        assert!(matches!(
            decide_desktop(&shortcut(Some("app:/Active.app"), None), &context(&last_mode)),
            Decision::Error(_)
        ));
    }

    #[test]
    fn toggle_always_stops_active_or_pending_capture_even_with_foreground_failure() {
        let settings = Settings::default();
        for phase in [SessionPhase::Running, SessionPhase::Starting] {
            let mut c = context(&settings);
            c.phase = phase;
            assert_eq!(decide_desktop(&shortcut(None, Some("unknown")), &c), Decision::Stop);
        }
        let mut c = context(&settings);
        c.starting = true;
        assert_eq!(decide_desktop(&shortcut(None, None), &c), Decision::Stop);
        assert_eq!(
            decide_desktop(&action(ActionKind::Stop, None), &context(&settings)),
            Decision::Stop
        );
    }

    #[test]
    fn stopping_startup_shutdown_connection_check_and_recorder_ignore_native_actions() {
        let settings = Settings::default();
        let tweaks: [fn(&mut DesktopContext); 5] = [
            |c| c.phase = SessionPhase::Stopping,
            |c| c.loaded = false,
            |c| c.closing = true,
            |c| c.checking = true,
            |c| c.recording = true,
        ];
        for tweak in tweaks {
            let mut c = context(&settings);
            tweak(&mut c);
            assert_eq!(
                decide_desktop(&shortcut(Some("app:/Active.app"), None), &c),
                Decision::Ignore
            );
        }
    }

    #[test]
    fn waiting_is_cancelled_by_toggle_and_shortcut_and_ignores_refresh() {
        let settings = Settings::default();
        let mut c = context(&settings);
        c.waiting = true;
        assert_eq!(decide_desktop(&action(ActionKind::Toggle, None), &c), Decision::Stop);
        assert_eq!(
            decide_desktop(&shortcut(Some("app:/A.app"), None), &c),
            Decision::Stop
        );
        assert_eq!(decide_desktop(&action(ActionKind::Refresh, None), &c), Decision::Ignore);
        assert_eq!(
            decide_desktop(&action(ActionKind::Source, Some("app:/A.app")), &c),
            Decision::Ignore
        );
    }

    #[test]
    fn source_and_language_menu_changes_are_locked_during_capture() {
        let settings = Settings::default();
        let mut c = context(&settings);
        c.phase = SessionPhase::Running;
        assert_eq!(
            decide_desktop(&action(ActionKind::Source, Some("app:/A.app")), &c),
            Decision::Ignore
        );
        assert_eq!(
            decide_desktop(&action(ActionKind::Language, Some("uk")), &context(&settings)),
            Decision::Language("uk".into())
        );
    }

    #[test]
    fn menu_synchronization_skips_identical_models_and_keeps_the_newest_state() {
        let mut sync = MenuSync::default();
        let first = MenuModel {
            revision: 1,
            ..MenuModel::default()
        };
        let third = MenuModel {
            revision: 3,
            ..MenuModel::default()
        };
        assert_eq!(sync.changed(&first), Some(first.clone()));
        assert_eq!(sync.changed(&first), None);
        assert_eq!(sync.changed(&third), Some(third.clone()));
        assert_eq!(sync.changed(&third), None);
        sync.invalidate();
        assert_eq!(sync.changed(&third), Some(third));
    }
}
