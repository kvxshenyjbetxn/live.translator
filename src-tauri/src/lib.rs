mod commands;
mod desktop;
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
        .manage(desktop::shortcut::Control::default())
        .manage(desktop::tray::Control::default())
        .manage(lifecycle::CloseState::default())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            app.manage(ConfigStore {
                directory: app.path().app_config_dir()?,
                lock: Mutex::new(()),
            });
            desktop::shortcut::initialize(app.handle());
            if desktop::tray::initialize(app.handle()).is_err() {
                app.state::<desktop::shortcut::Control>()
                    .status
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .integration_error = Some(
                    "Трей недоступний у цьому середовищі; використовуйте основне вікно.".into(),
                );
            }
            let store = app.state::<ConfigStore>();
            if let Ok(settings) = translator_core::settings::load_settings(&store.directory) {
                let _ = desktop::shortcut::configure(app.handle(), &settings, || Ok(()));
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle();
                // With a tray available, the close button only hides the window; "Exit" lives in the tray.
                if app.tray_by_id("translator").is_some() {
                    api.prevent_close();
                    let _ = window.hide();
                } else if app.state::<lifecycle::CloseState>().should_intercept() {
                    api.prevent_close();
                    lifecycle::request(app);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            desktop::tray::sync_desktop,
            desktop::shortcut::shortcut_status,
            desktop::shortcut::suspend_shortcut,
            desktop::show_main_window,
            commands::is_windows,
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
