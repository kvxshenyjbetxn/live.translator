//! Explicit no-audio check using the same path as the Settings button. No GUI actions.
use std::path::PathBuf;
use translator_core::{session::SessionManager, settings::load_settings};
#[tokio::main(flavor = "current_thread")]
async fn main() {
    let Some(path) = std::env::args_os().nth(1) else {
        eprintln!("Usage: check_connection SETTINGS_DIRECTORY");
        std::process::exit(2);
    };
    let settings = match load_settings(&PathBuf::from(path)) {
        Ok(settings) => settings,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    };
    match SessionManager::default().check_connection(settings).await {
        Ok(report) => println!(
            "PASS: {}, {} ms; session closed; no audio sent",
            report.model, report.elapsed_ms
        ),
        Err(error) => {
            eprintln!("FAIL: {error}");
            std::process::exit(1);
        }
    }
}
