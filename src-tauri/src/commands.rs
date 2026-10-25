use crate::ConfigStore;
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;
use translator_core::{
    capture,
    session::{SessionManager, SessionStatus},
    settings::{self, Language, Settings},
    sources::AudioSource,
};

#[tauri::command]
pub fn load_settings(store: State<'_, ConfigStore>) -> Result<Settings, String> {
    let _guard = store
        .lock
        .lock()
        .map_err(|_| "Налаштування тимчасово недоступні.")?;
    settings::load_settings(&store.directory).map_err(|e| e.to_string())
}
#[tauri::command]
pub fn save_settings(settings: Settings, store: State<'_, ConfigStore>) -> Result<(), String> {
    let _guard = store
        .lock
        .lock()
        .map_err(|_| "Налаштування тимчасово недоступні.")?;
    settings::save_settings(&store.directory, &settings).map_err(|e| e.to_string())
}
#[tauri::command]
pub fn config_directory(store: State<'_, ConfigStore>) -> String {
    store.directory.to_string_lossy().into_owned()
}
#[tauri::command]
pub fn open_config_directory(app: AppHandle, store: State<'_, ConfigStore>) -> Result<(), String> {
    // Only the app-selected config directory can be opened, never a renderer-supplied path.
    std::fs::create_dir_all(&store.directory)
        .map_err(|_| "Не вдалося відкрити папку налаштувань.")?;
    app.opener()
        .open_path(store.directory.to_string_lossy().into_owned(), None::<&str>)
        .map_err(|_| "Не вдалося відкрити папку налаштувань.".into())
}
#[tauri::command]
pub fn supported_languages() -> Vec<Language> {
    settings::LANGUAGES.to_vec()
}
#[tauri::command]
pub async fn list_sources() -> Result<Vec<AudioSource>, String> {
    tauri::async_runtime::spawn_blocking(capture::list_sources)
        .await
        .map_err(|_| "Не вдалося отримати список програм.")?
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub fn start_translation(
    source_id: String,
    store: State<'_, ConfigStore>,
    session: State<'_, SessionManager>,
    lifecycle: State<'_, crate::lifecycle::CloseState>,
) -> Result<u64, String> {
    if !lifecycle.is_ready() {
        return Err("Інтерфейс ще завантажується.".into());
    }
    let settings = {
        let _guard = store.lock.lock().map_err(|_| "Налаштування недоступні.")?;
        settings::load_settings(&store.directory).map_err(|e| e.to_string())?
    };
    if settings.selected_source_id.as_ref() != Some(&source_id) {
        return Err("Спочатку збережіть вибір програми.".into());
    }
    session
        .start(settings, source_id)
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn stop_translation(session: State<'_, SessionManager>) -> Result<(), String> {
    session.stop().map_err(|e| e.to_string())?;
    session.wait_for_stop().await.map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn check_connection(
    store: State<'_, ConfigStore>,
    session: State<'_, SessionManager>,
) -> Result<translator_core::provider::ConnectionReport, String> {
    let settings = {
        let _guard = store.lock.lock().map_err(|_| "Налаштування недоступні.")?;
        settings::load_settings(&store.directory).map_err(|error| error.to_string())?
    };
    session
        .check_connection(settings)
        .await
        .map_err(|error| error.to_string())
}
#[tauri::command]
pub fn session_status(session: State<'_, SessionManager>) -> SessionStatus {
    session.status()
}
#[tauri::command]
pub fn set_volumes(
    original: f32,
    translation: f32,
    session: State<'_, SessionManager>,
) -> Result<(), String> {
    session
        .set_gains(original, translation)
        .map_err(|e| e.to_string())
}
