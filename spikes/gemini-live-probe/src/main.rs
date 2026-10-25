//! Disposable connection probe. No captured audio, billing changes, or key storage.
use anyhow::{bail, ensure, Result};
use serde_json::{json, Value};
use std::{
    net::TcpStream,
    time::{Duration, Instant},
};
use tungstenite::{connect, stream::MaybeTlsStream, Error, Message};

fn main() -> Result<()> {
    let key = match std::env::var("GEMINI_API_KEY") {
        Ok(value) if !value.trim().is_empty() => value,
        _ => rpassword::prompt_password("Gemini API key (hidden, not saved): ")?,
    };
    let key = key.trim();
    ensure!(!key.is_empty(), "No API key supplied");
    // Keep URLs and raw transport errors out of diagnostics: they may contain credentials.
    ensure!(
        key.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b)),
        "Unexpected key characters"
    );
    let url = format!("wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent?key={key}");
    let (mut socket, _) = match connect(url.as_str()) {
        Ok(connection) => connection,
        Err(Error::Http(response)) => bail!("WebSocket handshake rejected: HTTP {}. Check key, API restrictions and project access. No audio sent.", response.status().as_u16()),
        Err(_) => bail!("WebSocket connection failed (details suppressed to avoid leaking key). No audio sent."),
    };
    // Bound subsequent reads; TLS uses the underlying TCP stream.
    let tcp: &TcpStream = match socket.get_ref() {
        MaybeTlsStream::Plain(stream) => stream,
        MaybeTlsStream::Rustls(stream) => stream.get_ref(),
        _ => bail!("Unsupported TLS transport"),
    };
    tcp.set_read_timeout(Some(Duration::from_secs(10)))?;
    tcp.set_write_timeout(Some(Duration::from_secs(10)))?;
    socket.send(Message::Text(json!({
        "setup": {
            "model": "models/gemini-3.5-live-translate-preview",
            "generationConfig": {
                "responseModalities": ["AUDIO"],
                "translationConfig": {"targetLanguageCode": "uk", "echoTargetLanguage": false}
            }
        }
    }).to_string().into())).map_err(|_| anyhow::anyhow!("Failed to send setup (details suppressed)"))?;
    println!("Connected; waiting for Live Translation setup confirmation. No audio sent.");
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        match socket.read() {
            Ok(message @ (Message::Text(_) | Message::Binary(_))) => {
                let value: Value = serde_json::from_slice(&message.into_data())
                    .map_err(|_| anyhow::anyhow!("Invalid server response"))?;
                if value.get("setupComplete").is_some() || value.get("setup_complete").is_some() {
                    let _ = socket.close(None);
                    println!("PASS: Live Translation session accepted; target language uk. Session closed. No audio sent. This does not establish free quota or translation quality.");
                    return Ok(());
                }
                if let Some(error) = value.get("error") {
                    let code = error.get("code").and_then(Value::as_i64).unwrap_or(0);
                    let detail = error
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("no detail")
                        .replace(key, "[REDACTED]");
                    let _ = socket.close(None);
                    bail!("Server rejected setup, code {code}: {detail}. No audio sent.");
                }
            }
            Ok(Message::Close(frame)) => {
                let code = frame
                    .as_ref()
                    .map(|frame| u16::from(frame.code))
                    .unwrap_or(0);
                let reason = frame
                    .map(|frame| frame.reason.to_string())
                    .unwrap_or_default()
                    .replace(key, "[REDACTED]");
                bail!("Server closed session, WebSocket code {code}: {reason}. No audio sent.");
            }
            Ok(_) => {}
            Err(error) => {
                let detail = error.to_string().replace(key, "[REDACTED]");
                bail!("Read failed; session dropped: {detail}. No audio sent.");
            }
        }
    }
    let _ = socket.close(None);
    bail!("Setup confirmation timed out; no audio sent")
}
