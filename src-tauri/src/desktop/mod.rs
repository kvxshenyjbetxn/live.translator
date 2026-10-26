pub mod foreground;
pub mod shortcut;
pub mod tray;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Action {
    pub kind: String,
    pub value: Option<String>,
    pub error: Option<String>,
}
impl Action {
    pub fn new(kind: &str, value: Option<String>) -> Self {
        Self {
            kind: kind.into(),
            value,
            error: None,
        }
    }
}
pub fn emit(app: &AppHandle, action: Action) {
    if app.state::<crate::lifecycle::CloseState>().is_ready() {
        let _ = app.emit("desktop-action", action);
    }
}
pub fn show(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}
#[tauri::command]
pub fn show_main_window(app: AppHandle) {
    show(&app);
}

#[cfg(test)]
mod tests {
    use super::*;
    use translator_core::session::{SessionPhase, SessionStatus};
    #[test]
    fn health_never_invents_good_network_or_confuses_local_queue_with_network() {
        let mut s = SessionStatus::default();
        assert_eq!(tray::health(&s).0, tray::Color::Gray);
        s.phase = SessionPhase::Starting;
        assert_eq!(tray::health(&s).0, tray::Color::Yellow);
        s.phase = SessionPhase::Running;
        assert_eq!(tray::health(&s).0, tray::Color::Yellow);
        s.diagnostics.network_rtt_ms = Some(80.);
        assert_eq!(tray::health(&s).0, tray::Color::Green);
        s.diagnostics
            .issues
            .push(translator_core::session::diagnostics::DiagnosticIssue::PlaybackQueue);
        assert_eq!(tray::health(&s).0, tray::Color::Green);
        s.diagnostics
            .issues
            .push(translator_core::session::diagnostics::DiagnosticIssue::NetworkUnstable);
        assert_eq!(tray::health(&s).0, tray::Color::Yellow);
        s.phase = SessionPhase::Error;
        assert_eq!(tray::health(&s).0, tray::Color::Red);
    }
}
