//! Opt-in, no-audio diagnostic. Reads only the explicitly supplied settings directory.
use futures_util::{SinkExt, StreamExt};
use std::{path::PathBuf, time::Duration};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Error, Message},
};
use translator_core::{
    provider::{setup, TranslationConfig},
    settings::load_settings,
};
fn transport(error: &Error) -> String {
    match error {
        Error::Io(error) => format!("io kind={:?}, os={:?}", error.kind(), error.raw_os_error()),
        Error::Tls(_) => "tls".into(),
        Error::Http(response) => format!("http={}", response.status().as_u16()),
        Error::Protocol(_) => "websocket protocol".into(),
        Error::ConnectionClosed | Error::AlreadyClosed => "connection closed".into(),
        _ => "other transport error (details suppressed)".into(),
    }
}
#[tokio::main(flavor = "current_thread")]
async fn main() {
    let Some(directory) = std::env::args_os().nth(1) else {
        eprintln!("Usage: connection_diagnostic SETTINGS_DIRECTORY");
        std::process::exit(2);
    };
    let settings = match load_settings(&PathBuf::from(directory)) {
        Ok(s) => s,
        Err(_) => {
            eprintln!("Could not load settings; contents suppressed");
            std::process::exit(1);
        }
    };
    let config = TranslationConfig {
        api_key: settings.api_key.trim().into(),
        target_language: settings.target_language,
    };
    if config.api_key.is_empty()
        || !config
            .api_key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
    {
        eprintln!("Missing/invalid key; nothing sent");
        return;
    }
    let result = tokio::time::timeout(Duration::from_secs(15), async {
        let url = format!("wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent?key={}", config.api_key);
        let (mut socket, _) = match connect_async(url.as_str()).await { Ok(value) => value, Err(e) => { println!("CONNECT FAILED: {}", transport(&e)); return; } };
        println!("CONNECTED: TLS/WebSocket handshake accepted; no audio");
        if let Err(e) = socket.send(Message::Text(setup(&config).to_string().into())).await { println!("SETUP SEND FAILED: {}", transport(&e)); return; }
        loop {
            match socket.next().await {
                Some(Ok(message @ (Message::Text(_) | Message::Binary(_)))) => {
                    let Ok(value) = serde_json::from_slice::<serde_json::Value>(&message.into_data()) else { println!("INVALID JSON (body suppressed)"); return; };
                    if value.get("setupComplete").is_some() || value.get("setup_complete").is_some() { println!("READY: Live Translation model accepted; no audio"); let _ = socket.close(None).await; return; }
                    if let Some(error) = value.get("error") { println!("SETUP ERROR: code={}, status={} (message suppressed)", error.get("code").and_then(|v| v.as_i64()).unwrap_or(0), error.get("status").and_then(|v| v.as_str()).filter(|s| s.chars().all(|c| c.is_ascii_uppercase() || c == '_')).unwrap_or("unknown")); return; }
                }
                Some(Ok(Message::Close(frame))) => { println!("CLOSED: code={:?} (reason suppressed)", frame.map(|f| u16::from(f.code))); return; }
                Some(Err(e)) => { println!("SETUP RECEIVE FAILED: {}", transport(&e)); return; }
                None => { println!("SETUP RECEIVE: ended without confirmation"); return; }
                _ => {}
            }
        }
    }).await;
    if result.is_err() {
        println!("TIMEOUT: total setup exceeded 15 seconds");
    }
}
