use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter, Manager, State};
use translator_core::session::SessionManager;
#[derive(Default)]
pub struct CloseState {
    ready: AtomicBool,
    allowed: AtomicBool,
}
impl CloseState {
    pub fn should_intercept(&self) -> bool {
        self.ready.load(Ordering::SeqCst) && !self.allowed.load(Ordering::SeqCst)
    }
    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::SeqCst)
    }
}
#[tauri::command]
pub fn frontend_ready(state: State<'_, CloseState>) {
    state.ready.store(true, Ordering::SeqCst);
}
#[tauri::command]
pub fn begin_close(session: State<'_, SessionManager>) {
    session.begin_close();
}
#[tauri::command]
pub fn cancel_close(session: State<'_, SessionManager>) {
    session.cancel_close();
}
#[tauri::command]
pub async fn finish_close(
    app: AppHandle,
    session: State<'_, SessionManager>,
    state: State<'_, CloseState>,
) -> Result<(), String> {
    session.begin_close();
    session.wait_for_stop().await.map_err(|e| e.to_string())?;
    state.allowed.store(true, Ordering::SeqCst);
    app.exit(0);
    Ok(())
}
pub fn request(app: &AppHandle) {
    app.state::<SessionManager>().begin_close();
    let _ = app.emit("app-close-requested", ());
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unready_or_finished_window_does_not_trap_exit() {
        let state = CloseState::default();
        assert!(!state.should_intercept());
        state.ready.store(true, Ordering::SeqCst);
        assert!(state.should_intercept());
        state.allowed.store(true, Ordering::SeqCst);
        assert!(!state.should_intercept());
    }
}
