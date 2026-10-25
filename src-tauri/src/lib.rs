mod commands;
mod lifecycle;
use std::{path::PathBuf, sync::Mutex};
use tauri::Manager;
use translator_core::session::SessionManager;
struct ConfigStore {
    directory: PathBuf,
    lock: Mutex<()>,
}

pub fn run() {
    let app = tauri::Builder::default()
        .manage(SessionManager::default())
        .manage(lifecycle::CloseState::default())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            app.manage(ConfigStore {
                directory: app.path().app_config_dir()?,
                lock: Mutex::new(()),
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle();
                if app.state::<lifecycle::CloseState>().should_intercept() {
                    api.prevent_close();
                    lifecycle::request(app);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::load_settings,
            commands::save_settings,
            commands::config_directory,
            commands::open_config_directory,
            commands::supported_languages,
            commands::list_sources,
            commands::start_translation,
            commands::stop_translation,
            commands::session_status,
            commands::set_volumes,
            commands::check_connection,
            lifecycle::frontend_ready,
            lifecycle::begin_close,
            lifecycle::cancel_close,
            lifecycle::finish_close,
        ])
        .build(tauri::generate_context!())
        .expect("Could not initialize Live Translator");
    app.run(|app, event| {
        if let tauri::RunEvent::ExitRequested { api, .. } = event {
            if app.state::<lifecycle::CloseState>().should_intercept() {
                api.prevent_exit();
                lifecycle::request(app);
            } else {
                app.state::<SessionManager>().begin_close();
            }
        }
    });
}
